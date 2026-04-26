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
