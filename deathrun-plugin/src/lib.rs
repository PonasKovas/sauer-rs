use crate::map::{MapState, get_map_state};
use bytes::Bytes;
use cube_string::{CubeString, IntoCubeStringLossy};
use flate2::Compression;
use flate2::write::ZlibEncoder;
use ogz_rs::data::Variable;
use rand::seq::SliceRandom;
use sauer_base_plugin::items::{
    ServerItem, delay_spawn, entity_to_item_type, get_items, spawntime,
};
use sauer_proto::{
    Client,
    channel1::{self, EditMode, ServMsg},
    channel2,
    data::{EditVarValue, Orient, SelInfo, Vec3},
};
use sauer_server::{
    Server,
    plugin::{Context, Plugin},
};
use std::{
    io::Write,
    sync::{Arc, RwLock},
    time::Duration,
};
use tokio::time::sleep;
use tracing::{error, info, warn};

mod map;

pub struct DeathRun {}

/// Packs a sauer_map::Node into the binary format expected by Sauerbraten's clipboard `block3` structure
fn pack_cube(node: &sauer_map::Node, out: &mut Vec<u8>) {
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
        // Skip entirely empty space to optimize the network stream and rendering speed
        if let sauer_map::Node::Leaf(cube) = node {
            if cube.is_empty() {
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
            if !cube.is_empty() {
                out.push((node_pos, node_size, node.clone()));
            }
        }
    }
}

/// Generates a valid zlib-compressed clipboard packet enclosing the given node
fn generate_clipboard(node: &sauer_map::Node, size: i32) -> (i32, Vec<u8>) {
    let mut uncompressed = Vec::new();
    // block3.o = [0, 0, 0] (local internal offset)
    for val in [0i32, 0, 0] {
        uncompressed.extend_from_slice(&val.to_le_bytes());
    }
    // block3.s = [1, 1, 1] (exactly 1 cube containing the whole branch/leaf)
    for val in [1i32, 1, 1] {
        uncompressed.extend_from_slice(&val.to_le_bytes());
    }
    // block3.grid = size
    uncompressed.extend_from_slice(&size.to_le_bytes());
    // block3.orient = 0
    uncompressed.extend_from_slice(&0i32.to_le_bytes());

    pack_cube(node, &mut uncompressed);

    // vslots header terminator (4 bytes of zero: ushort index + ushort slot = 0)
    // Because we use DelCube instead of NewMap, textures are preserved in memory!
    // We don't need to append custom texture definitions.
    uncompressed.extend_from_slice(&0u32.to_le_bytes());

    let unpack_len = uncompressed.len() as i32;

    let mut encoder = ZlibEncoder::new(Vec::new(), Compression::best());
    encoder.write_all(&uncompressed).unwrap();
    let compressed = encoder.finish().unwrap();

    (unpack_len, compressed)
}

impl DeathRun {
    pub fn new() -> Self {
        Self {}
    }

    /// Transforms the map into the uploaded custom map piece by piece.
    async fn recreate_map(&self, ctx: &mut Context, divisions: i32, delay_ms: u64) {
        let state_lock = get_map_state(ctx.server()).read().unwrap();
        let state = match state_lock.as_ref() {
            Some(s) => s,
            None => {
                ctx.server().send_chan1(
                    ctx.cn(),
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

        // Randomize the chunks for an EPIC pop-in animation
        let mut rng = rand::thread_rng();
        chunks.shuffle(&mut rng);

        drop(state_lock);

        let server = Arc::clone(ctx.server());
        let cn = ctx.cn();

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

            // 1. Instantly wipe the map geometry using DelCube.
            // This leaves the map textures (vslots) and environment settings perfectly intact!
            server.broadcast_chan1(&Client {
                client_num: cn as i32,
                packet: channel1::DelCube { sel: whole_map_sel },
            });

            // 2. Hide all entities into the void temporarily so they don't float during reconstruction
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

            // 3. Incrementally construct the custom map's geometry using chunked clipboard streaming
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
                            orient: Orient::Left, // Orientation 0 ensures exact match with clipboard block
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

            // Delay slightly before injecting entities to let rendering meshes build
            sleep(Duration::from_millis(500)).await;

            // 4. Finally, place all items and custom map entities back into the world
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
}

impl Default for DeathRun {
    fn default() -> Self {
        Self::new()
    }
}

// ── helpers ───────────────────────────────────────────────────────────────────

/// Serialize the current map state and send it to `cn` over channel 2.
/// Returns `true` if a map was available and sent.
fn send_map_to(server: &Server, cn: u32) -> bool {
    let bytes = {
        let lock = get_map_state(server).read().unwrap();
        match lock.as_ref() {
            None => return false,
            Some(state) => match state.to_ogz_bytes() {
                Ok(b) => b,
                Err(e) => {
                    error!("failed to serialize map state: {e}");
                    return false;
                }
            },
        }
    };
    server.send_chan2(cn, &channel2::SendMap { data: bytes });
    true
}

// ── Plugin impl ───────────────────────────────────────────────────────────────

impl Plugin for DeathRun {
    async fn init(&self, server: &mut Server) {
        server.extra.insert(RwLock::new(None::<MapState>));
    }

    async fn text(&self, ctx: &mut Context, packet: &channel1::Text) {
        let text = packet.text.to_utf8();
        let parts: Vec<&str> = text.trim().split_whitespace().collect();
        if parts.is_empty() {
            return;
        }

        // #recreate [divisions_power] [delay_ms]
        if parts[0] == "#recreate" {
            let mut divisions_power = 3;
            let mut delay_ms = 20;

            if parts.len() > 1 {
                if let Ok(d) = parts[1].parse::<u32>() {
                    // Prevent insane allocations; clamp max divisions to 2^8 (256 per axis, 16.7M max chunks)
                    divisions_power = d.clamp(1, 8);
                }
            }
            if parts.len() > 2 {
                if let Ok(t) = parts[2].parse::<u64>() {
                    delay_ms = t;
                }
            }

            let divisions = 1 << divisions_power;

            self.recreate_map(ctx, divisions, delay_ms).await;
        }
    }

    async fn edit_mode(&self, ctx: &mut Context, _packet: &channel1::EditMode) {
        if get_map_state(ctx.server()).read().unwrap().is_none() {
            ctx.server().send_chan1(
                ctx.cn(),
                &Client {
                    client_num: ctx.cn() as i32,
                    packet: EditMode { is_editing: false },
                },
            );
            ctx.server().send_chan1(
                ctx.cn(),
                &ServMsg {
                    text: CubeString::builder()
                        .color(cube_string::Color::Yellow)
                        .push_str("Server doesnt have the map yet. /sendmap before editing")
                        .build(),
                },
            );

            ctx.cancel();
        }
    }

    /// A client uploaded a map via channel 2.  Parse it with ogz-rs, build
    /// the `World` with sauer-map, and store both.  Cancel so the base plugin
    /// doesn't also stash the raw bytes in its own `MapData`.
    async fn map(&self, ctx: &mut Context, data: &Bytes) {
        ctx.cancel();

        let cn = ctx.cn();
        let server = ctx.server();

        match MapState::from_bytes(data) {
            Ok(state) => {
                let world_size = state.world.size;
                let num_players = server.client_cns().len();

                let mut items = get_items(server).write().unwrap();
                items.reset();
                for (i, ent) in state.world.entities.iter().enumerate() {
                    if let ogz_rs::data::EntityType::GameSpecific(n) = ent.ent_type {
                        let Ok(proto_type) = sauer_proto::data::EntityType::try_from(n as i32)
                        else {
                            continue;
                        };
                        if let Some(item_type) = entity_to_item_type(proto_type) {
                            if items.list.len() <= i {
                                items.list.resize_with(i + 1, || ServerItem {
                                    item_type,
                                    spawned: false,
                                    spawn_time_ms: 0,
                                });
                            }
                            items.list[i].item_type = item_type;
                            if delay_spawn(item_type) {
                                items.list[i].spawned = false;
                                items.list[i].spawn_time_ms = spawntime(item_type, num_players);
                            } else {
                                items.list[i].spawned = true;
                                items.list[i].spawn_time_ms = 0;
                            }
                        }
                    }
                }
                items.notgotitems = false;
                drop(items);

                *get_map_state(server).write().unwrap() = Some(state);
                info!("cn {cn} uploaded a map (world_size={world_size}); state loaded");
            }
            Err(e) => {
                warn!("cn {cn} sent an invalid map: {e}");
            }
        }
    }

    /// A client connected and completed the N_CONNECT handshake.  If we have
    /// a map loaded, push it to them immediately so they receive it alongside
    /// the base-plugin's welcome sequence.
    async fn connect(&self, ctx: &mut Context, _packet: &channel1::Connect) {
        let cn = ctx.cn();
        let server = ctx.server();
        if send_map_to(server, cn) {
            info!("sent map state to cn {cn} on connect");
        }
    }

    /// The client explicitly requested the current map.  Serialize our tracked
    /// `World` state and send it.  Cancel so the base plugin does not also send
    /// its raw (untracked) copy.
    async fn get_map(&self, ctx: &mut Context, _packet: &channel1::GetMap) {
        let cn = ctx.cn();
        let server = ctx.server();

        if send_map_to(server, cn) {
            info!("sent map state to cn {cn} on N_GETMAP");
            ctx.cancel();
        }
    }

    // ── Coop-edit tracking ────────────────────────────────────────────────────

    /// `N_EDITVAR` - Sync map variables (skybox, water color, etc)
    async fn edit_var(&self, ctx: &mut Context, packet: &channel1::EditVar) {
        let mut lock = get_map_state(ctx.server()).write().unwrap();
        let Some(state) = lock.as_mut() else { return };

        let ogz_var = match &packet.val {
            EditVarValue::Int(v) => Variable::Int {
                name: packet.name.clone(),
                value: *v,
            },
            EditVarValue::Float(v) => Variable::Float {
                name: packet.name.clone(),
                value: *v,
            },
            EditVarValue::String(v) => Variable::String {
                name: packet.name.clone(),
                value: v.clone(),
            },
        };

        state.edit_var(&packet.name, ogz_var);
    }

    /// `N_EDITT` – texture edit on a selection.
    async fn edit_tex(&self, ctx: &mut Context, packet: &channel1::EditTex) {
        let mut lock = get_map_state(ctx.server()).write().unwrap();
        let Some(state) = lock.as_mut() else { return };

        let sel = sauer_map::Selection {
            o: [packet.sel.o.x, packet.sel.o.y, packet.sel.o.z],
            s: [packet.sel.s.x, packet.sel.s.y, packet.sel.s.z],
            grid: packet.sel.grid,
            orient: packet.sel.orient as u8,
            cx: packet.sel.cx,
            cxs: packet.sel.cxs,
            cy: packet.sel.cy,
            cys: packet.sel.cys,
            corner: packet.sel.corner,
        };

        state
            .world
            .set_texture(&sel, packet.tex as u16, packet.all_faces);
        state.invalidate_cache();
    }

    /// `N_EDITM` – material edit on a selection.
    async fn edit_mat(&self, ctx: &mut Context, packet: &channel1::EditMat) {
        let mut lock = get_map_state(ctx.server()).write().unwrap();
        let Some(state) = lock.as_mut() else { return };

        let sel = sauer_map::Selection {
            o: [packet.sel.o.x, packet.sel.o.y, packet.sel.o.z],
            s: [packet.sel.s.x, packet.sel.s.y, packet.sel.s.z],
            grid: packet.sel.grid,
            orient: packet.sel.orient as u8,
            cx: packet.sel.cx,
            cxs: packet.sel.cxs,
            cy: packet.sel.cy,
            cys: packet.sel.cys,
            corner: packet.sel.corner,
        };

        let packet_mat = packet.mat as i32;
        let packet_filter = packet.filter as i32;

        let mut filtermat = 0u16;
        let mut filtermask = 0u16;
        let mut filtergeom = 0i32;

        if packet_filter >= 0 {
            filtermat = (packet_filter & 0xFFFF) as u16;
            filtergeom = packet_filter & !0xFFFF;

            filtermask = if (filtermat & 0x1F) != 0 {
                0x1F
            } else if (filtermat & 0xE0) != 0 {
                0xE0
            } else {
                filtermat
            };
        }

        let mut matid;
        let mut matmask;

        if packet_mat < 0 {
            matid = 0;
            matmask = filtermask;
            if (filtermat & 0x1C) == 0x0C {
                matmask &= !0xE0u16;
            }
            if (filtermat & 0x1C) == 0x08 {
                matmask &= !0x0100u16;
            }
        } else {
            matid = (packet_mat & 0xFFFF) as u16;
            matmask = if (matid & 0x1F) != 0 {
                0
            } else if (matid & 0xE0) != 0 {
                !0xE0u16
            } else {
                !matid
            };

            if (matid & 0x1C) == 0x0C {
                matid |= 0x40;
            }
            if (matid & 0x1C) == 0x08 {
                matid |= 0x0100;
            }
        }

        const EDITMATF_EMPTY: i32 = 0x10000;
        const EDITMATF_NOTEMPTY: i32 = 0x20000;
        const EDITMATF_SOLID: i32 = 0x30000;
        const EDITMATF_NOTSOLID: i32 = 0x40000;
        const MAT_AIR: u16 = 0;

        fn apply_mat(
            node: &mut sauer_map::Node,
            matid: u16,
            matmask: u16,
            filtermat: u16,
            filtermask: u16,
            filtergeom: i32,
        ) {
            match node {
                sauer_map::Node::Leaf(cube) => {
                    let current = cube.material.to_bits();
                    if (current & filtermask) == filtermat {
                        let skip = match filtergeom {
                            EDITMATF_EMPTY => !cube.is_empty(),
                            EDITMATF_NOTEMPTY => cube.is_empty(),
                            EDITMATF_SOLID => !cube.is_entirely_solid(),
                            EDITMATF_NOTSOLID => cube.is_entirely_solid(),
                            _ => false,
                        };

                        if !skip {
                            let new_bits = if matid != MAT_AIR {
                                (current & matmask) | matid
                            } else {
                                MAT_AIR
                            };
                            cube.material = ogz_rs::data::Material::from_bits(new_bits);
                        }
                    }
                }
                sauer_map::Node::Branch(children) => {
                    for child in children.iter_mut() {
                        apply_mat(child, matid, matmask, filtermat, filtermask, filtergeom);
                    }
                }
            }
        }

        state.world.edit_sel(&sel, |node, _| {
            apply_mat(node, matid, matmask, filtermat, filtermask, filtergeom);
        });
        state.invalidate_cache();
    }

    /// `N_DELCUBE` – delete (empty) cubes in a selection.
    async fn del_cube(&self, ctx: &mut Context, packet: &channel1::DelCube) {
        let mut lock = get_map_state(ctx.server()).write().unwrap();
        let Some(state) = lock.as_mut() else { return };

        let sel = sauer_map::Selection {
            o: [packet.sel.o.x, packet.sel.o.y, packet.sel.o.z],
            s: [packet.sel.s.x, packet.sel.s.y, packet.sel.s.z],
            grid: packet.sel.grid,
            orient: packet.sel.orient as u8,
            cx: packet.sel.cx,
            cxs: packet.sel.cxs,
            cy: packet.sel.cy,
            cys: packet.sel.cys,
            corner: packet.sel.corner,
        };

        state.world.fill(&sel, false);
        state.invalidate_cache();
    }

    /// `N_REPLACE` – replace all occurrences of one texture with another.
    async fn replace(&self, ctx: &mut Context, packet: &channel1::Replace) {
        let mut lock = get_map_state(ctx.server()).write().unwrap();
        let Some(state) = lock.as_mut() else { return };

        let old_tex = packet.old_tex as u16;
        let new_tex = packet.new_tex as u16;

        fn replace_tex(node: &mut sauer_map::Node, old_tex: u16, new_tex: u16) {
            match node {
                sauer_map::Node::Leaf(cube) => {
                    for t in &mut cube.textures {
                        if *t == old_tex {
                            *t = new_tex;
                        }
                    }
                }
                sauer_map::Node::Branch(children) => {
                    for child in children.iter_mut() {
                        replace_tex(child, old_tex, new_tex);
                    }
                }
            }
        }

        if packet.in_sel != 0 {
            let sel = sauer_map::Selection {
                o: [packet.sel.o.x, packet.sel.o.y, packet.sel.o.z],
                s: [packet.sel.s.x, packet.sel.s.y, packet.sel.s.z],
                grid: packet.sel.grid,
                orient: packet.sel.orient as u8,
                cx: packet.sel.cx,
                cxs: packet.sel.cxs,
                cy: packet.sel.cy,
                cys: packet.sel.cys,
                corner: packet.sel.corner,
            };

            state.world.edit_sel(&sel, |node, _| {
                replace_tex(node, old_tex, new_tex);
            });
        } else {
            replace_tex(&mut state.world.root, old_tex, new_tex);
        }
        state.invalidate_cache();
    }

    /// `N_EDITF` – geometry face edit on a selection.
    async fn edit_face(&self, ctx: &mut Context, packet: &channel1::EditFace) {
        let mut lock = get_map_state(ctx.server()).write().unwrap();
        let Some(state) = lock.as_mut() else { return };

        let sel = sauer_map::Selection {
            o: [packet.sel.o.x, packet.sel.o.y, packet.sel.o.z],
            s: [packet.sel.s.x, packet.sel.s.y, packet.sel.s.z],
            grid: packet.sel.grid,
            orient: packet.sel.orient as u8,
            cx: packet.sel.cx as i32,
            cxs: packet.sel.cxs as i32,
            cy: packet.sel.cy as i32,
            cys: packet.sel.cys as i32,
            corner: packet.sel.corner as i32,
        };

        state.world.edit_face(&sel, packet.dir, packet.mode as i32);
        state.invalidate_cache();
    }

    /// `N_EDITENT` – create, modify, or delete a map entity.
    async fn edit_ent(&self, ctx: &mut Context, packet: &channel1::EditEnt) {
        let mut lock = get_map_state(ctx.server()).write().unwrap();
        let Some(state) = lock.as_mut() else { return };

        state.world.edit_ent(
            packet.ent_id as usize,
            [packet.pos.x, packet.pos.y, packet.pos.z],
            (packet.type_ as u8).into(),
            packet.attrs.map(|x| x as i16),
        );
    }
}
