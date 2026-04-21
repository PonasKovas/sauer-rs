use crate::map::{MapState, get_map_state};
use bytes::Bytes;
use cube_string::CubeString;
use sauer_base_plugin::items::{
    ServerItem, delay_spawn, entity_to_item_type, get_items, spawntime,
};
use sauer_proto::{
    Client,
    channel1::{self, EditMode, ServMsg},
    channel2,
};
use sauer_server::{
    Server,
    plugin::{Context, Plugin},
};
use std::sync::RwLock;
use tracing::{error, info, warn};

mod map;

pub struct DeathRun {}

impl DeathRun {
    pub fn new() -> Self {
        Self {}
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

    async fn edit_mode(&self, ctx: &mut Context, _packet: &channel1::EditMode) {
        if get_map_state(ctx.server()).read().unwrap().is_none() {
            // dont let

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

        // Parse C++ filter semantics
        if packet_filter >= 0 {
            filtermat = (packet_filter & 0xFFFF) as u16;
            filtergeom = packet_filter & !0xFFFF;

            filtermask = if (filtermat & 0x1F) != 0 {
                0x1F // MATF_VOLUME | MATF_INDEX
            } else if (filtermat & 0xE0) != 0 {
                0xE0 // MATF_CLIP
            } else {
                filtermat
            };
        }

        let mut matid;
        let mut matmask;

        if packet_mat < 0 {
            // Negative means removing the material
            matid = 0;
            matmask = filtermask;
            if (filtermat & 0x1C) == 0x0C {
                matmask &= !0xE0u16;
            } // Unset MAT_CLIP
            if (filtermat & 0x1C) == 0x08 {
                matmask &= !0x0100u16;
            } // Unset MAT_DEATH
        } else {
            matid = (packet_mat & 0xFFFF) as u16;
            matmask = if (matid & 0x1F) != 0 {
                0
            } else if (matid & 0xE0) != 0 {
                !0xE0u16
            } else {
                !matid
            };

            // Fix the bug: Sauerbraten automatically forces these bits locally,
            // but the network packet only sends the base material!
            if (matid & 0x1C) == 0x0C {
                matid |= 0x40;
            } // MAT_GLASS -> MAT_CLIP
            if (matid & 0x1C) == 0x08 {
                matid |= 0x0100;
            } // MAT_LAVA -> MAT_DEATH
        }

        const EDITMATF_EMPTY: i32 = 0x10000;
        const EDITMATF_NOTEMPTY: i32 = 0x20000;
        const EDITMATF_SOLID: i32 = 0x30000;
        const EDITMATF_NOTSOLID: i32 = 0x40000;
        const MAT_AIR: u16 = 0;

        // Recursive tree walker closure
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
                        // Check if it violates geometry filters (e.g., painting "water empty")
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
    }

    /// `N_REPLACE` – replace all occurrences of one texture with another.
    async fn replace(&self, ctx: &mut Context, packet: &channel1::Replace) {
        let mut lock = get_map_state(ctx.server()).write().unwrap();
        let Some(state) = lock.as_mut() else { return };

        let old_tex = packet.old_tex as u16;
        let new_tex = packet.new_tex as u16;

        // Recursive tree walker closure
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
            // Global replace: walk the entire octree from the root
            replace_tex(&mut state.world.root, old_tex, new_tex);
        }
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

        // EditFaceMode is represented via its C++ integer counterpart (0, 1, or 2)
        state.world.edit_face(&sel, packet.dir, packet.mode as i32);
    }

    /// `N_EDITENT` – create, modify, or delete a map entity.
    async fn edit_ent(&self, ctx: &mut Context, packet: &channel1::EditEnt) {
        let mut lock = get_map_state(ctx.server()).write().unwrap();
        let Some(state) = lock.as_mut() else { return };

        // The wire format transmits sizes as 32-bit but map entities are strictly stored
        // as 16-bit integers natively. Truncation perfectly matches vanilla C++ behaviour.
        state.world.edit_ent(
            packet.ent_id as usize,
            [packet.pos.x, packet.pos.y, packet.pos.z],
            (packet.type_ as u8).into(),
            packet.attrs.map(|x| x as i16),
        );
    }
}
