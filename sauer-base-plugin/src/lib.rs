use crate::{
    options::{BasePluginOptions, get_options},
    players::{
        Players, clear_player,
        combat::{apply_damage, explosion_damage, gun_damage_per_ray, kill_player},
        get_players, init_player,
        model::{init_model, set_model},
        name::{init_name, set_name},
        position::set_position,
        state::prepare_spawn,
        team::set_team,
    },
};
use sauer_proto::{
    Client, channel0,
    channel1::{self, ExplodeFx, RedoServer, ServInfo, ShotFx, SpawnServer, UndoServer},
    data::{ClientState, GameMode, SetTeamReason},
};
use sauer_server::{
    Server,
    plugin::{Context, Plugin},
};
use tracing::info;

use std::sync::RwLock;

mod error;
pub mod options;
pub mod players;
mod util;
mod welcome;

pub use error::BasePluginError;
pub use welcome::welcome;

pub struct BasePlugin {
    initial_options: BasePluginOptions,
}

impl BasePlugin {
    pub fn new() -> Self {
        Self {
            initial_options: BasePluginOptions::default(),
        }
    }

    pub fn with_options(options: BasePluginOptions) -> Self {
        Self {
            initial_options: options,
        }
    }
}

impl Default for BasePlugin {
    fn default() -> Self {
        Self::new()
    }
}

macro_rules! relay_coop {
    ($callback:ident ($struct:path)) => {
        async fn $callback(&self, ctx: &mut Context, packet: &$struct) {
            if get_options(ctx.server()).read().unwrap().mode != GameMode::CoopEdit {
                return;
            }

            let packet = Client {
                client_num: ctx.cn() as i32,
                packet: packet.clone(),
            };
            ctx.server().broadcast_except_chan1(ctx.cn(), &packet);
        }
    };
}

impl Plugin for BasePlugin {
    async fn init(&self, server: &mut Server) {
        server
            .extra
            .insert(RwLock::new(self.initial_options.clone()));
        server.extra.insert(Players::new());
    }

    async fn connected(&self, ctx: &mut Context) {
        let session_id = init_player(ctx.server(), ctx.cn());

        let options = get_options(ctx.server()).read().unwrap();
        ctx.server().send_chan1(
            ctx.cn(),
            &ServInfo {
                client_num: ctx.cn() as i32,
                protocol: sauer_proto::PROTOCOL_VERSION,
                session_id,
                has_password: options.has_password,
                server_desc: options.server_desc.clone(),
                server_auth: options.server_auth.clone(),
            },
        );
    }

    async fn disconnected(&self, ctx: &mut Context) {
        clear_player(ctx.server(), ctx.cn());

        ctx.server().broadcast_chan1(&channel1::ClientDisconnected {
            client_num: ctx.cn() as i32,
        });
    }

    async fn connect(&self, ctx: &mut Context, packet: &channel1::Connect) {
        info!("{} (cn {}) connected.", packet.name, ctx.cn());

        init_name(ctx.server(), ctx.cn(), packet.name.clone());
        init_model(ctx.server(), ctx.cn(), packet.player_model);

        let options = get_options(ctx.server()).read().unwrap();
        if options.instant_welcome {
            drop(options);
            welcome(ctx.server(), ctx.cn());
        }
    }

    async fn try_spawn(&self, ctx: &mut Context, _packet: &channel1::TrySpawn) {
        prepare_spawn(ctx.server(), ctx.cn());
    }

    async fn spawn_client(&self, ctx: &mut Context, packet: &channel1::SpawnClient) {
        let cn = ctx.cn();
        let server = ctx.server();

        let spawned_state = {
            let mut p = match get_players(server).try_get_mut(cn) {
                Some(p) => p,
                None => return,
            };
            if p.client_state != ClientState::Dead {
                return;
            }
            if p.state.is_none() {
                return;
            }
            p.client_state = ClientState::Alive;
            if let Some(s) = p.state.as_mut() {
                // Sync server life_seq to what the client is using — the client
                // increments it locally before sending N_SPAWN, so we can't
                // predict it reliably (e.g. when N_TRYSPAWN fires between
                // welcome's N_SPAWNSTATE and the client's N_SPAWN).
                s.life_sequence = packet.life_sequence;
                s.gun_select = packet.gun_select;
            }
            p.state.clone()
        };

        if let Some(state) = spawned_state {
            // Send to all OTHER clients — the spawning client already received
            // N_SPAWNSTATE and is CS_ALIVE. If we echo N_SPAWN back to them,
            // the client sets itself back to CS_SPAWNING which breaks actions.
            server.broadcast_except_chan1(
                cn,
                &Client {
                    client_num: cn as i32,
                    packet: SpawnServer { state },
                },
            );
        }
    }

    async fn suicide(&self, ctx: &mut Context, _packet: &channel1::Suicide) {
        kill_player(ctx.server(), ctx.cn());
    }

    async fn shoot(&self, ctx: &mut Context, packet: &channel1::Shoot) {
        let cn = ctx.cn();
        let server = ctx.server();

        {
            let p = get_players(server).get(cn);
            if p.client_state != ClientState::Alive {
                return;
            }
        }

        server.broadcast_chan1(&ShotFx {
            client_num: cn as i32,
            gun: packet.gun,
            id: packet.id,
            from: packet.from,
            to: packet.to,
        });

        for hit in &packet.hits {
            let target_cn = hit.target_cn as u32;

            // Validate target is alive and lifesequence matches (anti-cheat: can't hit a respawned player)
            let life_seq_ok = match get_players(server).try_get(target_cn) {
                Some(p) => {
                    p.client_state == ClientState::Alive
                        && p.state.as_ref().map(|s| s.life_sequence) == Some(hit.life_sequence)
                }
                None => false,
            };
            if !life_seq_ok {
                continue;
            }

            let damage = hit.rays * gun_damage_per_ray(packet.gun);
            apply_damage(server, target_cn, cn, packet.gun, damage, hit.dir);
        }
    }

    async fn explode(&self, ctx: &mut Context, packet: &channel1::Explode) {
        let cn = ctx.cn();
        let server = ctx.server();

        {
            let p = get_players(server).get(cn);
            if p.client_state != ClientState::Alive {
                return;
            }
        }

        server.broadcast_chan1(&ExplodeFx {
            client_num: cn as i32,
            gun: packet.gun,
            id: packet.id,
        });

        for hit in &packet.hits {
            let target_cn = hit.target_cn as u32;

            let life_seq_ok = match get_players(server).try_get(target_cn) {
                Some(p) => {
                    p.client_state == ClientState::Alive
                        && p.state.as_ref().map(|s| s.life_sequence) == Some(hit.life_sequence)
                }
                None => false,
            };
            if !life_seq_ok {
                continue;
            }

            let damage = explosion_damage(packet.gun, hit.dist, target_cn == cn);

            apply_damage(server, target_cn, cn, packet.gun, damage, hit.dir);
        }
    }

    async fn pos(&self, ctx: &mut Context, packet: &channel0::Position) {
        info!("N_POS {} (from {})", packet.client_num, ctx.cn());
        set_position(ctx.server(), ctx.cn(), packet.pos);
        ctx.server().broadcast_except_chan0(ctx.cn(), packet);
    }
    async fn teleport(&self, ctx: &mut Context, packet: &channel0::Teleport) {
        ctx.server().broadcast_except_chan0(ctx.cn(), packet);
    }
    async fn jumppad(&self, ctx: &mut Context, packet: &channel0::JumpPad) {
        ctx.server().broadcast_except_chan0(ctx.cn(), packet);
    }

    async fn text(&self, ctx: &mut Context, packet: &channel1::Text) {
        let mut packet = packet.clone();

        packet.text = packet.text.filter(cube_string::FilterOptions {
            strip_colors: true,
            preserve_whitespace: true,
            collapse_spaces: true,
            max_len: Some(5000),
        });

        let name = get_players(ctx.server()).get(ctx.cn()).name.clone();
        info!("[CHAT] {name}: {}", packet.text);

        ctx.server().broadcast_chan1(&Client {
            client_num: ctx.cn() as i32,
            packet,
        });
    }
    async fn switch_name(&self, ctx: &mut Context, packet: &channel1::SwitchName) {
        set_name(ctx.server(), ctx.cn(), packet.name.clone());
    }
    async fn switch_model(&self, ctx: &mut Context, packet: &channel1::SwitchModel) {
        set_model(ctx.server(), ctx.cn(), packet.model);
    }
    async fn switch_team(&self, ctx: &mut Context, packet: &channel1::SwitchTeam) {
        set_team(
            ctx.server(),
            ctx.cn(),
            packet.team.clone(),
            SetTeamReason::Switched,
        );
    }

    async fn ping(&self, ctx: &mut Context, packet: &channel1::Ping) {
        ctx.server()
            .send_chan1(ctx.cn(), &channel1::Pong { ping: packet.time });
    }
    async fn client_ping(&self, ctx: &mut Context, packet: &channel1::ClientPing) {
        ctx.server().broadcast_except_chan1(
            ctx.cn(),
            &Client {
                client_num: ctx.cn() as i32,
                packet: packet.clone(),
            },
        );
    }
    async fn taunt(&self, ctx: &mut Context, packet: &channel1::Taunt) {
        let packet = Client {
            client_num: ctx.cn() as i32,
            packet: packet.clone(),
        };
        ctx.server().broadcast_except_chan1(ctx.cn(), &packet);
    }
    async fn sound(&self, ctx: &mut Context, packet: &channel1::Sound) {
        let packet = Client {
            client_num: ctx.cn() as i32,
            packet: packet.clone(),
        };
        ctx.server().broadcast_except_chan1(ctx.cn(), &packet);
    }
    relay_coop!(edit_tex(channel1::EditTex));
    relay_coop!(edit_face(channel1::EditFace));
    relay_coop!(replace(channel1::Replace));
    relay_coop!(edit_v_slot(channel1::EditVSlot));
    relay_coop!(flip(channel1::Flip));
    relay_coop!(edit_ent(channel1::EditEnt));
    relay_coop!(edit_var(channel1::EditVar));
    relay_coop!(edit_mat(channel1::EditMat));
    relay_coop!(edit_mode(channel1::EditMode));
    relay_coop!(rotate(channel1::Rotate));
    relay_coop!(del_cube(channel1::DelCube));
    relay_coop!(remip(channel1::Remip));
    relay_coop!(new_map(channel1::NewMap));

    async fn undo(&self, ctx: &mut Context, packet: &channel1::UndoClient) {
        if get_options(ctx.server()).read().unwrap().mode != GameMode::CoopEdit {
            return;
        }

        let packet = UndoServer {
            client_num: ctx.cn() as i32,
            unpack_len: packet.unpack_len,
            data: packet.data.clone(),
        };
        ctx.server().broadcast_except_chan1(ctx.cn(), &packet);
    }
    async fn redo(&self, ctx: &mut Context, packet: &channel1::RedoClient) {
        if get_options(ctx.server()).read().unwrap().mode != GameMode::CoopEdit {
            return;
        }

        let packet = RedoServer {
            client_num: ctx.cn() as i32,
            unpack_len: packet.unpack_len,
            data: packet.data.clone(),
        };
        ctx.server().broadcast_except_chan1(ctx.cn(), &packet);
    }
}
