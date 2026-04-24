pub mod entity_store;
mod map;
pub mod options;
mod recreate;

use crate::map::{DeathrunState, MapState, get_server_state};
use crate::options::DeathrunOptions;
use bytes::Bytes;
use cube_string::{CubeString, IntoCubeStringLossy};
use sauer_proto::{
    Client,
    channel1::{self, EditMode, ServMsg},
    channel2,
    data::{ClientState, Privilege},
};
use sauer_server::{
    Server,
    plugin::{Context, Plugin},
};
use std::sync::RwLock;
use tracing::{error, info, warn};

pub struct DeathRun {
    pub options: DeathrunOptions,
}

impl DeathRun {
    pub fn new(options: DeathrunOptions) -> Self {
        Self { options }
    }
}

impl Default for DeathRun {
    fn default() -> Self {
        Self::new(DeathrunOptions::default())
    }
}

fn with_map_state_mut<F>(ctx: &Context, f: F)
where
    F: FnOnce(&mut MapState),
{
    if let Some(map) = get_server_state(ctx.server()).write().unwrap().map.as_mut() {
        f(map);
    }
}

fn send_map_to(server: &Server, cn: u32) -> bool {
    let bytes = {
        let lock = get_server_state(server).read().unwrap();
        match lock.map.as_ref() {
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

impl Plugin for DeathRun {
    async fn init(&self, server: &mut Server) {
        server.extra.insert(RwLock::new(DeathrunState::default()));
    }

    async fn text(&self, _ctx: &mut Context, _packet: &channel1::Text) {
        // #recreate chat command intentionally removed for now, logic kept in recreate.rs
    }

    async fn edit_mode(&self, ctx: &mut Context, _packet: &channel1::EditMode) {
        if get_server_state(ctx.server())
            .read()
            .unwrap()
            .waiting_for_map
        {
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

    async fn try_spawn(&self, ctx: &mut Context, _packet: &channel1::TrySpawn) {
        if self.options.is_edit_server
            && get_server_state(ctx.server())
                .read()
                .unwrap()
                .waiting_for_map
        {
            ctx.cancel();
        }
    }

    async fn spawn_client(&self, ctx: &mut Context, _packet: &channel1::SpawnClient) {
        if self.options.is_edit_server
            && get_server_state(ctx.server())
                .read()
                .unwrap()
                .waiting_for_map
        {
            let cn = ctx.cn();
            let server = ctx.server();
            if let Some(mut p) = server.players.try_get_mut(cn) {
                if p.client_state != ClientState::Spectator {
                    p.client_state = ClientState::Spectator;
                    server.broadcast_chan1(&channel1::SpectatorServer {
                        client_num: cn as i32,
                        is_spectator: true,
                    });
                }
            }
            ctx.cancel();
        }
    }

    async fn map_vote(&self, ctx: &mut Context, packet: &channel1::MapVote) {
        if !self.options.is_edit_server {
            return;
        }

        let current_map = ctx.server().map_name.read().unwrap().clone();
        if packet.map_name == current_map {
            ctx.cancel();
            return;
        }

        // Update the server's map name so future welcomes are correct.
        *ctx.server().map_name.write().unwrap() = packet.map_name.clone();

        ctx.server().broadcast_chan1(&channel1::MapChange {
            map_name: packet.map_name.clone(),
            mode: sauer_proto::data::GameMode::CoopEdit,
            has_items: false,
        });

        {
            let mut state = get_server_state(ctx.server()).write().unwrap();
            state.map = None;
            state.waiting_for_map = true;
        }

        for cn in ctx.server().client_cns() {
            if let Some(mut p) = ctx.server().players.try_get_mut(cn) {
                p.client_state = ClientState::Spectator;
            }
            ctx.server().broadcast_chan1(&channel1::SpectatorServer {
                client_num: cn as i32,
                is_spectator: true,
            });
        }

        ctx.cancel();
    }

    async fn map(&self, ctx: &mut Context, data: &Bytes) {
        // Always cancel so the base handler does not store the raw bytes
        // (deathrun parses the map itself for in-memory editing).
        ctx.cancel();

        let cn = ctx.cn();
        let server = ctx.server();

        if self.options.is_edit_server {
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

                // --- temporary entity_store smoke test ---
                #[derive(Debug, serde::Serialize, serde::Deserialize)]
                struct TestData { value: u64, label: String }

                const STORE_VERSION: u8 = 0;

                {
                    let state = get_server_state(server).read().unwrap();
                    if let Some(map) = state.map.as_ref() {
                        match entity_store::read::<TestData>(&map.world, STORE_VERSION) {
                            Ok(None) => {
                                info!("[store test] nothing in map yet");
                                server.broadcast_chan1(&channel1::ServMsg {
                                    text: "[store test] no data found in map".lossy_cubestring(),
                                });
                            }
                            Ok(Some(d)) => {
                                info!("[store test] read back: {:?}", d);
                                server.broadcast_chan1(&channel1::ServMsg {
                                    text: format!(
                                        "[store test] read: value={} label='{}'",
                                        d.value, d.label
                                    )
                                    .lossy_cubestring(),
                                });
                            }
                            Err(e) => {
                                warn!("[store test] read error: {e}");
                                server.broadcast_chan1(&channel1::ServMsg {
                                    text: format!("[store test] read error: {e}").lossy_cubestring(),
                                });
                            }
                        }
                    }
                }

                {
                    let rval: u64 = rand::random();
                    let data = TestData {
                        value: rval,
                        label: format!("written at upload by cn {cn}"),
                    };
                    let mut state = get_server_state(server).write().unwrap();
                    if let Some(map) = state.map.as_mut() {
                        match entity_store::write(&mut map.world, server, cn, &data, STORE_VERSION) {
                            Ok(()) => info!("[store test] wrote {:?}", data),
                            Err(e) => warn!("[store test] write error: {e}"),
                        }
                    }
                }
                // --- end temporary test ---

                if self.options.is_edit_server {
                    let was_spectator = server
                        .players
                        .try_get(cn)
                        .map_or(false, |p| p.client_state == ClientState::Spectator);

                    if was_spectator {
                        if let Some(mut p) = server.players.try_get_mut(cn) {
                            p.client_state = ClientState::Dead;
                        }
                        server.broadcast_chan1(&channel1::SpectatorServer {
                            client_num: cn as i32,
                            is_spectator: false,
                        });
                    }

                    server.send_chan1(
                        cn,
                        &channel1::ServMsg {
                            text: "Map received. You can now spawn and edit.".lossy_cubestring(),
                        },
                    );
                }
            }
            Err(e) => warn!("cn {cn} sent an invalid map: {e}"),
        }
    }

    async fn connect(&self, ctx: &mut Context, _packet: &channel1::Connect) {
        let cn = ctx.cn();
        let server = ctx.server();

        if self.options.is_edit_server {
            // First player gets Master.
            if server.client_cns().len() == 1 {
                server.set_privilege(cn, Privilege::Master);
            }
        }

        if send_map_to(server, cn) {
            info!("sent map state to cn {cn} on connect");
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
        if self.options.is_edit_server
            && !packet.val
            && get_server_state(ctx.server())
                .read()
                .unwrap()
                .waiting_for_map
        {
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
