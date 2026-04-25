use crate::map_state::{
    DeathrunState, MapState, get_server_state, is_waiting_for_map, send_map_to, with_map_state_mut,
};
use bytes::Bytes;
use cube_string::{CubeString, IntoCubeStringLossy};
use sauer_proto::{
    Client,
    channel1::{self, EditMode, ServMsg},
    data::{ClientState, GameMode, PlayerState, Privilege},
};
use sauer_server::{
    Server,
    plugin::{Context, Plugin},
};
use std::sync::RwLock;
use tracing::{info, warn};

pub struct DeathrunEdit;

impl Plugin for DeathrunEdit {
    async fn init(&self, server: &mut Server) {
        server.extra.insert(RwLock::new(DeathrunState::default()));
    }

    async fn text(&self, _ctx: &mut Context, _packet: &channel1::Text) {
        // #recreate chat command intentionally removed for now, logic kept in recreate.rs
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
        }

        for cn in server.client_cns() {
            server.set_spectator(cn, true);
        }
    }

    async fn map(&self, ctx: &mut Context, data: &Bytes) {
        // Always cancel so the base handler does not store the raw bytes
        // (deathrun parses the map itself for in-memory editing).
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
                &channel1::ServMsg {
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
                    &channel1::ServMsg {
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

        // First player to complete the handshake gets Master and is unspectated
        // (the client auto-spectates itself when it sees Locked master mode).
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
                &channel1::ServMsg {
                    text: "Waiting for map upload, /sendmap to proceed.".lossy_cubestring(),
                },
            );
            ctx.cancel();
        }
    }

    // ── Coop-edit map tracking ────────────────────────────────────────────────

    async fn edit_var(&self, ctx: &mut Context, packet: &channel1::EditVar) {
        with_map_state_mut(ctx, |state| state.handle_edit_var(packet));
    }

    async fn edit_tex(&self, ctx: &mut Context, packet: &channel1::EditTex) {
        with_map_state_mut(ctx, |state| state.handle_edit_tex(packet));
    }

    async fn edit_mat(&self, ctx: &mut Context, packet: &channel1::EditMat) {
        with_map_state_mut(ctx, |state| state.handle_edit_mat(packet));
    }

    async fn del_cube(&self, ctx: &mut Context, packet: &channel1::DelCube) {
        with_map_state_mut(ctx, |state| state.handle_del_cube(packet));
    }

    async fn replace(&self, ctx: &mut Context, packet: &channel1::Replace) {
        with_map_state_mut(ctx, |state| state.handle_replace(packet));
    }

    async fn edit_face(&self, ctx: &mut Context, packet: &channel1::EditFace) {
        with_map_state_mut(ctx, |state| state.handle_edit_face(packet));
    }

    async fn edit_ent(&self, ctx: &mut Context, packet: &channel1::EditEnt) {
        with_map_state_mut(ctx, |state| state.handle_edit_ent(packet));
    }
}
