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

/// Relay a coop-edit packet from `id` to all other clients, but only in CoopEdit mode.
fn relay_coop<T>(server: &Server, id: u64, pkt: &T)
where
    Client<T>: sauer_proto::PacketSequence<sauer_proto::route::S2C<sauer_proto::route::Chan1>>,
    T: Clone + sauer_proto::PacketDescriptor + std::fmt::Debug,
{
    if *server.game_mode.read().unwrap() != GameMode::CoopEdit {
        return;
    }
    if let Some(cn) = server.get_cn(id) {
        server.broadcast_except_chan1(
            id,
            &Client {
                client_num: cn as i32,
                packet: pkt.clone(),
            },
        );
    }
}

/// Returns true if `target_id` is alive with a matching `life_sequence`.
/// Guards against hitting a player who has already respawned.
fn check_hit(server: &Server, target_id: u64, life_sequence: i32) -> bool {
    server.players.try_get(target_id).map_or(false, |p| {
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
        let id = ctx.id();
        let server = ctx.server();

        let cn = match server.assign_cn(id) {
            Some(c) => c,
            None => {
                server.disconnect(id);
                ctx.cancel();
                return;
            }
        };

        let session_id = init_player(&server.players, id);

        server.send_chan1(
            id,
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

    async fn server_query(
        &self,
        ctx: &mut crate::plugin::ServerQueryContext,
        request: &sauer_proto::server_query::ServerQueryRequest,
    ) {
        use sauer_proto::server_query::{
            ExtInfoRequest, ExtInfoResponse, ServerQueryRequest, ServerQueryResponse,
        };

        let server = ctx.server();
        match request {
            ServerQueryRequest::ServerInfo { ping_id } => {
                let gamemode = *server.game_mode.read().unwrap() as i32;
                let mastermode = *server.master_mode.read().unwrap() as i32;
                let has_password = *server.has_password.read().unwrap();
                let mm = if has_password { 4 } else { mastermode };
                let map_name = server.map_name.read().unwrap().clone();
                let server_desc = server.server_desc.read().unwrap().clone();

                let resp = ServerQueryResponse::ServerInfo {
                    ping_id: *ping_id,
                    num_clients: server.client_count() as i32,
                    protocol_version: sauer_proto::PROTOCOL_VERSION,
                    game_mode: gamemode,
                    remaining_time: 0, // TODO
                    max_clients: 128,
                    master_mode: mm,
                    is_paused: false, // TODO
                    game_speed: 100,  // TODO
                    map_name,
                    server_desc,
                };
                server.send_server_query(ctx.addr(), &resp);
            }
            ServerQueryRequest::ExtInfo(ext_req) => {
                match ext_req {
                    ExtInfoRequest::Uptime => {
                        server.send_server_query(
                            ctx.addr(),
                            &ServerQueryResponse::ExtInfo(ExtInfoResponse::Uptime {
                                seconds: server.start_time.elapsed().as_secs() as i32,
                            }),
                        );
                    }
                    ExtInfoRequest::PlayerStats { client_number } => {
                        if let Some(cn_i32) = client_number {
                            if let Some(id) = server.get_id(*cn_i32 as u8) {
                                if let Some(p) = server.players.try_get(id) {
                                    server.send_server_query(
                                        ctx.addr(),
                                        &ServerQueryResponse::ExtInfo(ExtInfoResponse::PlayerStats {
                                            client_number: Some(*cn_i32),
                                            ok: true,
                                        }),
                                    );
                                    server.send_server_query(
                                        ctx.addr(),
                                        &ServerQueryResponse::ExtInfo(
                                            ExtInfoResponse::PlayerStatsIds { ids: vec![*cn_i32] },
                                        ),
                                    );

                                    let state = p.state.clone().unwrap_or_else(|| {
                                        crate::players::state::make_spawn_state(
                                            server.game_mode.read().unwrap().is_insta(),
                                            0,
                                        )
                                    });
                                    let ip = server.peer_addr(id).map(|a| match a.ip() {
                                        std::net::IpAddr::V4(v4) => {
                                            let octets = v4.octets();
                                            [octets[0], octets[1], octets[2]]
                                        }
                                        _ => [0, 0, 0],
                                    });

                                    server.send_server_query(
                                        ctx.addr(),
                                        &ServerQueryResponse::ExtInfo(
                                            ExtInfoResponse::PlayerStatsStats {
                                                client_num: *cn_i32,
                                                ping: p.ping,
                                                name: p.name.clone(),
                                                team: p.team.clone(),
                                                frags: p.frags,
                                                flags: p.flags,
                                                deaths: p.deaths,
                                                teamkills: p.teamkills,
                                                damage: 0, // TODO
                                                health: state.health,
                                                armour: state.armour,
                                                gunselect: state.gun_select,
                                                privilege: p.privilege,
                                                state: p.client_state,
                                                ip,
                                            },
                                        ),
                                    );
                                }
                            } else {
                                server.send_server_query(
                                    ctx.addr(),
                                    &ServerQueryResponse::ExtInfo(ExtInfoResponse::PlayerStats {
                                        client_number: Some(*cn_i32),
                                        ok: false,
                                    }),
                                );
                            }
                        } else {
                            server.send_server_query(
                                ctx.addr(),
                                &ServerQueryResponse::ExtInfo(ExtInfoResponse::PlayerStats {
                                    client_number: None,
                                    ok: true,
                                }),
                            );

                            let mut ids = Vec::new();
                            for e in server.players.map.iter() {
                                if let Some(cn) = server.get_cn(*e.key()) {
                                    ids.push(cn as i32);
                                }
                            }
                            server.send_server_query(
                                ctx.addr(),
                                &ServerQueryResponse::ExtInfo(ExtInfoResponse::PlayerStatsIds {
                                    ids,
                                }),
                            );

                            for e in server.players.map.iter() {
                                let id = *e.key();
                                let Some(cn) = server.get_cn(id) else { continue; };
                                let p = e.value();
                                let state = p.state.clone().unwrap_or_else(|| {
                                    crate::players::state::make_spawn_state(
                                        server.game_mode.read().unwrap().is_insta(),
                                        0,
                                    )
                                });
                                let ip = server.peer_addr(id).map(|a| match a.ip() {
                                    std::net::IpAddr::V4(v4) => {
                                        let octets = v4.octets();
                                        [octets[0], octets[1], octets[2]]
                                    }
                                    _ => [0, 0, 0],
                                });

                                server.send_server_query(
                                    ctx.addr(),
                                    &ServerQueryResponse::ExtInfo(
                                        ExtInfoResponse::PlayerStatsStats {
                                            client_num: cn as i32,
                                            ping: p.ping,
                                            name: p.name.clone(),
                                            team: p.team.clone(),
                                            frags: p.frags,
                                            flags: p.flags,
                                            deaths: p.deaths,
                                            teamkills: p.teamkills,
                                            damage: 0, // TODO
                                            health: state.health,
                                            armour: state.armour,
                                            gunselect: state.gun_select,
                                            privilege: p.privilege,
                                            state: p.client_state,
                                            ip,
                                        },
                                    ),
                                );
                            }
                        }
                    }
                    ExtInfoRequest::TeamScore => {
                        let game_mode = *server.game_mode.read().unwrap() as i32;
                        let scores = if server.game_mode.read().unwrap().has_teams() {
                            Some(Vec::new()) // TODO: actual team scores
                        } else {
                            None
                        };
                        server.send_server_query(
                            ctx.addr(),
                            &ServerQueryResponse::ExtInfo(ExtInfoResponse::TeamScore {
                                game_mode,
                                remaining_time: 0, // TODO
                                scores,
                            }),
                        );
                    }
                }
            }
        }
    }

    async fn disconnected(&self, ctx: &mut Context) {
        let id = ctx.id();
        let server = ctx.server();

        let had_privilege = server
            .players
            .try_get(id)
            .map_or(false, |p| p.privilege != Privilege::None);

        clear_player(&server.players, id);

        server.map_votes.write().unwrap().remove(&id);

        if let Some(cn) = server.get_cn(id) {
            server.broadcast_chan1(&channel1::ClientDisconnected {
                client_num: cn as i32,
            });
            server.free_cn(cn);
        }

        if had_privilege {
            server.broadcast_current_master();
        }
    }

    async fn connect(&self, ctx: &mut Context, packet: &channel1::Connect) {
        let id = ctx.id();
        let server = ctx.server();
        
        let cn_str = server.get_cn(id).map(|c| c.to_string()).unwrap_or_else(|| "?".to_string());
        info!("{} (cn {}) connected.", packet.name, cn_str);

        server.init_name(id, packet.name.clone());
        server.init_model(id, packet.player_model);

        if server.master_mode() == MasterMode::Locked
            && server.get_privilege(id) < Privilege::Master
        {
            if let Some(mut p) = server.players.try_get_mut(id) {
                p.client_state = ClientState::Spectator;
            }
        }

        if matches!(server.master_mode(), MasterMode::Private) {
            return;
        }

        server.welcome(id);

        if let Some(plugin) = self.plugin.upgrade() {
            ctx.cancelled = false;
            plugin.on_client_welcomed(ctx).await;
        }
    }

    async fn on_spawn(&self, ctx: &mut Context, state: &mut PlayerState) {
        ctx.server().apply_spawn_state(ctx.id(), state.clone());
    }

    async fn try_spawn(&self, ctx: &mut Context, _packet: &channel1::TrySpawn) {
        let id = ctx.id();
        if let Some(mut state) = ctx.server().compute_spawn_state(id) {
            if let Some(plugin) = self.plugin.upgrade() {
                plugin.on_spawn(ctx, &mut state).await;
            }
        }
    }

    async fn spawn_client(&self, ctx: &mut Context, packet: &channel1::SpawnClient) {
        let id = ctx.id();
        let server = ctx.server();

        let spawned_state = {
            let Some(mut p) = server.players.try_get_mut(id) else {
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
            if let Some(cn) = server.get_cn(id) {
                server.broadcast_except_chan1(
                    id,
                    &Client {
                        client_num: cn as i32,
                        packet: SpawnServer { state },
                    },
                );
            }
        }
    }

    async fn spectator(&self, ctx: &mut Context, packet: &channel1::SpectatorClient) {
        let id = ctx.id();
        let server = ctx.server();
        let Some(target_id) = server.get_id(packet.spectator_cn as u8) else { return; };

        let has_privilege = server.get_privilege(id) >= Privilege::Master;
        let locked = server.master_mode() == MasterMode::Locked;

        if !has_privilege && (target_id != id || locked) {
            return;
        }

        server.set_spectator(target_id, packet.val);
    }

    async fn suicide(&self, ctx: &mut Context, _packet: &channel1::Suicide) {
        let id = ctx.id();
        if ctx.server().kill_player(id) {
            if let Some(plugin) = self.plugin.upgrade() {
                plugin.on_player_died(ctx, id, id).await;
            }
        }
    }

    async fn shoot(&self, ctx: &mut Context, packet: &channel1::Shoot) {
        let id = ctx.id();
        let server = ctx.server().clone();

        {
            let Some(mut p) = server.players.try_get_mut(id) else {
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

        if let Some(cn) = server.get_cn(id) {
            server.broadcast_except_chan1(
                id,
                &ShotFx {
                    client_num: cn as i32,
                    gun: packet.gun,
                    id: packet.id,
                    from: packet.from,
                    to: packet.to,
                },
            );
        }

        for hit in &packet.hits {
            let Some(target_id) = server.get_id(hit.target_cn as u8) else { continue; };
            if !check_hit(&server, target_id, hit.life_sequence) {
                continue;
            }
            let mut damage = hit.rays * gun_damage_per_ray(packet.gun);

            ctx.cancelled = false;
            if let Some(plugin) = self.plugin.upgrade() {
                plugin.on_damage(ctx, target_id, id, &mut damage).await;
            }
            if ctx.cancelled || damage <= 0 {
                continue;
            }

            if server.apply_damage(target_id, id, packet.gun, damage, hit.dir) {
                if let Some(plugin) = self.plugin.upgrade() {
                    plugin.on_player_died(ctx, target_id, id).await;
                }
            }
        }
    }

    async fn explode(&self, ctx: &mut Context, packet: &channel1::Explode) {
        let id = ctx.id();
        let server = ctx.server().clone();

        if server.players.get(id).client_state != ClientState::Alive {
            return;
        }

        if let Some(cn) = server.get_cn(id) {
            server.broadcast_chan1(&ExplodeFx {
                client_num: cn as i32,
                gun: packet.gun,
                id: packet.id,
            });
        }

        for hit in &packet.hits {
            let Some(target_id) = server.get_id(hit.target_cn as u8) else { continue; };
            if !check_hit(&server, target_id, hit.life_sequence) {
                continue;
            }
            let mut damage = explosion_damage(packet.gun, hit.dist, target_id == id);

            ctx.cancelled = false;
            if let Some(plugin) = self.plugin.upgrade() {
                plugin.on_damage(ctx, target_id, id, &mut damage).await;
            }
            if ctx.cancelled || damage <= 0 {
                continue;
            }

            if server.apply_damage(target_id, id, packet.gun, damage, hit.dir) {
                if let Some(plugin) = self.plugin.upgrade() {
                    plugin.on_player_died(ctx, target_id, id).await;
                }
            }
        }
    }

    async fn pos(&self, ctx: &mut Context, packet: &channel0::Position) {
        ctx.server().set_position(ctx.id(), packet.pos);
        ctx.server().broadcast_except_chan0(ctx.id(), packet);
    }

    async fn teleport(&self, ctx: &mut Context, packet: &channel0::Teleport) {
        ctx.server().broadcast_except_chan0(ctx.id(), packet);
    }

    async fn jumppad(&self, ctx: &mut Context, packet: &channel0::JumpPad) {
        ctx.server().broadcast_except_chan0(ctx.id(), packet);
    }

    async fn text(&self, ctx: &mut Context, packet: &channel1::Text) {
        let id = ctx.id();
        let bytes = packet.text.as_bytes();

        if bytes.first() == Some(&b'#') {
            let args = parse_command_args(bytes);
            let cmd = args.get(0).and_then(|s| s.as_str()).unwrap_or("");

            if cmd == "#help" {
                let mut cmds = ctx.server().commands.read().unwrap().clone();
                cmds.sort_by(|a, b| a.name.cmp(&b.name));
                let mut lines = vec![crate::ui::header("SERVER COMMANDS")];
                for c in cmds {
                    let mut usage = c.name.clone();
                    if !c.usage.is_empty() {
                        usage.push_str(" ");
                        usage.push_str(&c.usage);
                    }
                    lines.push(crate::ui::tip(&usage, &c.desc));
                }
                crate::ui::send_banner(ctx.server(), ctx.id(), &lines);
            } else if let Some(plugin) = self.plugin.upgrade() {
                plugin.on_command(ctx, &args).await;
            }
        } else {
            let mut text = packet.text.clone();
            if let Some(plugin) = self.plugin.upgrade() {
                plugin.on_chat(ctx, &mut text).await;
            }
            if !ctx.cancelled {
                let name = ctx.server().players.get(id).name.clone();
                info!("[CHAT] {name}: {text}");
                if let Some(cn) = ctx.server().get_cn(id) {
                    ctx.server().broadcast_except_chan1(
                        id,
                        &Client {
                            client_num: cn as i32,
                            packet: channel1::Text { text },
                        },
                    );
                }
            }
        }
    }

    async fn switch_name(&self, ctx: &mut Context, packet: &channel1::SwitchName) {
        ctx.server().set_name(ctx.id(), packet.name.clone());
    }

    async fn switch_model(&self, ctx: &mut Context, packet: &channel1::SwitchModel) {
        ctx.server().set_model(ctx.id(), packet.model);
    }

    async fn switch_team(&self, ctx: &mut Context, packet: &channel1::SwitchTeam) {
        ctx.server()
            .set_team(ctx.id(), packet.team.clone(), SetTeamReason::Switched);
    }

    async fn ping(&self, ctx: &mut Context, packet: &channel1::Ping) {
        ctx.server()
            .send_chan1(ctx.id(), &channel1::Pong { ping: packet.time });
    }

    async fn client_ping(&self, ctx: &mut Context, packet: &channel1::ClientPing) {
        let id = ctx.id();
        if let Some(mut p) = ctx.server().players.try_get_mut(id) {
            p.ping = packet.ping;
        }
        if let Some(cn) = ctx.server().get_cn(id) {
            ctx.server().broadcast_except_chan1(
                id,
                &Client {
                    client_num: cn as i32,
                    packet: packet.clone(),
                },
            );
        }
    }

    async fn taunt(&self, ctx: &mut Context, packet: &channel1::Taunt) {
        let id = ctx.id();
        if let Some(cn) = ctx.server().get_cn(id) {
            ctx.server().broadcast_except_chan1(
                id,
                &Client {
                    client_num: cn as i32,
                    packet: packet.clone(),
                },
            );
        }
    }

    async fn sound(&self, ctx: &mut Context, packet: &channel1::Sound) {
        let id = ctx.id();
        if let Some(cn) = ctx.server().get_cn(id) {
            ctx.server().broadcast_except_chan1(
                id,
                &Client {
                    client_num: cn as i32,
                    packet: packet.clone(),
                },
            );
        }
    }

    async fn map(&self, ctx: &mut Context, data: &bytes::Bytes) {
        let id = ctx.id();
        let server = ctx.server();

        *server.map_data.write().unwrap() = Some(data.clone());

        let name = server.players.get(id).name.clone();
        let cn_str = server.get_cn(id).map(|c| c.to_string()).unwrap_or_else(|| "?".to_string());
        info!("[MAP] {name} (cn {cn_str}) uploaded a new map.");

        server.broadcast_chan1(&channel1::ServMsg {
            text: format!("[{name} uploaded the map, use \"/getmap\" to receive it]")
                .lossy_cubestring(),
        });
    }

    async fn get_map(&self, ctx: &mut Context, _packet: &channel1::GetMap) {
        let id = ctx.id();
        let server = ctx.server();

        let map_bytes = server.map_data.read().unwrap().clone();
        let Some(data) = map_bytes else { return };

        server.send_chan2(
            id,
            &channel2::SendMap {
                data: data.to_vec(),
            },
        );

        let clipboard_packets: Vec<(i32, i32, Vec<u8>)> = server
            .players
            .map
            .iter()
            .filter(|e| *e.key() != id)
            .filter_map(|e| {
                let owner_id = *e.key();
                server.get_cn(owner_id).and_then(|cn| {
                    e.value()
                        .clipboard
                        .as_ref()
                        .map(|cb| (cn as i32, cb.unpack_len, cb.data.clone()))
                })
            })
            .collect();
        for (owner_cn, unpack_len, data) in clipboard_packets {
            server.send_chan1(
                id,
                &ClipboardServer {
                    client_num: owner_cn,
                    unpack_len,
                    data,
                },
            );
        }
    }

    async fn master_mode(&self, ctx: &mut Context, packet: &channel1::MasterModeClient) {
        if ctx.server().get_privilege(ctx.id()) >= Privilege::Master {
            ctx.server().set_master_mode(packet.mode);
        }
    }

    async fn map_vote(&self, ctx: &mut Context, packet: &channel1::MapVote) {
        let id = ctx.id();
        let server = ctx.server();
        let privilege = server.get_privilege(id);
        let prev_map = server.map_name.read().unwrap().clone();

        match server.master_mode() {
            MasterMode::Open => {
                server
                    .map_votes
                    .write()
                    .unwrap()
                    .insert(id, packet.map_name.clone());

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
        relay_coop(ctx.server(), ctx.id(), packet);
    }
    async fn edit_face(&self, ctx: &mut Context, packet: &channel1::EditFace) {
        relay_coop(ctx.server(), ctx.id(), packet);
    }
    async fn replace(&self, ctx: &mut Context, packet: &channel1::Replace) {
        relay_coop(ctx.server(), ctx.id(), packet);
    }
    async fn edit_v_slot(&self, ctx: &mut Context, packet: &channel1::EditVSlot) {
        relay_coop(ctx.server(), ctx.id(), packet);
    }
    async fn flip(&self, ctx: &mut Context, packet: &channel1::Flip) {
        relay_coop(ctx.server(), ctx.id(), packet);
    }
    async fn edit_var(&self, ctx: &mut Context, packet: &channel1::EditVar) {
        relay_coop(ctx.server(), ctx.id(), packet);
    }
    async fn edit_mat(&self, ctx: &mut Context, packet: &channel1::EditMat) {
        relay_coop(ctx.server(), ctx.id(), packet);
    }
    async fn edit_mode(&self, ctx: &mut Context, packet: &channel1::EditMode) {
        let id = ctx.id();
        if let Some(mut p) = ctx.server().players.try_get_mut(id) {
            if packet.is_editing {
                p.edit_state = p.client_state;
                p.client_state = ClientState::Editing;
            } else {
                p.client_state = p.edit_state;
            }
        }
        relay_coop(ctx.server(), ctx.id(), packet);
    }
    async fn rotate(&self, ctx: &mut Context, packet: &channel1::Rotate) {
        relay_coop(ctx.server(), ctx.id(), packet);
    }
    async fn del_cube(&self, ctx: &mut Context, packet: &channel1::DelCube) {
        relay_coop(ctx.server(), ctx.id(), packet);
    }
    async fn remip(&self, ctx: &mut Context, packet: &channel1::Remip) {
        relay_coop(ctx.server(), ctx.id(), packet);
    }

    async fn edit_ent(&self, ctx: &mut Context, packet: &channel1::EditEnt) {
        let id = ctx.id();
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

        if let Some(cn) = server.get_cn(id) {
            server.broadcast_except_chan1(
                id,
                &Client {
                    client_num: cn as i32,
                    packet: packet.clone(),
                },
            );
        }

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
        relay_coop(ctx.server(), ctx.id(), packet);
    }

    async fn item_list(&self, ctx: &mut Context, packet: &channel1::ItemList) {
        let id = ctx.id();
        let server = ctx.server();

        let is_spectator = server
            .players
            .try_get(id)
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
        let id = ctx.id();
        let server = ctx.server();
        let idx = packet.item_index as usize;

        let can = {
            let Some(p) = server.players.try_get(id) else {
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
                id,
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

        if let Some(mut p) = server.players.try_get_mut(id) {
            let mut quad = p.quad_millis;
            if let Some(state) = p.state.as_mut() {
                apply_pickup(state, &mut quad, item_type);
            }
            p.quad_millis = quad;
        }

        if let Some(cn) = server.get_cn(id) {
            server.broadcast_chan1(&ItemAck {
                item_index: packet.item_index,
                client_num: cn as i32,
            });
        }
    }

    async fn undo(&self, ctx: &mut Context, packet: &channel1::UndoClient) {
        if *ctx.server().game_mode.read().unwrap() != GameMode::CoopEdit {
            return;
        }
        let id = ctx.id();
        if let Some(cn) = ctx.server().get_cn(id) {
            ctx.server().broadcast_except_chan1(
                id,
                &UndoServer {
                    client_num: cn as i32,
                    unpack_len: packet.unpack_len,
                    data: packet.data.clone(),
                },
            );
        }
    }

    async fn redo(&self, ctx: &mut Context, packet: &channel1::RedoClient) {
        if *ctx.server().game_mode.read().unwrap() != GameMode::CoopEdit {
            return;
        }
        let id = ctx.id();
        if let Some(cn) = ctx.server().get_cn(id) {
            ctx.server().broadcast_except_chan1(
                id,
                &RedoServer {
                    client_num: cn as i32,
                    unpack_len: packet.unpack_len,
                    data: packet.data.clone(),
                },
            );
        }
    }

    async fn copy(&self, ctx: &mut Context, packet: &channel1::CopyClient) {
        if *ctx.server().game_mode.read().unwrap() != GameMode::CoopEdit {
            return;
        }
        let id = ctx.id();
        if let Some(mut p) = ctx.server().players.try_get_mut(id) {
            p.clipboard = None;
        }
        if let Some(cn) = ctx.server().get_cn(id) {
            ctx.server().broadcast_except_chan1(
                id,
                &Client {
                    client_num: cn as i32,
                    packet: channel1::CopyServer {
                        sel: packet.sel.clone(),
                    },
                },
            );
        }
    }

    async fn paste(&self, ctx: &mut Context, packet: &channel1::PasteClient) {
        if *ctx.server().game_mode.read().unwrap() != GameMode::CoopEdit {
            return;
        }
        let id = ctx.id();
        if let Some(cn) = ctx.server().get_cn(id) {
            ctx.server().broadcast_except_chan1(
                id,
                &Client {
                    client_num: cn as i32,
                    packet: channel1::PasteServer {
                        sel: packet.sel.clone(),
                    },
                },
            );
        }
    }

    async fn clipboard(&self, ctx: &mut Context, packet: &channel1::ClipboardClient) {
        let id = ctx.id();
        let server = ctx.server();

        let is_spectator = server
            .players
            .try_get(id)
            .map_or(false, |p| p.client_state == ClientState::Spectator);
        if is_spectator {
            return;
        }

        let pack_len = packet.data.len();
        if pack_len == 0 || pack_len > (1 << 16) || packet.unpack_len <= 0 {
            if let Some(mut p) = server.players.try_get_mut(id) {
                p.clipboard = None;
            }
            return;
        }

        if let Some(mut p) = server.players.try_get_mut(id) {
            p.clipboard = Some(ClipboardData {
                unpack_len: packet.unpack_len,
                data: packet.data.clone(),
            });
        }

        if let Some(cn) = server.get_cn(id) {
            server.broadcast_except_chan1(
                id,
                &ClipboardServer {
                    client_num: cn as i32,
                    unpack_len: packet.unpack_len,
                    data: packet.data.clone(),
                },
            );
        }
    }
}
