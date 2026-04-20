use crate::map::{MapState, get_map_state};
use bytes::Bytes;
use ogz_rs::data::Material;
use sauer_proto::{channel1, channel2};
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

        let tex = packet.tex as u16;
        let orient = packet.sel.orient as usize;
        let all_faces = packet.all_faces;
        let sel = &packet.sel;

        state.world.edit_sel(
            [sel.o.x, sel.o.y, sel.o.z],
            [sel.s.x, sel.s.y, sel.s.z],
            sel.grid,
            |cube| {
                if all_faces {
                    cube.textures.fill(tex);
                } else {
                    cube.textures[orient] = tex;
                }
            },
        );
    }

    /// `N_EDITM` – material edit on a selection.
    async fn edit_mat(&self, ctx: &mut Context, packet: &channel1::EditMat) {
        let mut lock = get_map_state(ctx.server()).write().unwrap();
        let Some(state) = lock.as_mut() else { return };

        let mat_bits = packet.mat as u16;
        let filter_bits = packet.filter as u16;
        let sel = &packet.sel;

        state.world.edit_sel(
            [sel.o.x, sel.o.y, sel.o.z],
            [sel.s.x, sel.s.y, sel.s.z],
            sel.grid,
            |cube| {
                let current = cube.material.to_bits();
                cube.material =
                    Material::from_bits((current & !filter_bits) | (mat_bits & filter_bits));
            },
        );
    }

    /// `N_DELCUBE` – delete (empty) cubes in a selection.
    async fn del_cube(&self, ctx: &mut Context, packet: &channel1::DelCube) {
        let mut lock = get_map_state(ctx.server()).write().unwrap();
        let Some(state) = lock.as_mut() else { return };

        let sel = &packet.sel;
        state.world.edit_sel(
            [sel.o.x, sel.o.y, sel.o.z],
            [sel.s.x, sel.s.y, sel.s.z],
            sel.grid,
            |cube| {
                cube.edges = [0x00; 12];
            },
        );
    }

    /// `N_REPLACE` – replace all occurrences of one texture with another.
    async fn replace(&self, ctx: &mut Context, packet: &channel1::Replace) {
        let mut lock = get_map_state(ctx.server()).write().unwrap();
        let Some(state) = lock.as_mut() else { return };

        let old_tex = packet.old_tex as u16;
        let new_tex = packet.new_tex as u16;

        if packet.in_sel != 0 {
            // Replace only within the selection.
            let sel = &packet.sel;
            state.world.edit_sel(
                [sel.o.x, sel.o.y, sel.o.z],
                [sel.s.x, sel.s.y, sel.s.z],
                sel.grid,
                |cube| {
                    for t in &mut cube.textures {
                        if *t == old_tex {
                            *t = new_tex;
                        }
                    }
                },
            );
        } else {
            // Global replace: walk the entire octree.
            replace_texture_global(&mut state.world.root, old_tex, new_tex);
        }
    }
}

// ── Global-replace helper ─────────────────────────────────────────────────────

fn replace_texture_global(node: &mut sauer_map::Node, old_tex: u16, new_tex: u16) {
    match node {
        sauer_map::Node::Branch(children) => {
            for child in children.iter_mut() {
                replace_texture_global(child, old_tex, new_tex);
            }
        }
        sauer_map::Node::Leaf(cube) => {
            for t in &mut cube.textures {
                if *t == old_tex {
                    *t = new_tex;
                }
            }
        }
    }
}
