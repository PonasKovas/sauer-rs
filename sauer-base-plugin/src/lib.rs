use crate::{
    items::{
        Items, ServerItem, apply_pickup, canpickup, delay_spawn, entity_to_item_type, get_items,
        spawntime,
    },
    options::{BasePluginOptions, get_options},
    players::{
        ClipboardData, Players, clear_player,
        combat::{apply_damage, explosion_damage, gun_damage_per_ray, kill_player},
        get_players, init_player,
        model::{init_model, set_model},
        name::{init_name, set_name},
        position::set_position,
        state::prepare_spawn,
        team::set_team,
    },
};
use bytes::Bytes;
use cube_string::IntoCubeStringLossy;
use sauer_proto::{
    Client, channel0,
    channel1::{
        self, ClipboardServer, ExplodeFx, ItemAck, ItemSpawn, RedoServer, ServInfo, ShotFx,
        SpawnServer, UndoServer,
    },
    channel2,
    data::{ClientState, GameMode, SetTeamReason},
};
use sauer_server::{
    Server,
    plugin::{Context, Plugin},
};
use tracing::info;

use std::sync::RwLock;

/// Server-wide stored map data uploaded via channel 2.
pub struct MapData(pub Option<Bytes>);

fn get_map_data(server: &Server) -> &RwLock<MapData> {
    server.extra.get().expect("MapData not initialized")
}

pub mod items;
pub mod options;
pub mod players;
mod util;
mod welcome;

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

/// Returns true if `target_cn` is alive and their life sequence matches `life_sequence`.
/// Used as an anti-cheat guard: prevents hitting a player who has already respawned.
fn check_hit(server: &Server, target_cn: u32, life_sequence: i32) -> bool {
    get_players(server).try_get(target_cn).map_or(false, |p| {
        p.client_state == ClientState::Alive
            && matches!(p.state.as_ref(), Some(s) if s.life_sequence == life_sequence)
    })
}

impl Plugin for BasePlugin {
    async fn init(&self, server: &mut Server) {
        server
            .extra
            .insert(RwLock::new(self.initial_options.clone()));
        server.extra.insert(Players::new());
        server.extra.insert(RwLock::new(MapData(None)));
        server.extra.insert(RwLock::new(Items::new()));
    }

    async fn start(&self, server: &std::sync::Arc<Server>) {
        let server = server.clone();
        tokio::spawn(async move {
            let mut interval = tokio::time::interval(std::time::Duration::from_millis(100));
            loop {
                interval.tick().await;
                // Tick item respawn timers and broadcast N_ITEMSPAWN for any that fired.
                // Decrement quad timers for all players.
                for mut entry in get_players(&server).map.iter_mut() {
                    if entry.quad_millis > 0 {
                        entry.quad_millis = (entry.quad_millis - 100).max(0);
                    }
                }

                let spawning: Vec<usize> = {
                    let mut items = get_items(&server).write().unwrap();
                    if items.notgotitems {
                        continue;
                    }
                    items
                        .list
                        .iter_mut()
                        .enumerate()
                        .filter(|(_, item)| item.spawn_time_ms > 0)
                        .filter_map(|(i, item)| {
                            item.spawn_time_ms -= 100;
                            if item.spawn_time_ms <= 0 {
                                item.spawn_time_ms = 0;
                                item.spawned = true;
                                Some(i)
                            } else {
                                None
                            }
                        })
                        .collect()
                };
                for i in spawning {
                    server.broadcast_chan1(&ItemSpawn {
                        item_index: i as i32,
                    });
                }
            }
        });
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
            let Some(mut p) = get_players(server).try_get_mut(cn) else {
                return;
            };
            if p.client_state != ClientState::Dead {
                return;
            }
            let Some(state) = p.state.as_mut() else {
                return;
            };
            // Sync server life_seq to what the client is using — the client
            // increments it locally before sending N_SPAWN, so we can't
            // predict it reliably (e.g. when N_TRYSPAWN fires between
            // welcome's N_SPAWNSTATE and the client's N_SPAWN).
            state.life_sequence = packet.life_sequence;
            state.gun_select = packet.gun_select;
            p.client_state = ClientState::Alive;
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

        server.broadcast_except_chan1(
            cn,
            &ShotFx {
                client_num: cn as i32,
                gun: packet.gun,
                id: packet.id,
                from: packet.from,
                to: packet.to,
            },
        );

        for hit in &packet.hits {
            let target_cn = hit.target_cn as u32;
            if !check_hit(server, target_cn, hit.life_sequence) {
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
            if !check_hit(server, target_cn, hit.life_sequence) {
                continue;
            }
            let damage = explosion_damage(packet.gun, hit.dist, target_cn == cn);
            apply_damage(server, target_cn, cn, packet.gun, damage, hit.dir);
        }
    }

    async fn pos(&self, ctx: &mut Context, packet: &channel0::Position) {
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
        let name = get_players(ctx.server()).get(ctx.cn()).name.clone();
        info!("[CHAT] {name}: {}", packet.text);

        ctx.server().broadcast_except_chan1(
            ctx.cn(),
            &Client {
                client_num: ctx.cn() as i32,
                packet: packet.clone(),
            },
        );
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
    async fn map(&self, ctx: &mut Context, data: &Bytes) {
        let server = ctx.server();
        let cn = ctx.cn();

        get_map_data(server).write().unwrap().0 = Some(data.clone());

        let name = get_players(server).get(cn).name.clone();
        info!("[MAP] {name} (cn {cn}) uploaded a new map.");

        server.broadcast_chan1(&channel1::ServMsg {
            text: format!("[{name} uploaded the map, use \"/getmap\" to receive it]")
                .lossy_cubestring(),
        });
    }

    async fn get_map(&self, ctx: &mut Context, _packet: &channel1::GetMap) {
        let cn = ctx.cn();
        let server = ctx.server();

        let map_bytes = get_map_data(server).read().unwrap().0.clone();
        let Some(data) = map_bytes else {
            return;
        };

        server.send_chan2(
            cn,
            &channel2::SendMap {
                data: data.to_vec(),
            },
        );

        // Resend all players' clipboards so the client has current clipboard
        // state after loading the fresh map.
        let players = get_players(server);
        let clipboard_packets: Vec<(i32, i32, Vec<u8>)> = players
            .map
            .iter()
            .filter(|e| *e.key() != cn)
            .filter_map(|e| {
                e.value()
                    .clipboard
                    .as_ref()
                    .map(|cb| (*e.key() as i32, cb.unpack_len, cb.data.clone()))
            })
            .collect();
        for (owner_cn, unpack_len, data) in clipboard_packets {
            server.send_chan1(
                cn,
                &ClipboardServer {
                    client_num: owner_cn,
                    unpack_len,
                    data,
                },
            );
        }
    }

    relay_coop!(edit_tex(channel1::EditTex));
    relay_coop!(edit_face(channel1::EditFace));
    relay_coop!(replace(channel1::Replace));
    relay_coop!(edit_v_slot(channel1::EditVSlot));
    relay_coop!(flip(channel1::Flip));
    async fn edit_ent(&self, ctx: &mut Context, packet: &channel1::EditEnt) {
        if get_options(ctx.server()).read().unwrap().mode != GameMode::CoopEdit {
            return;
        }

        let idx = packet.ent_id as usize;
        let item_placed = {
            let mut items = get_items(ctx.server()).write().unwrap();
            match entity_to_item_type(packet.type_) {
                Some(item_type) => {
                    // Entity was set to an item type — ensure the slot exists.
                    if items.list.len() <= idx {
                        items.list.resize_with(idx + 1, || ServerItem {
                            item_type,
                            spawned: false,
                            spawn_time_ms: 0,
                        });
                    }
                    items.list[idx].item_type = item_type;
                    // Don't mark spawned yet — broadcast N_ITEMSPAWN below so all
                    // clients (including the editor) transition from unspawned to spawned.
                    items.list[idx].spawned = false;
                    items.list[idx].spawn_time_ms = 0;
                    true
                }
                None => {
                    // Entity changed to a non-item type — deactivate the slot so it
                    // can no longer be picked up or trigger a respawn timer.
                    if let Some(item) = items.list.get_mut(idx) {
                        item.spawned = false;
                        item.spawn_time_ms = 0;
                    }
                    false
                }
            }
        };

        // Relay the geometry change to all other coop clients.
        ctx.server().broadcast_except_chan1(
            ctx.cn(),
            &Client {
                client_num: ctx.cn() as i32,
                packet: packet.clone(),
            },
        );

        // After the relay, tell *all* clients (including the editor) that the new
        // item slot is now spawned so they render it as a live pickup.
        if item_placed {
            get_items(ctx.server()).write().unwrap().list[idx].spawned = true;
            ctx.server().broadcast_chan1(&ItemSpawn {
                item_index: idx as i32,
            });
        }
    }
    relay_coop!(edit_var(channel1::EditVar));
    relay_coop!(edit_mat(channel1::EditMat));
    relay_coop!(edit_mode(channel1::EditMode));
    relay_coop!(rotate(channel1::Rotate));
    relay_coop!(del_cube(channel1::DelCube));
    relay_coop!(remip(channel1::Remip));

    async fn new_map(&self, ctx: &mut Context, packet: &channel1::NewMap) {
        // size >= 0 means a new blank map: clear items (no entities to wait for).
        if packet.size >= 0 {
            let mut items = get_items(ctx.server()).write().unwrap();
            items.reset();
            items.notgotitems = false;
        }
        if get_options(ctx.server()).read().unwrap().mode != GameMode::CoopEdit {
            return;
        }
        ctx.server().broadcast_except_chan1(
            ctx.cn(),
            &Client {
                client_num: ctx.cn() as i32,
                packet: packet.clone(),
            },
        );
    }

    async fn item_list(&self, ctx: &mut Context, packet: &channel1::ItemList) {
        let cn = ctx.cn();
        let server = ctx.server();

        let is_spectator = get_players(server)
            .try_get(cn)
            .map_or(false, |p| p.client_state == ClientState::Spectator);

        {
            let mut items = get_items(server).write().unwrap();
            if !items.notgotitems || is_spectator {
                return;
            }
            let num_players = server.client_cns().len();
            for item in &packet.items {
                let idx = item.item_index as usize;
                while items.list.len() <= idx {
                    // placeholder; type will be set below
                    items.list.push(items::ServerItem {
                        item_type: item.item_type,
                        spawned: false,
                        spawn_time_ms: 0,
                    });
                }
                items.list[idx].item_type = item.item_type;
                if delay_spawn(item.item_type) {
                    items.list[idx].spawn_time_ms = spawntime(item.item_type, num_players);
                    items.list[idx].spawned = false;
                } else {
                    items.list[idx].spawned = true;
                    items.list[idx].spawn_time_ms = 0;
                }
            }
            items.notgotitems = false;
        }
    }

    async fn item_pickup(&self, ctx: &mut Context, packet: &channel1::ItemPickup) {
        let cn = ctx.cn();
        let server = ctx.server();
        let idx = packet.item_index as usize;

        // Check the player is alive and can pick up the item given their state.
        let can = {
            let Some(p) = get_players(server).try_get(cn) else {
                return;
            };
            if p.client_state != ClientState::Alive {
                return;
            }
            let quad_millis = p.quad_millis;
            p.state.as_ref().map_or(false, |s| {
                get_items(server)
                    .read()
                    .unwrap()
                    .list
                    .get(idx)
                    .map_or(false, |item| {
                        item.spawned && canpickup(s, quad_millis, item.item_type)
                    })
            })
        };

        if !can {
            server.send_chan1(
                cn,
                &ItemAck {
                    item_index: packet.item_index,
                    client_num: -1,
                },
            );
            return;
        }

        let num_players = server.client_cns().len();
        let item_type = {
            let mut items = get_items(server).write().unwrap();
            let item = &mut items.list[idx];
            item.spawned = false;
            item.spawn_time_ms = spawntime(item.item_type, num_players);
            item.item_type
        };

        if let Some(mut p) = get_players(server).try_get_mut(cn) {
            let mut quad = p.quad_millis;
            if let Some(state) = p.state.as_mut() {
                apply_pickup(state, &mut quad, item_type);
            }
            p.quad_millis = quad;
        }

        server.broadcast_chan1(&ItemAck {
            item_index: packet.item_index,
            client_num: cn as i32,
        });
    }

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

    async fn copy(&self, ctx: &mut Context, packet: &channel1::CopyClient) {
        if get_options(ctx.server()).read().unwrap().mode != GameMode::CoopEdit {
            return;
        }

        // Clear any stored clipboard for this player — they are starting a new copy.
        if let Some(mut p) = get_players(ctx.server()).try_get_mut(ctx.cn()) {
            p.clipboard = None;
        }

        ctx.server().broadcast_except_chan1(
            ctx.cn(),
            &Client {
                client_num: ctx.cn() as i32,
                packet: channel1::CopyServer {
                    sel: packet.sel.clone(),
                },
            },
        );
    }

    async fn paste(&self, ctx: &mut Context, packet: &channel1::PasteClient) {
        if get_options(ctx.server()).read().unwrap().mode != GameMode::CoopEdit {
            return;
        }

        ctx.server().broadcast_except_chan1(
            ctx.cn(),
            &Client {
                client_num: ctx.cn() as i32,
                packet: channel1::PasteServer {
                    sel: packet.sel.clone(),
                },
            },
        );
    }

    async fn clipboard(&self, ctx: &mut Context, packet: &channel1::ClipboardClient) {
        let cn = ctx.cn();
        let server = ctx.server();

        let is_spectator = get_players(server)
            .try_get(cn)
            .map_or(false, |p| p.client_state == ClientState::Spectator);
        if is_spectator {
            return;
        }

        let pack_len = packet.data.len();
        if pack_len == 0 || pack_len > (1 << 16) || packet.unpack_len <= 0 {
            if let Some(mut p) = get_players(server).try_get_mut(cn) {
                p.clipboard = None;
            }
            return;
        }

        // Store the clipboard and broadcast it to all other clients so they
        // can paste from this player's clipboard immediately.
        if let Some(mut p) = get_players(server).try_get_mut(cn) {
            p.clipboard = Some(ClipboardData {
                unpack_len: packet.unpack_len,
                data: packet.data.clone(),
            });
        }

        server.broadcast_except_chan1(
            cn,
            &ClipboardServer {
                client_num: cn as i32,
                unpack_len: packet.unpack_len,
                data: packet.data.clone(),
            },
        );
    }
}
