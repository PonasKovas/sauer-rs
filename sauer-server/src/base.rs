//! The built-in Sauerbraten base plugin, always present as the innermost plugin.
//! User plugins run before it and can cancel any handler.

use crate::Status;
use crate::{
    Server,
    items::{apply_pickup, canpickup},
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
    data::{Cn, GameMode, ItemType, MasterMode, PlayerState, Privilege, SetTeamReason},
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
    if *server.game_mode.read() != GameMode::CoopEdit {
        return;
    }
    if let Some(cn) = server.get_cn(id) {
        server.broadcast_except_chan1(
            id,
            &Client {
                client_num: cn,
                packet: pkt.clone(),
            },
        );
    }
}

/// Like the engine's `setmaster`: once nobody holds master, the mode goes back
/// to open. Unlike the engine, bans go too: nobody left can lift them.
fn reopen_if_masterless(server: &Server) {
    let has_master = server.players.any(|_, p| p.privilege >= Privilege::Master);
    if !has_master {
        server.clear_bans();
        let reopen = *server.reopen_when_masterless.read();
        if reopen && server.master_mode() != MasterMode::Open {
            server.set_master_mode(MasterMode::Open);
        }
    }
}

/// Engine `MAXTEAMLEN`.
const MAX_TEAM_LEN: usize = 4;

fn priv_name(p: Privilege) -> &'static str {
    match p {
        Privilege::Admin => "admin",
        Privilege::Auth => "auth",
        _ => "master",
    }
}

/// Returns true if `target_id` is alive with a matching `life_sequence`.
/// Guards against hitting a player who has already respawned.
fn check_hit(server: &Server, target_id: u64, life_sequence: i32) -> bool {
    server
        .players
        .with(target_id, |p| {
            p.status == Status::Alive
                && matches!(p.state.as_ref(), Some(s) if s.life_sequence == life_sequence)
        })
        .unwrap_or(false)
}

// ── Map change helper ─────────────────────────────────────────────────────────

/// Updates server state for a map change, broadcasts `N_MAPCHANGE` to all clients,
/// and clears all outstanding votes.
fn do_map_change(server: &Server, map_name: cube_string::CubeString, mode: GameMode) {
    *server.map_name.write() = map_name.clone();
    *server.game_mode.write() = mode;
    server.map_votes.write().clear();
    server.items.write().await_list();
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

                server.players.for_each_mut(|_, p| {
                    if p.quad_millis > 0 {
                        p.quad_millis = (p.quad_millis - 100).max(0);
                    }
                });

                let spawned = server.items.write().tick(100);
                for i in spawned {
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

        // The player entry exists before the cn does, so code that finds the
        // id through its cn always finds its data too.
        let session_id = init_player(&server.players, id);
        let cn = match server.assign_cn(id) {
            Some(c) => c,
            None => {
                clear_player(&server.players, id);
                server.disconnect(id);
                ctx.cancel();
                return;
            }
        };

        server.send_chan1(
            id,
            &ServInfo {
                client_num: cn,
                protocol: sauer_proto::PROTOCOL_VERSION,
                session_id,
                has_password: *server.has_password.read(),
                server_desc: server.server_desc.read().clone(),
                server_auth: server.server_auth.read().clone(),
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
                let gamemode = *server.game_mode.read() as i32;
                let mastermode = *server.master_mode.read() as i32;
                let has_password = *server.has_password.read();
                let mm = if has_password { 4 } else { mastermode };
                let map_name = server.map_name.read().clone();
                let server_desc = server.server_desc.read().clone();

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
                        let send = |resp| {
                            server
                                .send_server_query(ctx.addr(), &ServerQueryResponse::ExtInfo(resp))
                        };
                        let ids = match client_number {
                            Some(cn) => match Cn::try_from(*cn).ok().and_then(|c| server.get_id(c))
                            {
                                Some(id) => vec![id],
                                None => {
                                    send(ExtInfoResponse::PlayerStats {
                                        client_number: Some(*cn),
                                        ok: false,
                                    });
                                    return;
                                }
                            },
                            None => server.client_ids(),
                        };
                        let stats: Vec<(i32, ExtInfoResponse)> = ids
                            .into_iter()
                            .filter_map(|id| ext_player_stats(server, id))
                            .collect();
                        send(ExtInfoResponse::PlayerStats {
                            client_number: *client_number,
                            ok: true,
                        });
                        send(ExtInfoResponse::PlayerStatsIds {
                            ids: stats.iter().map(|(cn, _)| *cn).collect(),
                        });
                        for (_, resp) in stats {
                            send(resp);
                        }
                    }
                    ExtInfoRequest::TeamScore => {
                        let game_mode = *server.game_mode.read() as i32;
                        let scores = if server.game_mode.read().has_teams() {
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
            .with(id, |p| p.privilege != Privilege::None)
            .unwrap_or(false);

        server.map_votes.write().remove(&id);

        // The cn goes before the player entry, the reverse of `connected`.
        if let Some(cn) = server.free_cn(id) {
            server.broadcast_chan1(&channel1::ClientDisconnected { client_num: cn });
        }
        clear_player(&server.players, id);

        if had_privilege {
            server.broadcast_current_master();
            reopen_if_masterless(server);
        }
        // Engine `noclients`: bans clear once the server is empty. This one is
        // still in the client list until this hook returns.
        if server.client_count() <= 1 {
            server.clear_bans();
        }
    }

    async fn connect(&self, ctx: &mut Context, packet: &channel1::Connect) {
        let id = ctx.id();
        let server = ctx.server();

        let cn_str = server
            .get_cn(id)
            .map(|c| c.to_string())
            .unwrap_or_else(|| "?".to_string());
        info!("{} (cn {}) connected.", packet.name, cn_str);

        if server
            .peer_addr(id)
            .is_some_and(|a| server.is_banned(a.ip()))
        {
            info!("{} (cn {}) is banned, disconnecting.", packet.name, cn_str);
            server.disconnect_with(id, crate::bans::DISC_IPBAN);
            return;
        }

        server.init_name(id, packet.name.clone());
        server.init_model(id, packet.player_model);

        if server.master_mode() == MasterMode::Locked
            && server.get_privilege(id) < Privilege::Master
        {
            server
                .players
                .with_mut(id, |p| p.status = Status::Spectator);
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

        let spawned_state = server.players.with_mut(id, |p| {
            if p.status != Status::Dead {
                return None;
            }
            let state = p.state.as_mut()?;
            state.life_sequence = packet.life_sequence;
            state.gun_select = packet.gun_select;
            let state = state.clone();
            p.status = Status::Alive;
            Some(state)
        });

        if let Some(state) = spawned_state.flatten() {
            if let Some(cn) = server.get_cn(id) {
                server.broadcast_except_chan1(
                    id,
                    &Client {
                        client_num: cn,
                        packet: SpawnServer { state },
                    },
                );
            }
        }
    }

    async fn spectator(&self, ctx: &mut Context, packet: &channel1::SpectatorClient) {
        let id = ctx.id();
        let server = ctx.server();
        let Some(target_id) = server.get_id(packet.spectator_cn) else {
            return;
        };

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

        let fired = server.players.with_mut(id, |p| {
            if p.status != Status::Alive {
                return false;
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
            true
        });
        if fired != Some(true) {
            return;
        }

        if let Some(cn) = server.get_cn(id) {
            server.broadcast_except_chan1(
                id,
                &ShotFx {
                    client_num: cn,
                    gun: packet.gun,
                    id: packet.id,
                    from: packet.from,
                    to: packet.to,
                },
            );
        }

        for hit in &packet.hits {
            let Some(target_id) = server.get_id(hit.target_cn) else {
                continue;
            };
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

        let alive = server.players.with(id, |p| p.status == Status::Alive);
        if alive != Some(true) {
            return;
        }

        if let Some(cn) = server.get_cn(id) {
            server.broadcast_chan1(&ExplodeFx {
                client_num: cn,
                gun: packet.gun,
                id: packet.id,
            });
        }

        for hit in &packet.hits {
            let Some(target_id) = server.get_id(hit.target_cn) else {
                continue;
            };
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
        // Like the engine, a client only moves itself: the cn inside the
        // packet is passed on as is, so it must be the sender's.
        if ctx.server().get_cn(ctx.id()) != Some(packet.client_num) {
            return;
        }
        ctx.server().set_position(ctx.id(), packet.pos);
        ctx.server().broadcast_except_chan0(ctx.id(), packet);
    }

    async fn teleport(&self, ctx: &mut Context, packet: &channel0::Teleport) {
        if ctx.server().get_cn(ctx.id()) != Some(packet.client_num) {
            return;
        }
        ctx.server().broadcast_except_chan0(ctx.id(), packet);
    }

    async fn jumppad(&self, ctx: &mut Context, packet: &channel0::JumpPad) {
        if ctx.server().get_cn(ctx.id()) != Some(packet.client_num) {
            return;
        }
        ctx.server().broadcast_except_chan0(ctx.id(), packet);
    }

    async fn text(&self, ctx: &mut Context, packet: &channel1::Text) {
        let id = ctx.id();
        let bytes = packet.text.as_bytes();

        if bytes.first() == Some(&b'#') {
            let args = parse_command_args(bytes);
            let cmd = args.get(0).and_then(|s| s.as_str()).unwrap_or("");

            if cmd == "#help" {
                let mut cmds = ctx.server().commands.read().clone();
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
                lines.extend(ctx.server().help_lines.read().iter().cloned());
                crate::ui::send_banner(ctx.server(), ctx.id(), &lines);
            } else if !ctx.server().commands.read().iter().any(|c| c.name == cmd) {
                crate::ui::send_warn(
                    ctx.server(),
                    id,
                    &format!("Unknown command {cmd}. Type #help to see all commands."),
                );
            } else if let Some(plugin) = self.plugin.upgrade() {
                plugin.on_command(ctx, &args).await;
            }
        } else {
            let mut text = packet.text.clone();
            if let Some(plugin) = self.plugin.upgrade() {
                plugin.on_chat(ctx, &mut text).await;
            }
            if !ctx.cancelled {
                let name = ctx.server().player_name(id);
                info!("[CHAT] {name}: {text}");
                if let Some(cn) = ctx.server().get_cn(id) {
                    ctx.server().broadcast_except_chan1(
                        id,
                        &Client {
                            client_num: cn,
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
        ctx.server().players.with_mut(id, |p| p.ping = packet.ping);
        if let Some(cn) = ctx.server().get_cn(id) {
            ctx.server().broadcast_except_chan1(
                id,
                &Client {
                    client_num: cn,
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
                    client_num: cn,
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
                    client_num: cn,
                    packet: packet.clone(),
                },
            );
        }
    }

    async fn map(&self, ctx: &mut Context, data: &bytes::Bytes) {
        let id = ctx.id();
        let server = ctx.server();

        *server.map_data.write() = Some(data.clone());

        let name = server.player_name(id);
        let cn_str = server
            .get_cn(id)
            .map(|c| c.to_string())
            .unwrap_or_else(|| "?".to_string());
        info!("[MAP] {name} (cn {cn_str}) uploaded a new map.");

        server.broadcast_chan1(&channel1::ServMsg {
            text: format!("[{name} uploaded the map, use \"/getmap\" to receive it]")
                .lossy_cubestring(),
        });
    }

    async fn get_map(&self, ctx: &mut Context, _packet: &channel1::GetMap) {
        let id = ctx.id();
        let server = ctx.server();

        let map_bytes = server.map_data.read().clone();
        let Some(data) = map_bytes else { return };

        server.send_chan2(
            id,
            &channel2::SendMap {
                data: data.to_vec(),
            },
        );

        let clipboard_packets = server.players.filter_map(|owner_id, p| {
            if owner_id == id {
                return None;
            }
            let cb = p.clipboard.as_ref()?;
            let cn = server.get_cn(owner_id)?;
            Some((cn, cb.payload.clone()))
        });
        for (owner_cn, payload) in clipboard_packets {
            server.send_chan1(
                id,
                &ClipboardServer {
                    client_num: owner_cn,
                    payload: Some(payload),
                },
            );
        }
    }

    async fn master_mode(&self, ctx: &mut Context, packet: &channel1::MasterModeClient) {
        if ctx.server().get_privilege(ctx.id()) >= Privilege::Master {
            ctx.server().set_master_mode(packet.mode);
        }
    }

    /// Mirrors the engine's `N_SETMASTER` handling (no passwords or auth).
    async fn set_master(&self, ctx: &mut Context, packet: &channel1::SetMaster) {
        let id = ctx.id();
        let server = ctx.server();
        let Some(target) = server.get_id(packet.client_num) else {
            return;
        };
        let own = server.get_privilege(id);
        let theirs = server.get_privilege(target);

        let new = if target != id {
            // Give or take another player's master.
            if own < Privilege::Master || theirs >= own || (packet.val && theirs != Privilege::None)
            {
                return;
            }
            if packet.val {
                Privilege::Master
            } else {
                Privilege::None
            }
        } else if packet.val {
            // Claim for yourself: admin with the right password, otherwise master.
            let want = if server.check_admin_pass(id, packet.hash.as_bytes()) {
                Privilege::Admin
            } else {
                Privilege::Master
            };
            if own >= want {
                return;
            }
            if want == Privilege::Master {
                let spectating = server
                    .players
                    .with(id, |p| p.status == Status::Spectator)
                    .unwrap_or(true);
                if spectating {
                    crate::ui::send_warn(server, id, "Spectators may not claim master.");
                    return;
                }
                let taken = server
                    .players
                    .any(|other, p| other != id && p.privilege != Privilege::None);
                if taken {
                    crate::ui::send_warn(server, id, "Master is already claimed.");
                    return;
                }
            }
            want
        } else if own == Privilege::None {
            return;
        } else {
            Privilege::None
        };

        let name = server.player_name(target);
        let msg = match new {
            Privilege::None => format!("{} relinquished {}", name.to_utf8(), priv_name(theirs)),
            p => format!("{} claimed {}", name.to_utf8(), priv_name(p)),
        };
        server.broadcast_chan1(&channel1::ServMsg {
            text: msg.lossy_cubestring(),
        });
        server.set_privilege(target, new);
        reopen_if_masterless(server);
    }

    /// Engine `trykick`: kicks and bans the IP for 4 hours. Admins can't be kicked.
    async fn kick(&self, ctx: &mut Context, packet: &channel1::Kick) {
        let id = ctx.id();
        let server = ctx.server();
        let own = server.get_privilege(id);
        let Some(victim) = server.get_id(packet.victim_cn) else {
            return;
        };
        let theirs = server.get_privilege(victim);
        if own < Privilege::Master || victim == id || theirs > own || theirs >= Privilege::Admin {
            return;
        }
        let Some(ip) = server.peer_addr(victim).map(|a| a.ip()) else {
            return;
        };

        let kicker = server.player_name(id).to_utf8();
        let kicked = server.player_name(victim).to_utf8();
        let reason = packet.reason.to_utf8();
        let msg = if reason.is_empty() {
            format!("{kicker} kicked {kicked}")
        } else {
            format!("{kicker} kicked {kicked} because: {reason}")
        };
        server.broadcast_chan1(&channel1::ServMsg {
            text: msg.lossy_cubestring(),
        });

        server.ban_ip(ip);
        // Everyone else on that IP goes too, except the kicker and anyone above them.
        for other in server.client_ids() {
            let same_ip = server.peer_addr(other).is_some_and(|a| a.ip() == ip);
            let p = server.get_privilege(other);
            if other != id && same_ip && p <= own && p < Privilege::Admin {
                server.disconnect_with(other, crate::bans::DISC_KICK);
            }
        }
    }

    async fn clear_bans(&self, ctx: &mut Context, _packet: &channel1::ClearBans) {
        let server = ctx.server();
        if server.get_privilege(ctx.id()) >= Privilege::Master {
            server.clear_bans();
            server.broadcast_chan1(&channel1::ServMsg {
                text: "cleared all bans".lossy_cubestring(),
            });
        }
    }

    /// Engine `N_SETTEAM`: masters move players between teams in team modes.
    async fn set_team(&self, ctx: &mut Context, packet: &channel1::SetTeamClient) {
        let server = ctx.server();
        if server.get_privilege(ctx.id()) < Privilege::Master
            || !server.game_mode.read().has_teams()
            || packet.team.as_bytes().is_empty()
        {
            return;
        }
        let Some(target) = server.get_id(packet.client_num) else {
            return;
        };
        let stripped = packet.team.strip_colors();
        let team = CubeString::from_bytes(
            stripped
                .as_bytes()
                .iter()
                .take(MAX_TEAM_LEN)
                .copied()
                .collect(),
        );
        let same = server.players.with(target, |p| p.team == team);
        if same == Some(false) {
            server.set_team(target, team, SetTeamReason::Forced);
        }
    }

    async fn map_vote(&self, ctx: &mut Context, packet: &channel1::MapVote) {
        let id = ctx.id();
        let server = ctx.server();
        let privilege = server.get_privilege(id);
        let prev_map = server.map_name.read().clone();

        match server.master_mode() {
            MasterMode::Open => {
                server.map_votes.write().insert(id, packet.map_name.clone());

                let total_clients = server.client_count();
                let vote_count = server
                    .map_votes
                    .read()
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

        let new_map = server.map_name.read().clone();
        if new_map != prev_map {
            let mode = *server.game_mode.read();
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
        let server = ctx.server();
        if *server.game_mode.read() != GameMode::CoopEdit {
            return;
        }
        let changed = server
            .players
            .with_mut(ctx.id(), |p| p.status.set_editing(packet.is_editing));
        if changed != Some(true) {
            return;
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

        if *server.game_mode.read() != GameMode::CoopEdit {
            return;
        }

        let idx = packet.ent_id as usize;
        let new_item = server
            .items
            .write()
            .edit(idx, ItemType::try_from(packet.type_).ok());

        if let Some(cn) = server.get_cn(id) {
            server.broadcast_except_chan1(
                id,
                &Client {
                    client_num: cn,
                    packet: packet.clone(),
                },
            );
        }

        if new_item {
            server.broadcast_chan1(&ItemSpawn {
                item_index: idx as i32,
            });
        }
    }

    async fn new_map(&self, ctx: &mut Context, packet: &channel1::NewMap) {
        if packet.size >= 0 {
            ctx.server().items.write().clear();
        }
        relay_coop(ctx.server(), ctx.id(), packet);
    }

    async fn item_list(&self, ctx: &mut Context, packet: &channel1::ItemList) {
        let id = ctx.id();
        let server = ctx.server();

        let is_spectator = server
            .players
            .with(id, |p| p.status == Status::Spectator)
            .unwrap_or(false);

        let num_players = server.client_count();
        let mut items = server.items.write();
        if !items.is_awaiting_list() || is_spectator {
            return;
        }
        items.load(
            packet
                .items
                .iter()
                .map(|item| (item.item_index as usize, item.item_type)),
            num_players,
        );
    }

    async fn item_pickup(&self, ctx: &mut Context, packet: &channel1::ItemPickup) {
        let id = ctx.id();
        let server = ctx.server();
        let idx = packet.item_index as usize;
        let num_players = server.client_count();

        // Checked and taken under one items lock, so two players can't both
        // get the item and a map change can't empty the list in between.
        let picked = server.players.with_mut(id, |p| {
            if p.status != Status::Alive {
                return None;
            }
            let state = p.state.as_mut()?;
            let quad_millis = p.quad_millis;
            let taken = server.items.write().take(idx, num_players, |item_type| {
                canpickup(state, quad_millis, item_type)
            });
            let Some(item_type) = taken else {
                return Some(false);
            };
            apply_pickup(state, &mut p.quad_millis, item_type);
            Some(true)
        });

        match picked {
            // Not alive, or never welcomed.
            None | Some(None) => {}
            Some(Some(true)) => {
                if let Some(cn) = server.get_cn(id) {
                    server.broadcast_chan1(&ItemAck {
                        item_index: packet.item_index,
                        client_num: Some(cn),
                    });
                }
            }
            Some(Some(false)) => server.send_chan1(
                id,
                &ItemAck {
                    item_index: packet.item_index,
                    client_num: None,
                },
            ),
        }
    }

    async fn undo(&self, ctx: &mut Context, packet: &channel1::UndoClient) {
        if *ctx.server().game_mode.read() != GameMode::CoopEdit {
            return;
        }
        let id = ctx.id();
        if let Some(cn) = ctx.server().get_cn(id) {
            ctx.server().broadcast_except_chan1(
                id,
                &UndoServer {
                    client_num: cn,
                    payload: packet.payload.clone(),
                },
            );
        }
    }

    async fn redo(&self, ctx: &mut Context, packet: &channel1::RedoClient) {
        if *ctx.server().game_mode.read() != GameMode::CoopEdit {
            return;
        }
        let id = ctx.id();
        if let Some(cn) = ctx.server().get_cn(id) {
            ctx.server().broadcast_except_chan1(
                id,
                &RedoServer {
                    client_num: cn,
                    payload: packet.payload.clone(),
                },
            );
        }
    }

    async fn copy(&self, ctx: &mut Context, packet: &channel1::CopyClient) {
        if *ctx.server().game_mode.read() != GameMode::CoopEdit {
            return;
        }
        let id = ctx.id();
        ctx.server().players.with_mut(id, |p| p.clipboard = None);
        if let Some(cn) = ctx.server().get_cn(id) {
            ctx.server().broadcast_except_chan1(
                id,
                &Client {
                    client_num: cn,
                    packet: channel1::CopyServer {
                        sel: packet.sel.clone(),
                    },
                },
            );
        }
    }

    async fn paste(&self, ctx: &mut Context, packet: &channel1::PasteClient) {
        if *ctx.server().game_mode.read() != GameMode::CoopEdit {
            return;
        }
        let id = ctx.id();
        if let Some(cn) = ctx.server().get_cn(id) {
            ctx.server().broadcast_except_chan1(
                id,
                &Client {
                    client_num: cn,
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
            .with(id, |p| p.status == Status::Spectator)
            .unwrap_or(false);
        if is_spectator {
            return;
        }

        server.players.with_mut(id, |p| {
            p.clipboard = packet
                .payload
                .clone()
                .map(|payload| ClipboardData { payload });
        });

        // An empty one is passed on too, so others drop their copy.
        if let Some(cn) = server.get_cn(id) {
            server.broadcast_except_chan1(
                id,
                &ClipboardServer {
                    client_num: cn,
                    payload: packet.payload.clone(),
                },
            );
        }
    }
}

/// One player's ExtInfo stats reply, with their cn.
fn ext_player_stats(
    server: &Server,
    id: u64,
) -> Option<(i32, sauer_proto::server_query::ExtInfoResponse)> {
    let cn = i32::from(server.get_cn(id)?);
    let insta = server.game_mode.read().is_insta();
    // Only the first three octets go out, like the engine.
    let ip = server.peer_addr(id).map(|a| match a.ip() {
        std::net::IpAddr::V4(v4) => {
            let [a, b, c, _] = v4.octets();
            [a, b, c]
        }
        _ => [0, 0, 0],
    });
    server.players.with(id, |p| {
        let state = p
            .state
            .clone()
            .unwrap_or_else(|| crate::players::state::make_spawn_state(insta, 0));
        let stats = sauer_proto::server_query::ExtInfoResponse::PlayerStatsStats {
            client_num: cn,
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
            state: p.status.client_state(),
            ip,
        };
        (cn, stats)
    })
}
