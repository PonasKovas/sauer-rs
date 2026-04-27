#![allow(dead_code)]
use crate::map_state::get_server_state;
use cube_string::IntoCubeStringLossy;
use flate2::{Compression, write::ZlibEncoder};
use rand::seq::SliceRandom;
use sauer_proto::{
    Client, channel1,
    data::{Orient, SelInfo, Vec3},
};
use sauer_server::Server;
use std::{io::Write, sync::Arc, time::Duration};
use tokio::time::sleep;

/// Packs a sauer_map::Node into the binary format expected by Sauerbraten's clipboard `block3` structure
pub(crate) fn pack_cube(node: &sauer_map::Node, out: &mut Vec<u8>) {
    match node {
        sauer_map::Node::Branch(children) => {
            out.push(0xFF);
            for child in children.as_ref() {
                pack_cube(child, out);
            }
        }
        sauer_map::Node::Leaf(cube) => {
            let mat_bits = cube.material.to_bits();
            out.push((mat_bits & 0xFF) as u8);
            out.push((mat_bits >> 8) as u8);
            out.extend_from_slice(&cube.edges);
            for &tex in &cube.textures {
                out.extend_from_slice(&tex.to_le_bytes());
            }
        }
    }
}

/// Recursively splits the map into volumetric chunks bounded by `chunk_size_limit`
fn collect_chunks(
    node: &sauer_map::Node,
    node_pos: [i32; 3],
    node_size: i32,
    chunk_size_limit: i32,
    out: &mut Vec<([i32; 3], i32, sauer_map::Node)>,
) {
    if node_size <= chunk_size_limit {
        if let sauer_map::Node::Leaf(cube) = node {
            if cube.is_empty() && cube.material == ogz_rs::data::Material::default() {
                return;
            }
        }
        out.push((node_pos, node_size, node.clone()));
        return;
    }

    match node {
        sauer_map::Node::Branch(children) => {
            let child_size = node_size / 2;
            for i in 0..8 {
                let cx = (i & 1) as i32;
                let cy = ((i >> 1) & 1) as i32;
                let cz = ((i >> 2) & 1) as i32;
                let child_pos = [
                    node_pos[0] + cx * child_size,
                    node_pos[1] + cy * child_size,
                    node_pos[2] + cz * child_size,
                ];
                collect_chunks(&children[i], child_pos, child_size, chunk_size_limit, out);
            }
        }
        sauer_map::Node::Leaf(cube) => {
            if !(cube.is_empty() && cube.material == ogz_rs::data::Material::default()) {
                out.push((node_pos, node_size, node.clone()));
            }
        }
    }
}

/// Chooses the smallest power-of-2 grid so no axis exceeds 8 cells.
/// Keeping cells ≤ 8 per axis (≤ 512 total) makes both DelCube and paste fast.
fn choose_grid(spans: [i32; 3]) -> i32 {
    let max_span = spans.iter().copied().max().unwrap_or(1).max(1);
    let mut g = 1i32;
    while (max_span + g - 1) / g > 8 {
        g <<= 1;
    }
    g
}

/// Walks the octree to find the node that occupies (target_pos, target_size).
/// If a Leaf is encountered before reaching target_size, that Leaf is returned
/// (it represents uniform geometry at all sub-sizes).
fn extract_node_at(
    node: &sauer_map::Node,
    node_pos: [i32; 3],
    node_size: i32,
    target_pos: [i32; 3],
    target_size: i32,
) -> sauer_map::Node {
    if node_size == target_size {
        return node.clone();
    }
    match node {
        sauer_map::Node::Leaf(cube) => sauer_map::Node::Leaf(*cube),
        sauer_map::Node::Branch(children) => {
            let half = node_size / 2;
            let cx = usize::from(target_pos[0] >= node_pos[0] + half);
            let cy = usize::from(target_pos[1] >= node_pos[1] + half);
            let cz = usize::from(target_pos[2] >= node_pos[2] + half);
            let child_pos = [
                node_pos[0] + cx as i32 * half,
                node_pos[1] + cy as i32 * half,
                node_pos[2] + cz as i32 * half,
            ];
            extract_node_at(&children[cx | (cy << 1) | (cz << 2)], child_pos, half, target_pos, target_size)
        }
    }
}

/// Walks both octrees and returns the tight bounding box of nodes that actually differ.
///
/// When one side is a Leaf and the other is a Branch, we recurse treating the Leaf as 8
/// uniform children. This means `Leaf(air)` vs `Branch([Leaf(air)×7, Leaf(solid)])` only
/// marks the one solid child, not the entire octant — critical because apply_region
/// subdivides Leaf(air) into Branch([Leaf(air); 8]) as a traversal side-effect.
pub(crate) fn diff_bounds(
    a: &sauer_map::Node,
    b: &sauer_map::Node,
    pos: [i32; 3],
    size: i32,
    out_min: &mut [i32; 3],
    out_max: &mut [i32; 3],
) {
    if a == b {
        return;
    }
    let half = size / 2;
    match (a, b) {
        (sauer_map::Node::Branch(ac), sauer_map::Node::Branch(bc)) => {
            for i in 0..8 {
                let cx = (i & 1) as i32;
                let cy = ((i >> 1) & 1) as i32;
                let cz = ((i >> 2) & 1) as i32;
                let child_pos = [pos[0] + cx * half, pos[1] + cy * half, pos[2] + cz * half];
                diff_bounds(&ac[i], &bc[i], child_pos, half, out_min, out_max);
            }
        }
        (sauer_map::Node::Branch(ac), sauer_map::Node::Leaf(bl)) => {
            // Treat b-Leaf as 8 uniform children and recurse
            let b_child = sauer_map::Node::Leaf(*bl);
            for i in 0..8 {
                let cx = (i & 1) as i32;
                let cy = ((i >> 1) & 1) as i32;
                let cz = ((i >> 2) & 1) as i32;
                let child_pos = [pos[0] + cx * half, pos[1] + cy * half, pos[2] + cz * half];
                diff_bounds(&ac[i], &b_child, child_pos, half, out_min, out_max);
            }
        }
        (sauer_map::Node::Leaf(al), sauer_map::Node::Branch(bc)) => {
            // Treat a-Leaf as 8 uniform children and recurse
            let a_child = sauer_map::Node::Leaf(*al);
            for i in 0..8 {
                let cx = (i & 1) as i32;
                let cy = ((i >> 1) & 1) as i32;
                let cz = ((i >> 2) & 1) as i32;
                let child_pos = [pos[0] + cx * half, pos[1] + cy * half, pos[2] + cz * half];
                diff_bounds(&a_child, &bc[i], child_pos, half, out_min, out_max);
            }
        }
        (sauer_map::Node::Leaf(_), sauer_map::Node::Leaf(_)) => {
            // Two different Leaves at this position (equal case caught by a==b above)
            for i in 0..3 {
                if pos[i] < out_min[i] {
                    out_min[i] = pos[i];
                }
                if pos[i] + size > out_max[i] {
                    out_max[i] = pos[i] + size;
                }
            }
        }
    }
}

/// Builds ONE multi-cell clipboard covering the entire min..max region, exactly like a
/// vanilla player copy-paste. Returns (padded_min, s, grid, unpack_len, compressed).
/// `s` is [sx, sy, sz] cell counts — use the same values for the companion DelCube sel.
pub(crate) fn generate_region_clipboard(
    root: &sauer_map::Node,
    world_size: i32,
    min: [i32; 3],
    max: [i32; 3],
) -> ([i32; 3], [i32; 3], i32, i32, Vec<u8>) {
    let spans = [max[0] - min[0], max[1] - min[1], max[2] - min[2]];
    let grid = choose_grid(spans);

    let padded_min: [i32; 3] = std::array::from_fn(|i| min[i].div_euclid(grid) * grid);
    let padded_max: [i32; 3] = std::array::from_fn(|i| {
        (((max[i] + grid - 1).div_euclid(grid)) * grid).min(world_size)
    });

    let sx = (padded_max[0] - padded_min[0]) / grid;
    let sy = (padded_max[1] - padded_min[1]) / grid;
    let sz = (padded_max[2] - padded_min[2]) / grid;

    let mut uncompressed = Vec::new();
    for &v in &padded_min { uncompressed.extend_from_slice(&v.to_le_bytes()); }
    for &v in &[sx, sy, sz]  { uncompressed.extend_from_slice(&v.to_le_bytes()); }
    uncompressed.extend_from_slice(&grid.to_le_bytes());
    uncompressed.extend_from_slice(&0i32.to_le_bytes()); // Orient::Left

    // Pack matching vanilla loopxyz order for Orient::Left (d=0, D=0, R=1, C=2):
    //   outer z → s[D]=s[0]=sx,  middle y → s[C]=s[2]=sz,  inner x → s[R]=s[1]=sy
    // blockcube maps: world_x = o.x+z*grid, world_y = o.y+x*grid, world_z = o.z+y*grid
    // So: xi↔z, yi↔x, zi↔y → loop order is xi (outer), zi (middle), yi (inner).
    for xi in 0..sx {
        for zi in 0..sz {
            for yi in 0..sy {
                let cell_pos = [
                    padded_min[0] + xi * grid,
                    padded_min[1] + yi * grid,
                    padded_min[2] + zi * grid,
                ];
                let node = extract_node_at(root, [0, 0, 0], world_size, cell_pos, grid);
                pack_cube(&node, &mut uncompressed);
            }
        }
    }
    uncompressed.extend_from_slice(&0u32.to_le_bytes()); // vslots terminator

    let unpack_len = uncompressed.len() as i32;
    let mut encoder = ZlibEncoder::new(Vec::new(), Compression::best());
    encoder.write_all(&uncompressed).unwrap();
    let compressed = encoder.finish().unwrap();

    eprintln!(
        "[clipboard] grid={} cells={}x{}x{}={} uncompressed={} compressed={}",
        grid, sx, sy, sz, sx * sy * sz, unpack_len, compressed.len()
    );

    (padded_min, [sx, sy, sz], grid, unpack_len, compressed)
}

/// Generates a valid zlib-compressed clipboard packet enclosing the given node
pub(crate) fn generate_clipboard(node: &sauer_map::Node, size: i32) -> (i32, Vec<u8>) {
    let mut uncompressed = Vec::new();
    for val in [0i32, 0, 0] {
        uncompressed.extend_from_slice(&val.to_le_bytes());
    }
    for val in [1i32, 1, 1] {
        uncompressed.extend_from_slice(&val.to_le_bytes());
    }
    uncompressed.extend_from_slice(&size.to_le_bytes());
    uncompressed.extend_from_slice(&0i32.to_le_bytes());

    pack_cube(node, &mut uncompressed);
    uncompressed.extend_from_slice(&0u32.to_le_bytes());

    let unpack_len = uncompressed.len() as i32;
    let mut encoder = ZlibEncoder::new(Vec::new(), Compression::best());
    encoder.write_all(&uncompressed).unwrap();
    let compressed = encoder.finish().unwrap();

    (unpack_len, compressed)
}

/// Transforms the map into the uploaded custom map piece by piece.
pub fn spawn_recreate_map(server: Arc<Server>, cn: u32, divisions: i32, delay_ms: u64) {
    let state_lock = get_server_state(&server).read().unwrap();
    let state = match state_lock.map.as_ref() {
        Some(s) => s,
        None => {
            server.send_chan1(
                cn,
                &channel1::ServMsg {
                    text: "No custom map uploaded to recreate!".lossy_cubestring(),
                },
            );
            return;
        }
    };

    let map_size = state.world.size;
    let entities = state.world.entities.clone();
    let chunk_size_limit = map_size / divisions;

    let mut chunks = Vec::new();
    collect_chunks(
        &state.world.root,
        [0, 0, 0],
        map_size,
        chunk_size_limit,
        &mut chunks,
    );

    let mut rng = rand::thread_rng();
    chunks.shuffle(&mut rng);

    drop(state_lock);

    tokio::spawn(async move {
        server.broadcast_chan1(&channel1::ServMsg {
            text: "Morphing map sequence initiated...".lossy_cubestring(),
        });

        let whole_map_sel = SelInfo {
            o: Vec3 { x: 0, y: 0, z: 0 },
            s: Vec3 { x: 2, y: 2, z: 2 },
            grid: map_size / 2,
            orient: Orient::Left,
            cx: 0,
            cxs: 2,
            cy: 0,
            cys: 2,
            corner: 0,
        };

        server.broadcast_chan1(&Client {
            client_num: cn as i32,
            packet: channel1::DelCube { sel: whole_map_sel },
        });

        for i in 0..entities.len() {
            server.broadcast_chan1(&Client {
                client_num: cn as i32,
                packet: channel1::EditEnt {
                    ent_id: i as i32,
                    pos: Vec3 {
                        x: 0.0,
                        y: 0.0,
                        z: 0.0,
                    },
                    type_: sauer_proto::data::EntityType::Empty,
                    attrs: [0; 5],
                },
            });
        }

        sleep(Duration::from_millis(500)).await;

        for (pos, size, chunk_node) in chunks {
            let (unpack_len, compressed) = generate_clipboard(&chunk_node, size);

            server.broadcast_chan1(&channel1::ClipboardServer {
                client_num: cn as i32,
                unpack_len,
                data: compressed,
            });

            server.broadcast_chan1(&Client {
                client_num: cn as i32,
                packet: channel1::PasteServer {
                    sel: SelInfo {
                        o: Vec3 {
                            x: pos[0],
                            y: pos[1],
                            z: pos[2],
                        },
                        s: Vec3 { x: 1, y: 1, z: 1 },
                        grid: size,
                        orient: Orient::Left,
                        cx: 0,
                        cxs: 1,
                        cy: 0,
                        cys: 1,
                        corner: 0,
                    },
                },
            });

            if delay_ms > 0 {
                sleep(Duration::from_millis(delay_ms)).await;
            }
        }

        sleep(Duration::from_millis(500)).await;

        for (i, ent) in entities.into_iter().enumerate() {
            let ent_type =
                match sauer_proto::data::EntityType::try_from(u8::from(ent.ent_type) as i32) {
                    Ok(t) => t,
                    Err(_) => continue,
                };
            server.broadcast_chan1(&Client {
                client_num: cn as i32,
                packet: channel1::EditEnt {
                    ent_id: i as i32,
                    pos: Vec3 {
                        x: ent.position[0],
                        y: ent.position[1],
                        z: ent.position[2],
                    },
                    type_: ent_type,
                    attrs: ent.attrs.map(|x| x as i32),
                },
            });
        }

        server.broadcast_chan1(&channel1::ServMsg {
            text: "Map morphing reconstruction complete!".lossy_cubestring(),
        });
    });
}
