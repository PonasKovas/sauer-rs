use crate::map_state::{
    DeathrunState, MapState, get_server_state, is_waiting_for_map, send_map_to, with_map_state_mut,
};
use crate::trap::{
    check_and_rebuild_traps, cmd_deltrap, cmd_newtrap, intersects_bounds, intersects_edit_face,
    intersects_sel,
};
use bytes::Bytes;
use cube_string::{CubeString, IntoCubeStringLossy};
use dashmap::DashMap;
use sauer_map::raycast::raycast;
use sauer_proto::{
    Client, channel0,
    channel1::{self, EditMode, ServMsg},
    data::{ClientState, Direction, GameMode, PlayerState, Privilege},
};
use sauer_server::{
    Server,
    plugin::{Context, Plugin},
};
use std::sync::RwLock;
use tracing::{info, warn};

pub struct DeathrunEdit {
    player_dirs: DashMap<u32, Direction>,
}

impl DeathrunEdit {
    pub fn new() -> Self {
        Self {
            player_dirs: DashMap::new(),
        }
    }
}

impl Plugin for DeathrunEdit {
    async fn init(&self, server: &mut Server) {
        server.extra.insert(RwLock::new(DeathrunState::default()));
    }

    async fn pos(&self, _ctx: &mut Context, packet: &channel0::Position) {
        self.player_dirs.insert(packet.client_num, packet.dir);
    }

    async fn text(&self, ctx: &mut Context, packet: &channel1::Text) {
        let cmd = packet.text.strip_colors().to_utf8();
        let cmd = cmd.trim();
        if cmd != "#newtrap" && cmd != "#deltrap" {
            return;
        }
        ctx.cancel();

        let cn = ctx.cn();
        let server = ctx.server().clone();

        let (pos, is_editing) = match server.players.try_get(cn) {
            Some(p) => (p.position, p.client_state == ClientState::Editing),
            None => return,
        };

        if !is_editing {
            server.send_chan1(
                cn,
                &ServMsg {
                    text: "Must be in edit mode to place a trap.".lossy_cubestring(),
                },
            );
            return;
        }

        let dir = match self.player_dirs.get(&cn) {
            Some(d) => *d,
            None => return,
        };

        let yaw_rad = (dir.yaw as f32).to_radians();
        let pitch_rad = (dir.pitch as f32).to_radians();
        let ray = [
            -yaw_rad.sin() * pitch_rad.cos(),
            yaw_rad.cos() * pitch_rad.cos(),
            pitch_rad.sin(),
        ];
        let origin = [pos.x, pos.y, pos.z + 14.0];

        let hit_orient;
        let hit_pt;
        {
            let state = get_server_state(&server).read().unwrap();
            let map = match state.map.as_ref() {
                None => return,
                Some(m) => m,
            };

            let hit = match raycast(&map.world, origin, ray, 2048.0) {
                None => return,
                Some(h) => h,
            };
            hit_orient = hit.orient;
            hit_pt = [
                origin[0] + ray[0] * hit.dist,
                origin[1] + ray[1] * hit.dist,
                origin[2] + ray[2] * hit.dist,
            ];
        }

        if cmd == "#deltrap" {
            cmd_deltrap(ctx, hit_pt);
        } else if cmd == "#newtrap" {
            cmd_newtrap(ctx, hit_pt, hit_orient);
        }
    }

    async fn edit_mode(&self, ctx: &mut Context, _packet: &channel1::EditMode) {
        if is_waiting_for_map(ctx.server()) {
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
                        .push_str("Waiting for map upload.")
                        .build(),
                },
            );
            ctx.cancel();
        }
    }

    async fn on_spawn(&self, ctx: &mut Context, _state: &mut PlayerState) {
        if is_waiting_for_map(ctx.server()) {
            ctx.cancel();
        }
    }

    async fn map_changed(&self, ctx: &mut Context, _map_name: &CubeString, _mode: GameMode) {
        let server = ctx.server();
        {
            let mut state = get_server_state(server).write().unwrap();
            state.map = None;
            state.waiting_for_map = true;
            state.traps.clear();
        }
        for cn in server.client_cns() {
            server.set_spectator(cn, true);
        }
    }

    async fn map(&self, ctx: &mut Context, data: &Bytes) {
        ctx.cancel();
        let cn = ctx.cn();
        let server = ctx.server();

        let is_spectator = server
            .players
            .try_get(cn)
            .map_or(false, |p| p.client_state == ClientState::Spectator);
        let is_master = server.get_privilege(cn) >= Privilege::Master;

        if is_spectator && !is_master {
            server.send_chan1(
                cn,
                &ServMsg {
                    text: "You must be unspectated to upload a map.".lossy_cubestring(),
                },
            );
            return;
        }

        match MapState::from_bytes(data) {
            Ok(state) => {
                let world_size = state.world.size;
                state.update_server_items(server);
                {
                    let mut s = get_server_state(server).write().unwrap();
                    s.map = Some(state);
                    s.waiting_for_map = false;
                }
                info!("cn {cn} uploaded a map (world_size={world_size}); state loaded");
                server.set_spectator(cn, false);
                server.send_chan1(
                    cn,
                    &ServMsg {
                        text: "Map received. You can now spawn and edit.".lossy_cubestring(),
                    },
                );
            }
            Err(e) => warn!("cn {cn} sent an invalid map: {e}"),
        }
    }

    async fn on_client_welcomed(&self, ctx: &mut Context) {
        let cn = ctx.cn();
        let server = ctx.server();
        if server.client_count() == 1 {
            server.set_privilege(cn, Privilege::Master);
            server.set_spectator(cn, false);
        }
        if send_map_to(server, cn) {
            info!("sent map state to cn {cn} after welcome");
        }
    }

    async fn get_map(&self, ctx: &mut Context, _packet: &channel1::GetMap) {
        let cn = ctx.cn();
        if send_map_to(ctx.server(), cn) {
            info!("sent map state to cn {cn} on N_GETMAP");
            ctx.cancel();
        }
    }

    async fn spectator(&self, ctx: &mut Context, packet: &channel1::SpectatorClient) {
        if !packet.val && is_waiting_for_map(ctx.server()) {
            ctx.server().send_chan1(
                ctx.cn(),
                &ServMsg {
                    text: "Waiting for map upload, /sendmap to proceed.".lossy_cubestring(),
                },
            );
            ctx.cancel();
        }
    }

    // ── Coop-edit map tracking + Trap immunities ────────────────────────────────────────────────

    async fn edit_tex(&self, ctx: &mut Context, packet: &channel1::EditTex) {
        with_map_state_mut(ctx, |state| state.handle_edit_tex(packet));
        check_and_rebuild_traps(ctx, |trap| intersects_sel(&packet.sel, trap));
    }

    async fn edit_mat(&self, ctx: &mut Context, packet: &channel1::EditMat) {
        with_map_state_mut(ctx, |state| state.handle_edit_mat(packet));
        check_and_rebuild_traps(ctx, |trap| intersects_sel(&packet.sel, trap));
    }

    async fn del_cube(&self, ctx: &mut Context, packet: &channel1::DelCube) {
        with_map_state_mut(ctx, |state| state.handle_del_cube(packet));
        check_and_rebuild_traps(ctx, |trap| intersects_sel(&packet.sel, trap));
    }

    async fn replace(&self, ctx: &mut Context, packet: &channel1::Replace) {
        with_map_state_mut(ctx, |state| state.handle_replace(packet));
        check_and_rebuild_traps(ctx, |trap| {
            if packet.in_sel == 0 {
                true // Matches entire map root
            } else {
                intersects_sel(&packet.sel, trap)
            }
        });
    }

    async fn edit_face(&self, ctx: &mut Context, packet: &channel1::EditFace) {
        with_map_state_mut(ctx, |state| state.handle_edit_face(packet));
        check_and_rebuild_traps(ctx, |trap| {
            intersects_edit_face(&packet.sel, packet.dir, trap)
        });
    }

    async fn edit_ent(&self, ctx: &mut Context, packet: &channel1::EditEnt) {
        with_map_state_mut(ctx, |state| state.handle_edit_ent(packet));
    }

    async fn edit_v_slot(&self, ctx: &mut Context, packet: &channel1::EditVSlot) {
        check_and_rebuild_traps(ctx, |trap| intersects_sel(&packet.sel, trap));
    }

    async fn flip(&self, ctx: &mut Context, packet: &channel1::Flip) {
        check_and_rebuild_traps(ctx, |trap| intersects_sel(&packet.sel, trap));
    }

    async fn rotate(&self, ctx: &mut Context, packet: &channel1::Rotate) {
        check_and_rebuild_traps(ctx, |trap| intersects_sel(&packet.sel, trap));
    }

    async fn paste(&self, ctx: &mut Context, packet: &channel1::PasteClient) {
        let cn = ctx.cn();
        let server = ctx.server().clone();

        if let Some(p) = server.players.try_get(cn) {
            if let Some(cb) = &p.clipboard {
                let cb_data = cb.data.clone();
                // Safely update server map octree tracking memory
                with_map_state_mut(ctx, |state| state.handle_paste(packet, &cb_data));

                if let Some((cb_sel, _)) = crate::block_reader::decode_clipboard(&cb_data) {
                    let min = [packet.sel.o.x, packet.sel.o.y, packet.sel.o.z];
                    let max = [
                        min[0] + cb_sel.s.x * packet.sel.grid,
                        min[1] + cb_sel.s.y * packet.sel.grid,
                        min[2] + cb_sel.s.z * packet.sel.grid,
                    ];
                    crate::trap::check_and_rebuild_traps(ctx, |trap| {
                        crate::trap::intersects_bounds(min, max, trap)
                    });
                    return;
                }
            }
        }
        crate::trap::check_and_rebuild_traps(ctx, |_| true);
    }

    async fn undo(&self, ctx: &mut Context, packet: &channel1::UndoClient) {
        with_map_state_mut(ctx, |state| state.handle_undo_redo(&packet.data));

        if let Some((sel, _)) = crate::block_reader::decode_undo(&packet.data) {
            let max = [
                sel.o.x + sel.s.x * sel.grid,
                sel.o.y + sel.s.y * sel.grid,
                sel.o.z + sel.s.z * sel.grid,
            ];
            crate::trap::check_and_rebuild_traps(ctx, |trap| {
                crate::trap::intersects_bounds([sel.o.x, sel.o.y, sel.o.z], max, trap)
            });
        } else {
            crate::trap::check_and_rebuild_traps(ctx, |_| true);
        }
    }

    async fn redo(&self, ctx: &mut Context, packet: &channel1::RedoClient) {
        with_map_state_mut(ctx, |state| state.handle_undo_redo(&packet.data));

        if let Some((sel, _)) = crate::block_reader::decode_undo(&packet.data) {
            let max = [
                sel.o.x + sel.s.x * sel.grid,
                sel.o.y + sel.s.y * sel.grid,
                sel.o.z + sel.s.z * sel.grid,
            ];
            crate::trap::check_and_rebuild_traps(ctx, |trap| {
                crate::trap::intersects_bounds([sel.o.x, sel.o.y, sel.o.z], max, trap)
            });
        } else {
            crate::trap::check_and_rebuild_traps(ctx, |_| true);
        }
    }
}
