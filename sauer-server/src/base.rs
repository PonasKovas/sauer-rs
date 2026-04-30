//! The built-in Sauerbraten base plugin, always present as the innermost plugin.
//! User plugins run before it and can cancel any handler.

use crate::{
    Server,
    items::{ServerItem, apply_pickup, canpickup, spawntime},
    players::combat::{explosion_damage, gun_damage_per_ray},
    players::{ClipboardData, clear_player, init_player},
    plugin::{Context, Plugin},
};
use cube_string::{CubeString, IntoCubeStringLossy};
use sauer_proto::{
    Client, channel0,
    channel1::{
        self, ClipboardServer, ExplodeFx, ItemAck, ItemSpawn, MapChange, RedoServer, ServInfo,
        ShotFx, SpawnServer, UndoServer,
    },
    channel2,
    data::{ClientState, GameMode, ItemType, MasterMode, PlayerState, Privilege, SetTeamReason},
};
use std::sync::{Arc, Weak};
use tracing::info;

pub(crate) struct Base<P: Plugin> {
    /// Weak back-reference to the full `(P, Base<P>)` arc so that `Base` can
    /// fire semantic hooks back through the complete plugin chain.
    plugin: Weak<(P, Base<P>)>,
}

impl<P: Plugin> Base<P> {
    pub(crate) fn new(plugin: Weak<(P, Base<P>)>) -> Self {
        Self { plugin }
    }
}

// ── Helpers ───────────────────────────────────────────────────────────────────

/// Relay a coop-edit packet from `cn` to all other clients, but only in CoopEdit mode.
fn relay_coop<T>(server: &Server, cn: u32, pkt: &T)
where
    Client<T>: sauer_proto::PacketSequence<sauer_proto::route::S2C<sauer_proto::route::Chan1>>,
    T: Clone + sauer_proto::PacketDescriptor + std::fmt::Debug,
{
    if *server.game_mode.read().unwrap() != GameMode::CoopEdit {
        return;
    }
    server.broadcast_except_chan1(
        cn,
        &Client {
            client_num: cn as i32,
            packet: pkt.clone(),
        },
    );
}

/// Returns true if `target_cn` is alive with a matching `life_sequence`.
/// Guards against hitting a player who has already respawned.
fn check_hit(server: &Server, target_cn: u32, life_sequence: i32) -> bool {
    server.players.try_get(target_cn).map_or(false, |p| {
        p.client_state == ClientState::Alive
            && matches!(p.state.as_ref(), Some(s) if s.life_sequence == life_sequence)
    })
}

// ── Map change helper ─────────────────────────────────────────────────────────

/// Updates server state for a map change, broadcasts `N_MAPCHANGE` to all clients,
/// and clears all outstanding votes.
fn do_map_change(server: &Server, map_name: cube_string::CubeString, mode: GameMode) {
    *server.map_name.write().unwrap() = map_name.clone();
    *server.game_mode.write().unwrap() = mode;
    server.map_votes.write().unwrap().clear();
    server.broadcast_chan1(&MapChange {
        map_name,
        mode,
        has_items: true,
    });
}

// ── Command arg parser ────────────────────────────────────────────────────────

/// Splits `bytes` (everything after the leading `#`) into command arguments.
/// Spaces delimit arguments; `"..."` groups tokens into one (quotes stripped).
fn parse_command_args(bytes: &[u8]) -> Vec<CubeString> {
    let mut args: Vec<CubeString> = Vec::new();
    let mut current: Vec<u8> = Vec::new();
    let mut in_quotes = false;

    for &b in bytes {
        match b {
            b'"' => in_quotes = !in_quotes,
            b' ' if !in_quotes => {
                if !current.is_empty() {
                    args.push(CubeString::from_bytes(std::mem::take(&mut current)));
                }
            }
            _ => current.push(b),
        }
    }
    if !current.is_empty() {
        args.push(CubeString::from_bytes(current));
    }
    args
}

// ── Plugin implementation ─────────────────────────────────────────────────────

impl<P: Plugin> Plugin for Base<P> {
    async fn start(&self, server: &Arc<Server>) {
        let server = Arc::clone(server);
        tokio::spawn(async move {
            let mut interval = tokio::time::interval(std::time::Duration::from_millis(100));
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
                    server.broadcast_chan1(&ItemSpawn {
                        item_index: i as i32,
                    });
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

        server.map_votes.write().unwrap().remove(&cn);

        server.broadcast_chan1(&channel1::ClientDisconnected {
            client_num: cn as i32,
        });

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

        if server.master_mode() == MasterMode::Locked
            && server.get_privilege(cn) < Privilege::Master
        {
            if let Some(mut p) = server.players.try_get_mut(cn) {
                p.client_state = ClientState::Spectator;
            }
        }

        if matches!(server.master_mode(), MasterMode::Private) {
            return;
        }

        server.welcome(cn);

        if let Some(plugin) = self.plugin.upgrade() {
            ctx.cancelled = false;
            plugin.on_client_welcomed(ctx).await;
        }
    }

    async fn on_spawn(&self, ctx: &mut Context, state: &mut PlayerState) {
        ctx.server().apply_spawn_state(ctx.cn(), state.clone());
    }

    async fn try_spawn(&self, ctx: &mut Context, _packet: &channel1::TrySpawn) {
        let cn = ctx.cn();
        if let Some(mut state) = ctx.server().compute_spawn_state(cn) {
            if let Some(plugin) = self.plugin.upgrade() {
                plugin.on_spawn(ctx, &mut state).await;
            }
        }
    }

    async fn spawn_client(&self, ctx: &mut Context, packet: &channel1::SpawnClient) {
        let cn = ctx.cn();
        let server = ctx.server();

        let spawned_state = {
            let Some(mut p) = server.players.try_get_mut(cn) else {
                return;
            };
            if p.client_state != ClientState::Dead {
                return;
            }
            let Some(state) = p.state.as_mut() else {
                return;
            };
            state.life_sequence = packet.life_sequence;
            state.gun_select = packet.gun_select;

            p.client_state = ClientState::Alive;
            p.state.clone()
        };

        if let Some(state) = spawned_state {
            server.broadcast_except_chan1(
                cn,
                &Client {
                    client_num: cn as i32,
                    packet: SpawnServer { state },
                },
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

        server.set_spectator(target_cn, packet.val);
    }

    async fn suicide(&self, ctx: &mut Context, _packet: &channel1::Suicide) {
        let cn = ctx.cn();
        if ctx.server().kill_player(cn) {
            if let Some(plugin) = self.plugin.upgrade() {
                plugin.on_player_died(ctx, cn, cn).await;
            }
        }
    }

    async fn shoot(&self, ctx: &mut Context, packet: &channel1::Shoot) {
        let cn = ctx.cn();
        let server = ctx.server().clone();

        {
            let Some(mut p) = server.players.try_get_mut(cn) else {
                return;
            };
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
            if !check_hit(&server, target_cn, hit.life_sequence) {
                continue;
            }
            let mut damage = hit.rays * gun_damage_per_ray(packet.gun);
            
            ctx.cancelled = false;
            if let Some(plugin) = self.plugin.upgrade() {
                plugin.on_damage(ctx, target_cn, cn, &mut damage).await;
            }
            if ctx.cancelled || damage <= 0 {
                continue;
            }

            if server.apply_damage(target_cn, cn, packet.gun, damage, hit.dir) {
                if let Some(plugin) = self.plugin.upgrade() {
                    plugin.on_player_died(ctx, target_cn, cn).await;
                }
            }
        }
    }

    async fn explode(&self, ctx: &mut Context, packet: &channel1::Explode) {
        let cn = ctx.cn();
        let server = ctx.server().clone();

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
            if !check_hit(&server, target_cn, hit.life_sequence) {
                continue;
            }
            let mut damage = explosion_damage(packet.gun, hit.dist, target_cn == cn);
            
            ctx.cancelled = false;
            if let Some(plugin) = self.plugin.upgrade() {
                plugin.on_damage(ctx, target_cn, cn, &mut damage).await;
            }
            if ctx.cancelled || damage <= 0 {
                continue;
            }

            if server.apply_damage(target_cn, cn, packet.gun, damage, hit.dir) {
                if let Some(plugin) = self.plugin.upgrade() {
                    plugin.on_player_died(ctx, target_cn, cn).await;
                }
            }
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
        let cn = ctx.cn();
        let bytes = packet.text.as_bytes();

        if bytes.first() == Some(&b'#') {
            let args = parse_command_args(&bytes[1..]);
            if let Some(plugin) = self.plugin.upgrade() {
                plugin.on_command(ctx, &args).await;
            }
        } else {
            let mut text = packet.text.clone();
            if let Some(plugin) = self.plugin.upgrade() {
                plugin.on_chat(ctx, &mut text).await;
            }
            if !ctx.cancelled {
                let name = ctx.server().players.get(cn).name.clone();
                info!("[CHAT] {name}: {text}");
                ctx.server().broadcast_except_chan1(
                    cn,
                    &Client {
                        client_num: cn as i32,
                        packet: channel1::Text { text },
                    },
                );
            }
        }
    }

    async fn switch_name(&self, ctx: &mut Context, packet: &channel1::SwitchName) {
        ctx.server().set_name(ctx.cn(), packet.name.clone());
    }

    async fn switch_model(&self, ctx: &mut Context, packet: &channel1::SwitchModel) {
        ctx.server().set_model(ctx.cn(), packet.model);
    }

    async fn switch_team(&self, ctx: &mut Context, packet: &channel1::SwitchTeam) {
        ctx.server()
            .set_team(ctx.cn(), packet.team.clone(), SetTeamReason::Switched);
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
        ctx.server().broadcast_except_chan1(
            ctx.cn(),
            &Client {
                client_num: ctx.cn() as i32,
                packet: packet.clone(),
            },
        );
    }

    async fn sound(&self, ctx: &mut Context, packet: &channel1::Sound) {
        ctx.server().broadcast_except_chan1(
            ctx.cn(),
            &Client {
                client_num: ctx.cn() as i32,
                packet: packet.clone(),
            },
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

        server.send_chan2(
            cn,
            &channel2::SendMap {
                data: data.to_vec(),
            },
        );

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

    async fn master_mode(&self, ctx: &mut Context, packet: &channel1::MasterModeClient) {
        if ctx.server().get_privilege(ctx.cn()) >= Privilege::Master {
            ctx.server().set_master_mode(packet.mode);
        }
    }

    async fn map_vote(&self, ctx: &mut Context, packet: &channel1::MapVote) {
        let cn = ctx.cn();
        let server = ctx.server();
        let privilege = server.get_privilege(cn);
        let prev_map = server.map_name.read().unwrap().clone();

        match server.master_mode() {
            MasterMode::Open => {
                server
                    .map_votes
                    .write()
                    .unwrap()
                    .insert(cn, packet.map_name.clone());

                let total_clients = server.client_count();
                let vote_count = server
                    .map_votes
                    .read()
                    .unwrap()
                    .values()
                    .filter(|v| **v == packet.map_name)
                    .count();

                if vote_count * 2 > total_clients {
                    do_map_change(server, packet.map_name.clone(), packet.mode);
                }
            }
            _ => {
                if privilege >= Privilege::Master {
                    do_map_change(server, packet.map_name.clone(), packet.mode);
                }
            }
        }

        let new_map = server.map_name.read().unwrap().clone();
        if new_map != prev_map {
            let mode = *server.game_mode.read().unwrap();
            if let Some(plugin) = self.plugin.upgrade() {
                ctx.cancelled = false;
                plugin.map_changed(ctx, &new_map, mode).await;
            }
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
        let cn = ctx.cn();
        if let Some(mut p) = ctx.server().players.try_get_mut(cn) {
            if packet.is_editing {
                p.edit_state = p.client_state;
                p.client_state = ClientState::Editing;
            } else {
                p.client_state = p.edit_state;
            }
        }
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
            match ItemType::try_from(packet.type_) {
                Ok(item_type) => {
                    if items.list.len() <= idx {
                        items.list.resize_with(idx + 1, || ServerItem {
                            item_type,
                            spawned: false,
                            spawn_time_ms: 0,
                        });
                        true
                    } else {
                        let item = &mut items.list[idx];
                        if item.item_type != item_type || (!item.spawned && item.spawn_time_ms == 0)
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
                Err(_) => {
                    if let Some(item) = items.list.get_mut(idx) {
                        item.spawned = false;
                        item.spawn_time_ms = 0;
                    }
                    false
                }
            }
        };

        server.broadcast_except_chan1(
            cn,
            &Client {
                client_num: cn as i32,
                packet: packet.clone(),
            },
        );

        if new_item {
            server.items.write().unwrap().list[idx].spawned = true;
            server.broadcast_chan1(&ItemSpawn {
                item_index: idx as i32,
            });
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
        let num_players = server.client_count();
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
            let Some(p) = server.players.try_get(cn) else {
                return;
            };
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

        let num_players = server.client_count();
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
                packet: channel1::CopyServer {
                    sel: packet.sel.clone(),
                },
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
                packet: channel1::PasteServer {
                    sel: packet.sel.clone(),
                },
            },
        );
    }

    async fn clipboard(&self, ctx: &mut Context, packet: &channel1::ClipboardClient) {
        let cn = ctx.cn();
        let server = ctx.server();

        let is_spectator = server
            .players
            .try_get(cn)
            .map_or(false, |p| p.client_state == ClientState::Spectator);
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
