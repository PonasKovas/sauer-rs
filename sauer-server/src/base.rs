/// The built-in Sauerbraten base plugin, always present as the innermost plugin.
/// User plugins run before it and can cancel any handler.
use crate::{
    Server,
    items::{ServerItem, apply_pickup, canpickup, entity_to_item_type, spawntime},
    players::{ClipboardData, clear_player, init_player},
    players::combat::{explosion_damage, gun_damage_per_ray},
    plugin::{Context, Plugin},
};
use cube_string::IntoCubeStringLossy;
use sauer_proto::{
    Client,
    channel0,
    channel1::{
        self, ClipboardServer, ExplodeFx, ItemAck, ItemSpawn, RedoServer, ServInfo, ShotFx,
        SpawnServer, UndoServer,
    },
    channel2,
    data::{ClientState, GameMode, MasterMode, Privilege, SetTeamReason},
};
use std::sync::Arc;
use tracing::info;

pub(crate) struct Base;

// ── Helpers ───────────────────────────────────────────────────────────────────

/// Relay a coop-edit packet from `cn` to all other clients, but only in CoopEdit mode.
fn relay_coop<T>(server: &Server, cn: u32, pkt: &T)
where
    Client<T>: sauer_proto::PacketSequence<sauer_proto::route::S2C<sauer_proto::route::Chan1>>,
    T: Clone,
{
    if *server.game_mode.read().unwrap() != GameMode::CoopEdit {
        return;
    }
    server.broadcast_except_chan1(cn, &Client { client_num: cn as i32, packet: pkt.clone() });
}

/// Returns true if `target_cn` is alive with a matching `life_sequence`.
/// Guards against hitting a player who has already respawned.
fn check_hit(server: &Server, target_cn: u32, life_sequence: i32) -> bool {
    server.players.try_get(target_cn).map_or(false, |p| {
        p.client_state == ClientState::Alive
            && matches!(p.state.as_ref(), Some(s) if s.life_sequence == life_sequence)
    })
}

// ── Plugin implementation ─────────────────────────────────────────────────────

impl Plugin for Base {
    async fn start(&self, server: &Arc<Server>) {
        let server = Arc::clone(server);
        tokio::spawn(async move {
            let mut interval =
                tokio::time::interval(std::time::Duration::from_millis(100));
            loop {
                interval.tick().await;

                for mut entry in server.players.map.iter_mut() {
                    if entry.quad_millis > 0 {
                        entry.quad_millis = (entry.quad_millis - 100).max(0);
                    }
                }

                let spawning: Vec<usize> = {
                    let mut items = server.items.write().unwrap();
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
                    server.broadcast_chan1(&ItemSpawn { item_index: i as i32 });
                }
            }
        });
    }

    async fn connected(&self, ctx: &mut Context) {
        let session_id = init_player(&ctx.server().players, ctx.cn());

        let cn = ctx.cn();
        let server = ctx.server();
        server.send_chan1(
            cn,
            &ServInfo {
                client_num: cn as i32,
                protocol: sauer_proto::PROTOCOL_VERSION,
                session_id,
                has_password: *server.has_password.read().unwrap(),
                server_desc: server.server_desc.read().unwrap().clone(),
                server_auth: server.server_auth.read().unwrap().clone(),
            },
        );
    }

    async fn disconnected(&self, ctx: &mut Context) {
        let cn = ctx.cn();
        let server = ctx.server();

        let had_privilege = server
            .players
            .try_get(cn)
            .map_or(false, |p| p.privilege != Privilege::None);

        clear_player(&server.players, cn);

        server.broadcast_chan1(&channel1::ClientDisconnected { client_num: cn as i32 });

        if had_privilege {
            server.broadcast_current_master();
        }
    }

    async fn connect(&self, ctx: &mut Context, packet: &channel1::Connect) {
        info!("{} (cn {}) connected.", packet.name, ctx.cn());

        let cn = ctx.cn();
        let server = ctx.server();

        server.init_name(cn, packet.name.clone());
        server.init_model(cn, packet.player_model);

        match server.master_mode() {
            MasterMode::Private => {
                // A plugin is responsible for auth and must call server.welcome(cn) manually.
            }
            MasterMode::Locked => {
                server.welcome(cn);
                if server.get_privilege(cn) < Privilege::Master {
                    if let Some(mut p) = server.players.try_get_mut(cn) {
                        p.client_state = ClientState::Spectator;
                    }
                    server.broadcast_chan1(&channel1::SpectatorServer {
                        client_num: cn as i32,
                        is_spectator: true,
                    });
                }
            }
            _ => server.welcome(cn),
        }
    }

    async fn try_spawn(&self, ctx: &mut Context, _packet: &channel1::TrySpawn) {
        ctx.server().prepare_spawn(ctx.cn());
    }

    async fn spawn_client(&self, ctx: &mut Context, packet: &channel1::SpawnClient) {
        let cn = ctx.cn();
        let server = ctx.server();

        let spawned_state = {
            let Some(mut p) = server.players.try_get_mut(cn) else { return };
            if p.client_state != ClientState::Dead {
                return;
            }
            let Some(state) = p.state.as_mut() else { return };
            // Sync server life_seq to what the client is using.
            state.life_sequence = packet.life_sequence;
            state.gun_select = packet.gun_select;
            p.client_state = ClientState::Alive;
            p.state.clone()
        };

        if let Some(state) = spawned_state {
            // Broadcast to all OTHER clients; the spawning client already set itself CS_ALIVE.
            server.broadcast_except_chan1(
                cn,
                &Client { client_num: cn as i32, packet: SpawnServer { state } },
            );
        }
    }

    async fn spectator(&self, ctx: &mut Context, packet: &channel1::SpectatorClient) {
        let cn = ctx.cn();
        let server = ctx.server();
        let target_cn = packet.spectator_cn as u32;

        let has_privilege = server.get_privilege(cn) >= Privilege::Master;
        let locked = server.master_mode() == MasterMode::Locked;

        if !has_privilege && (target_cn != cn || locked) {
            return;
        }

        if let Some(mut p) = server.players.try_get_mut(target_cn) {
            if packet.val {
                p.client_state = ClientState::Spectator;
            } else if p.client_state == ClientState::Spectator {
                p.client_state = ClientState::Dead;
            }
        }

        server.broadcast_chan1(&channel1::SpectatorServer {
            client_num: packet.spectator_cn,
            is_spectator: packet.val,
        });
    }

    async fn suicide(&self, ctx: &mut Context, _packet: &channel1::Suicide) {
        ctx.server().kill_player(ctx.cn());
    }

    async fn shoot(&self, ctx: &mut Context, packet: &channel1::Shoot) {
        let cn = ctx.cn();
        let server = ctx.server();

        {
            let Some(mut p) = server.players.try_get_mut(cn) else { return };
            if p.client_state != ClientState::Alive {
                return;
            }
            if let Some(state) = p.state.as_mut() {
                match packet.gun {
                    sauer_proto::data::Gun::Fist => {}
                    sauer_proto::data::Gun::Sg => state.ammo.shotgun -= 1,
                    sauer_proto::data::Gun::Cg => state.ammo.chaingun -= 1,
                    sauer_proto::data::Gun::Rl => state.ammo.rocket_launcher -= 1,
                    sauer_proto::data::Gun::Rifle => state.ammo.rifle -= 1,
                    sauer_proto::data::Gun::Gl => state.ammo.grenade_launcher -= 1,
                    sauer_proto::data::Gun::Pistol => state.ammo.pistol -= 1,
                }
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
            server.apply_damage(target_cn, cn, packet.gun, damage, hit.dir);
        }
    }

    async fn explode(&self, ctx: &mut Context, packet: &channel1::Explode) {
        let cn = ctx.cn();
        let server = ctx.server();

        if server.players.get(cn).client_state != ClientState::Alive {
            return;
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
            server.apply_damage(target_cn, cn, packet.gun, damage, hit.dir);
        }
    }

    async fn pos(&self, ctx: &mut Context, packet: &channel0::Position) {
        ctx.server().set_position(ctx.cn(), packet.pos);
        ctx.server().broadcast_except_chan0(ctx.cn(), packet);
    }

    async fn teleport(&self, ctx: &mut Context, packet: &channel0::Teleport) {
        ctx.server().broadcast_except_chan0(ctx.cn(), packet);
    }

    async fn jumppad(&self, ctx: &mut Context, packet: &channel0::JumpPad) {
        ctx.server().broadcast_except_chan0(ctx.cn(), packet);
    }

    async fn text(&self, ctx: &mut Context, packet: &channel1::Text) {
        let name = ctx.server().players.get(ctx.cn()).name.clone();
        info!("[CHAT] {name}: {}", packet.text);
        ctx.server().broadcast_except_chan1(
            ctx.cn(),
            &Client { client_num: ctx.cn() as i32, packet: packet.clone() },
        );
    }

    async fn switch_name(&self, ctx: &mut Context, packet: &channel1::SwitchName) {
        ctx.server().set_name(ctx.cn(), packet.name.clone());
    }

    async fn switch_model(&self, ctx: &mut Context, packet: &channel1::SwitchModel) {
        ctx.server().set_model(ctx.cn(), packet.model);
    }

    async fn switch_team(&self, ctx: &mut Context, packet: &channel1::SwitchTeam) {
        ctx.server().set_team(ctx.cn(), packet.team.clone(), SetTeamReason::Switched);
    }

    async fn ping(&self, ctx: &mut Context, packet: &channel1::Ping) {
        ctx.server().send_chan1(ctx.cn(), &channel1::Pong { ping: packet.time });
    }

    async fn client_ping(&self, ctx: &mut Context, packet: &channel1::ClientPing) {
        ctx.server().broadcast_except_chan1(
            ctx.cn(),
            &Client { client_num: ctx.cn() as i32, packet: packet.clone() },
        );
    }

    async fn taunt(&self, ctx: &mut Context, packet: &channel1::Taunt) {
        ctx.server().broadcast_except_chan1(
            ctx.cn(),
            &Client { client_num: ctx.cn() as i32, packet: packet.clone() },
        );
    }

    async fn sound(&self, ctx: &mut Context, packet: &channel1::Sound) {
        ctx.server().broadcast_except_chan1(
            ctx.cn(),
            &Client { client_num: ctx.cn() as i32, packet: packet.clone() },
        );
    }

    async fn map(&self, ctx: &mut Context, data: &bytes::Bytes) {
        let cn = ctx.cn();
        let server = ctx.server();

        *server.map_data.write().unwrap() = Some(data.clone());

        let name = server.players.get(cn).name.clone();
        info!("[MAP] {name} (cn {cn}) uploaded a new map.");

        server.broadcast_chan1(&channel1::ServMsg {
            text: format!("[{name} uploaded the map, use \"/getmap\" to receive it]")
                .lossy_cubestring(),
        });
    }

    async fn get_map(&self, ctx: &mut Context, _packet: &channel1::GetMap) {
        let cn = ctx.cn();
        let server = ctx.server();

        let map_bytes = server.map_data.read().unwrap().clone();
        let Some(data) = map_bytes else { return };

        server.send_chan2(cn, &channel2::SendMap { data: data.to_vec() });

        // Resend all players' clipboards so the client has current state after loading the map.
        let clipboard_packets: Vec<(i32, i32, Vec<u8>)> = server
            .players
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
            server.send_chan1(cn, &ClipboardServer { client_num: owner_cn, unpack_len, data });
        }
    }

    async fn master_mode(&self, ctx: &mut Context, packet: &channel1::MasterModeClient) {
        if ctx.server().get_privilege(ctx.cn()) >= Privilege::Master {
            ctx.server().set_master_mode(packet.mode);
        }
    }

    // ── Coop-edit ─────────────────────────────────────────────────────────────

    async fn edit_tex(&self, ctx: &mut Context, packet: &channel1::EditTex) {
        relay_coop(ctx.server(), ctx.cn(), packet);
    }
    async fn edit_face(&self, ctx: &mut Context, packet: &channel1::EditFace) {
        relay_coop(ctx.server(), ctx.cn(), packet);
    }
    async fn replace(&self, ctx: &mut Context, packet: &channel1::Replace) {
        relay_coop(ctx.server(), ctx.cn(), packet);
    }
    async fn edit_v_slot(&self, ctx: &mut Context, packet: &channel1::EditVSlot) {
        relay_coop(ctx.server(), ctx.cn(), packet);
    }
    async fn flip(&self, ctx: &mut Context, packet: &channel1::Flip) {
        relay_coop(ctx.server(), ctx.cn(), packet);
    }
    async fn edit_var(&self, ctx: &mut Context, packet: &channel1::EditVar) {
        relay_coop(ctx.server(), ctx.cn(), packet);
    }
    async fn edit_mat(&self, ctx: &mut Context, packet: &channel1::EditMat) {
        relay_coop(ctx.server(), ctx.cn(), packet);
    }
    async fn edit_mode(&self, ctx: &mut Context, packet: &channel1::EditMode) {
        relay_coop(ctx.server(), ctx.cn(), packet);
    }
    async fn rotate(&self, ctx: &mut Context, packet: &channel1::Rotate) {
        relay_coop(ctx.server(), ctx.cn(), packet);
    }
    async fn del_cube(&self, ctx: &mut Context, packet: &channel1::DelCube) {
        relay_coop(ctx.server(), ctx.cn(), packet);
    }
    async fn remip(&self, ctx: &mut Context, packet: &channel1::Remip) {
        relay_coop(ctx.server(), ctx.cn(), packet);
    }

    async fn edit_ent(&self, ctx: &mut Context, packet: &channel1::EditEnt) {
        let cn = ctx.cn();
        let server = ctx.server();

        if *server.game_mode.read().unwrap() != GameMode::CoopEdit {
            return;
        }

        let idx = packet.ent_id as usize;
        let new_item = {
            let mut items = server.items.write().unwrap();
            match entity_to_item_type(packet.type_) {
                Some(item_type) => {
                    if items.list.len() <= idx {
                        items.list.resize_with(idx + 1, || ServerItem {
                            item_type,
                            spawned: false,
                            spawn_time_ms: 0,
                        });
                        true
                    } else {
                        let item = &mut items.list[idx];
                        if item.item_type != item_type
                            || (!item.spawned && item.spawn_time_ms == 0)
                        {
                            item.item_type = item_type;
                            item.spawned = false;
                            item.spawn_time_ms = 0;
                            true
                        } else {
                            false
                        }
                    }
                }
                None => {
                    if let Some(item) = items.list.get_mut(idx) {
                        item.spawned = false;
                        item.spawn_time_ms = 0;
                    }
                    false
                }
            }
        };

        // Relay the geometry change to all other coop clients.
        server.broadcast_except_chan1(
            cn,
            &Client { client_num: cn as i32, packet: packet.clone() },
        );

        if new_item {
            server.items.write().unwrap().list[idx].spawned = true;
            server.broadcast_chan1(&ItemSpawn { item_index: idx as i32 });
        }
    }

    async fn new_map(&self, ctx: &mut Context, packet: &channel1::NewMap) {
        if packet.size >= 0 {
            let mut items = ctx.server().items.write().unwrap();
            items.reset();
            items.notgotitems = false;
        }
        relay_coop(ctx.server(), ctx.cn(), packet);
    }

    async fn item_list(&self, ctx: &mut Context, packet: &channel1::ItemList) {
        let cn = ctx.cn();
        let server = ctx.server();

        let is_spectator = server
            .players
            .try_get(cn)
            .map_or(false, |p| p.client_state == ClientState::Spectator);

        let mut items = server.items.write().unwrap();
        if !items.notgotitems || is_spectator {
            return;
        }
        let num_players = server.clients.len();
        for item in &packet.items {
            let idx = item.item_index as usize;
            while items.list.len() <= idx {
                items.list.push(ServerItem {
                    item_type: item.item_type,
                    spawned: false,
                    spawn_time_ms: 0,
                });
            }
            items.list[idx].item_type = item.item_type;
            if crate::items::delay_spawn(item.item_type) {
                items.list[idx].spawn_time_ms = spawntime(item.item_type, num_players);
                items.list[idx].spawned = false;
            } else {
                items.list[idx].spawned = true;
                items.list[idx].spawn_time_ms = 0;
            }
        }
        items.notgotitems = false;
    }

    async fn item_pickup(&self, ctx: &mut Context, packet: &channel1::ItemPickup) {
        let cn = ctx.cn();
        let server = ctx.server();
        let idx = packet.item_index as usize;

        let can = {
            let Some(p) = server.players.try_get(cn) else { return };
            if p.client_state != ClientState::Alive {
                return;
            }
            let quad_millis = p.quad_millis;
            p.state.as_ref().map_or(false, |s| {
                server
                    .items
                    .read()
                    .unwrap()
                    .list
                    .get(idx)
                    .map_or(false, |item| item.spawned && canpickup(s, quad_millis, item.item_type))
            })
        };

        if !can {
            server.send_chan1(cn, &ItemAck { item_index: packet.item_index, client_num: -1 });
            return;
        }

        let num_players = server.clients.len();
        let item_type = {
            let mut items = server.items.write().unwrap();
            let item = &mut items.list[idx];
            item.spawned = false;
            item.spawn_time_ms = spawntime(item.item_type, num_players);
            item.item_type
        };

        if let Some(mut p) = server.players.try_get_mut(cn) {
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
        if *ctx.server().game_mode.read().unwrap() != GameMode::CoopEdit {
            return;
        }
        ctx.server().broadcast_except_chan1(
            ctx.cn(),
            &UndoServer {
                client_num: ctx.cn() as i32,
                unpack_len: packet.unpack_len,
                data: packet.data.clone(),
            },
        );
    }

    async fn redo(&self, ctx: &mut Context, packet: &channel1::RedoClient) {
        if *ctx.server().game_mode.read().unwrap() != GameMode::CoopEdit {
            return;
        }
        ctx.server().broadcast_except_chan1(
            ctx.cn(),
            &RedoServer {
                client_num: ctx.cn() as i32,
                unpack_len: packet.unpack_len,
                data: packet.data.clone(),
            },
        );
    }

    async fn copy(&self, ctx: &mut Context, packet: &channel1::CopyClient) {
        if *ctx.server().game_mode.read().unwrap() != GameMode::CoopEdit {
            return;
        }
        if let Some(mut p) = ctx.server().players.try_get_mut(ctx.cn()) {
            p.clipboard = None;
        }
        ctx.server().broadcast_except_chan1(
            ctx.cn(),
            &Client {
                client_num: ctx.cn() as i32,
                packet: channel1::CopyServer { sel: packet.sel.clone() },
            },
        );
    }

    async fn paste(&self, ctx: &mut Context, packet: &channel1::PasteClient) {
        if *ctx.server().game_mode.read().unwrap() != GameMode::CoopEdit {
            return;
        }
        ctx.server().broadcast_except_chan1(
            ctx.cn(),
            &Client {
                client_num: ctx.cn() as i32,
                packet: channel1::PasteServer { sel: packet.sel.clone() },
            },
        );
    }

    async fn clipboard(&self, ctx: &mut Context, packet: &channel1::ClipboardClient) {
        let cn = ctx.cn();
        let server = ctx.server();

        let is_spectator =
            server.players.try_get(cn).map_or(false, |p| p.client_state == ClientState::Spectator);
        if is_spectator {
            return;
        }

        let pack_len = packet.data.len();
        if pack_len == 0 || pack_len > (1 << 16) || packet.unpack_len <= 0 {
            if let Some(mut p) = server.players.try_get_mut(cn) {
                p.clipboard = None;
            }
            return;
        }

        if let Some(mut p) = server.players.try_get_mut(cn) {
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
