//! The built-in Sauerbraten base plugin, always present as the innermost plugin.
//! User plugins run before it and can cancel any handler.

use crate::{
    Server, Status,
    items::{apply_pickup, canpickup},
    players::combat::{explosion_damage, gun_damage_per_ray},
    players::{clear_player, init_player},
    plugin::{Context, Flow, Plugin},
};
use cube_string::{CubeStr, CubeString, IntoCubeStringLossy};
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

    /// Engine `N_SWITCHTEAM`/`N_SETTEAM`: in team modes, moves `target` to the
    /// filtered `team`, killing them first if alive.
    async fn change_team(
        &self,
        ctx: &mut Context,
        target: u64,
        team: &CubeStr,
        reason: SetTeamReason,
    ) {
        let server = ctx.server().clone();
        let team = team.filter_text(false, MAX_TEAM_LEN);
        if !server.game.read().mode.has_teams() || team.as_bytes().is_empty() {
            return;
        }
        let Some((same, spectator)) = server
            .players
            .with(target, |p| (p.team == team, p.status == Status::Spectator))
        else {
            return;
        };
        if same {
            return;
        }
        if server.kill_player(target) {
            if let Some(plugin) = self.plugin.upgrade() {
                plugin.on_player_died(ctx, target, target).await;
            }
        }
        // A spectator switching teams is not announced.
        let reason = match reason {
            SetTeamReason::Switched if spectator => SetTeamReason::Silent,
            r => r,
        };
        server.set_team(target, team, reason);
    }
}

// ── Helpers ───────────────────────────────────────────────────────────────────

/// Relay a coop-edit packet from `id` to all other clients, but only in CoopEdit mode.
fn relay_coop<T>(server: &Server, id: u64, pkt: &T)
where
    Client<T>: sauer_proto::PacketSequence<sauer_proto::route::S2C<sauer_proto::route::Chan1>>,
    T: Clone + sauer_proto::PacketDescriptor + std::fmt::Debug,
{
    if server.game.read().mode != GameMode::CoopEdit {
        return;
    }
    server.relay_from(id, pkt.clone());
}

/// Like the engine's `setmaster`: once nobody holds master, the mode goes back
/// to open. Unlike the engine, bans go too: nobody left can lift them.
fn reopen_if_masterless(server: &Server) {
    let has_master = server.players.any(|_, p| p.privilege >= Privilege::Master);
    if !has_master {
        server.clear_bans();
        let reopen = server.config.read().reopen_when_masterless;
        if reopen && server.master_mode() != MasterMode::Open {
            server.set_master_mode(MasterMode::Open);
        }
    }
}

/// Engine `MAXNAMELEN`.
const MAX_NAME_LEN: usize = 15;
/// Engine `MAXTEAMLEN`.
const MAX_TEAM_LEN: usize = 4;

/// A client's name the way the engine keeps it: filtered and never empty.
fn clean_name(name: &CubeStr) -> CubeString {
    let name = name.filter_text(false, MAX_NAME_LEN);
    if name.as_bytes().is_empty() {
        "unnamed".lossy_cubestring()
    } else {
        name
    }
}

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
    server.game.write().change_map(map_name.clone(), mode);
    server.broadcast(&MapChange {
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

                let spawned = server.game.write().items.tick(100);
                for i in spawned {
                    server.broadcast(&ItemSpawn {
                        item_index: i as i32,
                    });
                }
            }
        });
    }

    async fn connected(&self, ctx: &mut Context) -> Flow {
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
                return Flow::Cancel;
            }
        };

        server.send(
            id,
            &ServInfo {
                client_num: cn,
                protocol: sauer_proto::PROTOCOL_VERSION,
                session_id,
                has_password: server.config.read().has_password,
                server_desc: server.config.read().desc.clone(),
                server_auth: server.config.read().auth.clone(),
            },
        );
        Flow::Continue
    }

    async fn server_query(
        &self,
        ctx: &mut crate::plugin::ServerQueryContext,
        request: &sauer_proto::server_query::ServerQueryRequest,
    ) -> Flow {
        use sauer_proto::server_query::{
            ExtInfoRequest, ExtInfoResponse, PlayerStats, PlayerStatsBody, ScoreData,
            ServerQueryRequest, ServerQueryResponse,
        };

        let server = ctx.server();
        match request {
            ServerQueryRequest::ServerInfo { ping_id } => {
                let gamemode = server.game.read().mode as i32;
                let mastermode = server.game.read().master_mode as i32;
                let has_password = server.config.read().has_password;
                let mm = if has_password { 4 } else { mastermode };
                let map_name = server.game.read().map_name.clone();
                let server_desc = server.config.read().desc.clone();

                let resp = ServerQueryResponse::ServerInfo {
                    ping_id: *ping_id,
                    num_clients: server.client_count() as i32,
                    protocol_version: sauer_proto::PROTOCOL_VERSION,
                    game_mode: gamemode,
                    remaining_time: 0, // TODO
                    max_clients: server.config.read().max_clients as i32,
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
                        let send = |body| {
                            let resp = ExtInfoResponse::PlayerStats {
                                client_number: *client_number,
                                body,
                            };
                            server
                                .send_server_query(ctx.addr(), &ServerQueryResponse::ExtInfo(resp))
                        };
                        let ids = match client_number {
                            Some(cn) => match Cn::try_from(*cn).ok().and_then(|c| server.get_id(c))
                            {
                                Some(id) => vec![id],
                                None => {
                                    send(PlayerStatsBody::Error);
                                    return Flow::Continue;
                                }
                            },
                            None => server.joined_ids(),
                        };
                        let stats: Vec<PlayerStats> = ids
                            .into_iter()
                            .filter_map(|id| ext_player_stats(server, id))
                            .collect();
                        send(PlayerStatsBody::Ids(
                            stats.iter().map(|s| s.client_num).collect(),
                        ));
                        for s in stats {
                            send(PlayerStatsBody::Stats(s));
                        }
                    }
                    ExtInfoRequest::TeamScore => {
                        let mode = server.game.read().mode;
                        // Like the engine: every team someone is playing on, with its frags.
                        let scores = mode.has_teams().then(|| {
                            let mut scores: Vec<ScoreData> = Vec::new();
                            let players = server.players.filter_map(|_, p| {
                                (p.status != Status::Spectator && !p.team.is_empty())
                                    .then(|| (p.team.clone(), p.frags))
                            });
                            for (team, frags) in players {
                                match scores.iter_mut().find(|s| s.team_name == team) {
                                    Some(s) => s.score += frags,
                                    None => scores.push(ScoreData {
                                        team_name: team,
                                        score: frags,
                                        bases: None,
                                    }),
                                }
                            }
                            scores
                        });
                        server.send_server_query(
                            ctx.addr(),
                            &ServerQueryResponse::ExtInfo(ExtInfoResponse::TeamScore {
                                game_mode: mode as i32,
                                remaining_time: 0, // TODO
                                scores,
                            }),
                        );
                    }
                }
            }
        }
        Flow::Continue
    }

    async fn disconnected(&self, ctx: &mut Context) {
        let id = ctx.id();
        let server = ctx.server();

        let had_privilege = server
            .players
            .with(id, |p| p.privilege != Privilege::None)
            .unwrap_or(false);

        server.game.write().map_votes.remove(&id);

        // The cn goes before the player entry, the reverse of `connected`.
        if let Some(cn) = server.free_cn(id) {
            server.broadcast(&channel1::ClientDisconnected { client_num: cn });
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

    async fn connect(&self, ctx: &mut Context, packet: &channel1::Connect) -> Flow {
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
            return Flow::Continue;
        }

        server.init_name(id, clean_name(&packet.name));
        server.init_model(id, packet.player_model);

        if server.master_mode() == MasterMode::Locked
            && server.get_privilege(id) < Privilege::Master
        {
            server
                .players
                .with_mut(id, |p| p.status = Status::Spectator);
        }

        if matches!(server.master_mode(), MasterMode::Private) {
            return Flow::Continue;
        }

        server.welcome(id);

        if let Some(plugin) = self.plugin.upgrade() {
            plugin.on_client_welcomed(ctx).await;
        }
        Flow::Continue
    }

    async fn on_spawn(&self, ctx: &mut Context, state: &mut PlayerState) -> Flow {
        ctx.server().apply_spawn_state(ctx.id(), state.clone());
        Flow::Continue
    }

    async fn try_spawn(&self, ctx: &mut Context, _packet: &channel1::TrySpawn) -> Flow {
        let id = ctx.id();
        if let Some(mut state) = ctx.server().compute_spawn_state(id) {
            if let Some(plugin) = self.plugin.upgrade() {
                // Base applies the spawn at the end of the chain, so a cancel already stopped it.
                let _ = plugin.on_spawn(ctx, &mut state).await;
            }
        }
        Flow::Continue
    }

    async fn spawn_client(&self, ctx: &mut Context, packet: &channel1::SpawnClient) -> Flow {
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
            server.relay_from(id, SpawnServer { state });
        }
        Flow::Continue
    }

    async fn spectator(&self, ctx: &mut Context, packet: &channel1::SpectatorClient) -> Flow {
        let id = ctx.id();
        let server = ctx.server();
        let Some(target_id) = server.get_id(packet.spectator_cn) else {
            return Flow::Continue;
        };

        let has_privilege = server.get_privilege(id) >= Privilege::Master;
        let locked = server.master_mode() == MasterMode::Locked;

        if !has_privilege && (target_id != id || locked) {
            return Flow::Continue;
        }

        server.set_spectator(target_id, packet.val);
        Flow::Continue
    }

    async fn suicide(&self, ctx: &mut Context, _packet: &channel1::Suicide) -> Flow {
        let id = ctx.id();
        if ctx.server().kill_player(id) {
            if let Some(plugin) = self.plugin.upgrade() {
                plugin.on_player_died(ctx, id, id).await;
            }
        }
        Flow::Continue
    }

    async fn shoot(&self, ctx: &mut Context, packet: &channel1::Shoot) -> Flow {
        let id = ctx.id();
        let server = ctx.server().clone();

        let fired = server.players.with_mut(id, |p| {
            if p.status != Status::Alive {
                return false;
            }
            // Like the engine, an empty gun doesn't fire. The fist needs no ammo.
            if let Some(ammo) = p.state.as_mut().and_then(|s| s.ammo.of_mut(packet.gun)) {
                if *ammo <= 0 {
                    return false;
                }
                *ammo -= 1;
            }
            true
        });
        if fired != Some(true) {
            return Flow::Continue;
        }

        if let Some(cn) = server.get_cn(id) {
            server.broadcast_except(
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

            let flow = match self.plugin.upgrade() {
                Some(plugin) => plugin.on_damage(ctx, target_id, id, &mut damage).await,
                None => Flow::Continue,
            };
            if flow == Flow::Cancel || damage <= 0 {
                continue;
            }

            if server.apply_damage(target_id, id, packet.gun, damage, hit.dir) {
                if let Some(plugin) = self.plugin.upgrade() {
                    plugin.on_player_died(ctx, target_id, id).await;
                }
            }
        }
        Flow::Continue
    }

    async fn explode(&self, ctx: &mut Context, packet: &channel1::Explode) -> Flow {
        let id = ctx.id();
        let server = ctx.server().clone();

        let alive = server.players.with(id, |p| p.status == Status::Alive);
        if alive != Some(true) {
            return Flow::Continue;
        }

        if let Some(cn) = server.get_cn(id) {
            server.broadcast(&ExplodeFx {
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

            let flow = match self.plugin.upgrade() {
                Some(plugin) => plugin.on_damage(ctx, target_id, id, &mut damage).await,
                None => Flow::Continue,
            };
            if flow == Flow::Cancel || damage <= 0 {
                continue;
            }

            if server.apply_damage(target_id, id, packet.gun, damage, hit.dir) {
                if let Some(plugin) = self.plugin.upgrade() {
                    plugin.on_player_died(ctx, target_id, id).await;
                }
            }
        }
        Flow::Continue
    }

    async fn pos(&self, ctx: &mut Context, packet: &channel0::Position) -> Flow {
        // Like the engine, a client only moves itself: the cn inside the
        // packet is passed on as is, so it must be the sender's.
        if ctx.server().get_cn(ctx.id()) != Some(packet.client_num) {
            return Flow::Continue;
        }
        ctx.server().set_position(ctx.id(), packet.pos);
        ctx.server().broadcast_except(ctx.id(), packet);
        Flow::Continue
    }

    async fn teleport(&self, ctx: &mut Context, packet: &channel0::Teleport) -> Flow {
        if ctx.server().get_cn(ctx.id()) != Some(packet.client_num) {
            return Flow::Continue;
        }
        ctx.server().broadcast_except(ctx.id(), packet);
        Flow::Continue
    }

    async fn jumppad(&self, ctx: &mut Context, packet: &channel0::JumpPad) -> Flow {
        if ctx.server().get_cn(ctx.id()) != Some(packet.client_num) {
            return Flow::Continue;
        }
        ctx.server().broadcast_except(ctx.id(), packet);
        Flow::Continue
    }

    async fn text(&self, ctx: &mut Context, packet: &channel1::Text) -> Flow {
        let id = ctx.id();
        let text = packet.text.filter_text(true, usize::MAX);
        let bytes = text.as_bytes();

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
                let _ = plugin.on_command(ctx, &args).await;
            }
        } else {
            let mut text = text.clone();
            let flow = match self.plugin.upgrade() {
                Some(plugin) => plugin.on_chat(ctx, &mut text).await,
                None => Flow::Continue,
            };
            if flow == Flow::Continue {
                let name = ctx.server().player_name(id);
                info!("[CHAT] {name}: {text}");
                ctx.server().relay_from(id, channel1::Text { text });
            }
        }
        Flow::Continue
    }

    /// Engine `N_SAYTEAM`: in team modes, sent only to playing teammates.
    async fn say_team(&self, ctx: &mut Context, packet: &channel1::SayTeamClient) -> Flow {
        let id = ctx.id();
        let server = ctx.server();
        let Some(cn) = server.get_cn(id) else {
            return Flow::Continue;
        };
        let Some((team, spectator)) = server
            .players
            .with(id, |p| (p.team.clone(), p.status == Status::Spectator))
        else {
            return Flow::Continue;
        };
        if (spectator && server.get_privilege(id) < Privilege::Master)
            || !server.game.read().mode.has_teams()
            || team.as_bytes().is_empty()
        {
            return Flow::Continue;
        }
        let packet = channel1::SayTeamServer {
            client_num: cn,
            text: packet.text.filter_text(true, usize::MAX),
        };
        info!("[TEAM {team}] {}: {}", server.player_name(id), packet.text);
        for to in server.players.filter_map(|other, p| {
            (other != id && p.status != Status::Spectator && p.team == team).then_some(other)
        }) {
            server.send(to, &packet);
        }
        Flow::Continue
    }

    async fn switch_name(&self, ctx: &mut Context, packet: &channel1::SwitchName) -> Flow {
        ctx.server().set_name(ctx.id(), clean_name(&packet.name));
        Flow::Continue
    }

    async fn switch_model(&self, ctx: &mut Context, packet: &channel1::SwitchModel) -> Flow {
        ctx.server().set_model(ctx.id(), packet.model);
        Flow::Continue
    }

    async fn switch_team(&self, ctx: &mut Context, packet: &channel1::SwitchTeam) -> Flow {
        self.change_team(ctx, ctx.id(), &packet.team, SetTeamReason::Switched)
            .await;
        Flow::Continue
    }

    async fn ping(&self, ctx: &mut Context, packet: &channel1::Ping) -> Flow {
        ctx.server()
            .send(ctx.id(), &channel1::Pong { ping: packet.time });
        Flow::Continue
    }

    async fn client_ping(&self, ctx: &mut Context, packet: &channel1::ClientPing) -> Flow {
        let id = ctx.id();
        ctx.server().players.with_mut(id, |p| p.ping = packet.ping);
        ctx.server().relay_from(id, packet.clone());
        Flow::Continue
    }

    async fn taunt(&self, ctx: &mut Context, packet: &channel1::Taunt) -> Flow {
        let id = ctx.id();
        ctx.server().relay_from(id, packet.clone());
        Flow::Continue
    }

    async fn sound(&self, ctx: &mut Context, packet: &channel1::Sound) -> Flow {
        let id = ctx.id();
        ctx.server().relay_from(id, packet.clone());
        Flow::Continue
    }

    async fn map(&self, ctx: &mut Context, data: &bytes::Bytes) -> Flow {
        let id = ctx.id();
        let server = ctx.server();

        server.game.write().map_data = Some(data.clone());

        let name = server.player_name(id);
        let cn_str = server
            .get_cn(id)
            .map(|c| c.to_string())
            .unwrap_or_else(|| "?".to_string());
        info!("[MAP] {name} (cn {cn_str}) uploaded a new map.");

        server.broadcast(&channel1::ServMsg {
            text: format!("[{name} uploaded the map, use \"/getmap\" to receive it]")
                .lossy_cubestring(),
        });
        Flow::Continue
    }

    async fn get_map(&self, ctx: &mut Context, _packet: &channel1::GetMap) -> Flow {
        let id = ctx.id();
        let server = ctx.server();

        let map_bytes = server.game.read().map_data.clone();
        let Some(data) = map_bytes else {
            return Flow::Continue;
        };

        server.send(
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
            Some((cn, cb.clone()))
        });
        for (owner_cn, payload) in clipboard_packets {
            server.send(
                id,
                &ClipboardServer {
                    client_num: owner_cn,
                    payload: Some(payload),
                },
            );
        }
        Flow::Continue
    }

    async fn master_mode(&self, ctx: &mut Context, packet: &channel1::MasterModeClient) -> Flow {
        if ctx.server().get_privilege(ctx.id()) >= Privilege::Master {
            ctx.server().set_master_mode(packet.mode);
        }
        Flow::Continue
    }

    /// Mirrors the engine's `N_SETMASTER` handling (no passwords or auth).
    async fn set_master(&self, ctx: &mut Context, packet: &channel1::SetMaster) -> Flow {
        let id = ctx.id();
        let server = ctx.server();
        let Some(target) = server.get_id(packet.client_num) else {
            return Flow::Continue;
        };
        let own = server.get_privilege(id);
        let theirs = server.get_privilege(target);

        let new = if target != id {
            // Give or take another player's master.
            if own < Privilege::Master || theirs >= own || (packet.val && theirs != Privilege::None)
            {
                return Flow::Continue;
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
                return Flow::Continue;
            }
            if want == Privilege::Master {
                let spectating = server
                    .players
                    .with(id, |p| p.status == Status::Spectator)
                    .unwrap_or(true);
                if spectating {
                    crate::ui::send_warn(server, id, "Spectators may not claim master.");
                    return Flow::Continue;
                }
                let taken = server
                    .players
                    .any(|other, p| other != id && p.privilege != Privilege::None);
                if taken {
                    crate::ui::send_warn(server, id, "Master is already claimed.");
                    return Flow::Continue;
                }
            }
            want
        } else if own == Privilege::None {
            return Flow::Continue;
        } else {
            Privilege::None
        };

        let name = server.player_name(target);
        let msg = match new {
            Privilege::None => format!("{} relinquished {}", name.to_utf8(), priv_name(theirs)),
            p => format!("{} claimed {}", name.to_utf8(), priv_name(p)),
        };
        server.broadcast(&channel1::ServMsg {
            text: msg.lossy_cubestring(),
        });
        server.set_privilege(target, new);
        reopen_if_masterless(server);
        Flow::Continue
    }

    /// Engine `trykick`: kicks and bans the IP for 4 hours. Admins can't be kicked.
    async fn kick(&self, ctx: &mut Context, packet: &channel1::Kick) -> Flow {
        let id = ctx.id();
        let server = ctx.server();
        let own = server.get_privilege(id);
        let Some(victim) = server.get_id(packet.victim_cn) else {
            return Flow::Continue;
        };
        let theirs = server.get_privilege(victim);
        if own < Privilege::Master || victim == id || theirs > own || theirs >= Privilege::Admin {
            return Flow::Continue;
        }
        let Some(ip) = server.peer_addr(victim).map(|a| a.ip()) else {
            return Flow::Continue;
        };

        let kicker = server.player_name(id).to_utf8();
        let kicked = server.player_name(victim).to_utf8();
        let reason = packet.reason.to_utf8();
        let msg = if reason.is_empty() {
            format!("{kicker} kicked {kicked}")
        } else {
            format!("{kicker} kicked {kicked} because: {reason}")
        };
        server.broadcast(&channel1::ServMsg {
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
        Flow::Continue
    }

    async fn clear_bans(&self, ctx: &mut Context, _packet: &channel1::ClearBans) -> Flow {
        let server = ctx.server();
        if server.get_privilege(ctx.id()) >= Privilege::Master {
            server.clear_bans();
            server.broadcast(&channel1::ServMsg {
                text: "cleared all bans".lossy_cubestring(),
            });
        }
        Flow::Continue
    }

    /// Engine `N_SETTEAM`: masters move players between teams in team modes.
    async fn set_team(&self, ctx: &mut Context, packet: &channel1::SetTeamClient) -> Flow {
        let server = ctx.server();
        if server.get_privilege(ctx.id()) < Privilege::Master {
            return Flow::Continue;
        }
        let Some(target) = server.get_id(packet.client_num) else {
            return Flow::Continue;
        };
        self.change_team(ctx, target, &packet.team, SetTeamReason::Forced)
            .await;
        Flow::Continue
    }

    async fn map_vote(&self, ctx: &mut Context, packet: &channel1::MapVote) -> Flow {
        let id = ctx.id();
        let server = ctx.server();

        let change = match server.master_mode() {
            MasterMode::Open => {
                let total_clients = server.client_count();
                let mut game = server.game.write();
                game.map_votes.insert(id, packet.map_name.clone());
                let votes = game.map_votes.values().filter(|v| **v == packet.map_name);
                votes.count() * 2 > total_clients
            }
            _ => server.get_privilege(id) >= Privilege::Master,
        };
        if change {
            do_map_change(server, packet.map_name.clone(), packet.mode);
            if let Some(plugin) = self.plugin.upgrade() {
                plugin.map_changed(ctx, &packet.map_name, packet.mode).await;
            }
        }
        Flow::Continue
    }

    // ── Coop-edit ─────────────────────────────────────────────────────────────

    async fn edit_tex(&self, ctx: &mut Context, packet: &channel1::EditTex) -> Flow {
        relay_coop(ctx.server(), ctx.id(), packet);
        Flow::Continue
    }
    async fn edit_face(&self, ctx: &mut Context, packet: &channel1::EditFace) -> Flow {
        relay_coop(ctx.server(), ctx.id(), packet);
        Flow::Continue
    }
    async fn replace(&self, ctx: &mut Context, packet: &channel1::Replace) -> Flow {
        relay_coop(ctx.server(), ctx.id(), packet);
        Flow::Continue
    }
    async fn edit_v_slot(&self, ctx: &mut Context, packet: &channel1::EditVSlot) -> Flow {
        relay_coop(ctx.server(), ctx.id(), packet);
        Flow::Continue
    }
    async fn flip(&self, ctx: &mut Context, packet: &channel1::Flip) -> Flow {
        relay_coop(ctx.server(), ctx.id(), packet);
        Flow::Continue
    }
    async fn edit_var(&self, ctx: &mut Context, packet: &channel1::EditVar) -> Flow {
        relay_coop(ctx.server(), ctx.id(), packet);
        Flow::Continue
    }
    async fn edit_mat(&self, ctx: &mut Context, packet: &channel1::EditMat) -> Flow {
        relay_coop(ctx.server(), ctx.id(), packet);
        Flow::Continue
    }
    async fn edit_mode(&self, ctx: &mut Context, packet: &channel1::EditMode) -> Flow {
        let server = ctx.server();
        if server.game.read().mode != GameMode::CoopEdit {
            return Flow::Continue;
        }
        let changed = server
            .players
            .with_mut(ctx.id(), |p| p.status.set_editing(packet.is_editing));
        if changed != Some(true) {
            return Flow::Continue;
        }
        relay_coop(ctx.server(), ctx.id(), packet);
        Flow::Continue
    }
    async fn rotate(&self, ctx: &mut Context, packet: &channel1::Rotate) -> Flow {
        relay_coop(ctx.server(), ctx.id(), packet);
        Flow::Continue
    }
    async fn del_cube(&self, ctx: &mut Context, packet: &channel1::DelCube) -> Flow {
        relay_coop(ctx.server(), ctx.id(), packet);
        Flow::Continue
    }
    async fn remip(&self, ctx: &mut Context, packet: &channel1::Remip) -> Flow {
        relay_coop(ctx.server(), ctx.id(), packet);
        Flow::Continue
    }

    async fn edit_ent(&self, ctx: &mut Context, packet: &channel1::EditEnt) -> Flow {
        let id = ctx.id();
        let server = ctx.server();

        if server.game.read().mode != GameMode::CoopEdit {
            return Flow::Continue;
        }

        let idx = packet.ent_id as usize;
        let new_item = server
            .game
            .write()
            .items
            .edit(idx, ItemType::try_from(packet.type_).ok());

        server.relay_from(id, packet.clone());

        if new_item {
            server.broadcast(&ItemSpawn {
                item_index: idx as i32,
            });
        }
        Flow::Continue
    }

    async fn new_map(&self, ctx: &mut Context, packet: &channel1::NewMap) -> Flow {
        if packet.size >= 0 {
            ctx.server().game.write().items.clear();
        }
        relay_coop(ctx.server(), ctx.id(), packet);
        Flow::Continue
    }

    async fn item_list(&self, ctx: &mut Context, packet: &channel1::ItemList) -> Flow {
        let id = ctx.id();
        let server = ctx.server();

        let is_spectator = server
            .players
            .with(id, |p| p.status == Status::Spectator)
            .unwrap_or(false);

        let num_players = server.client_count();
        let mut game = server.game.write();
        if !game.items.is_awaiting_list() || is_spectator {
            return Flow::Continue;
        }
        game.items.load(
            packet
                .items
                .iter()
                .map(|item| (item.item_index as usize, item.item_type)),
            num_players,
        );
        Flow::Continue
    }

    async fn item_pickup(&self, ctx: &mut Context, packet: &channel1::ItemPickup) -> Flow {
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
            let taken = server
                .game
                .write()
                .items
                .take(idx, num_players, |item_type| {
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
                    server.broadcast(&ItemAck {
                        item_index: packet.item_index,
                        client_num: Some(cn),
                    });
                }
            }
            Some(Some(false)) => server.send(
                id,
                &ItemAck {
                    item_index: packet.item_index,
                    client_num: None,
                },
            ),
        }
        Flow::Continue
    }

    async fn undo(&self, ctx: &mut Context, packet: &channel1::UndoClient) -> Flow {
        if ctx.server().game.read().mode != GameMode::CoopEdit {
            return Flow::Continue;
        }
        let id = ctx.id();
        if let Some(cn) = ctx.server().get_cn(id) {
            ctx.server().broadcast_except(
                id,
                &UndoServer {
                    client_num: cn,
                    payload: packet.payload.clone(),
                },
            );
        }
        Flow::Continue
    }

    async fn redo(&self, ctx: &mut Context, packet: &channel1::RedoClient) -> Flow {
        if ctx.server().game.read().mode != GameMode::CoopEdit {
            return Flow::Continue;
        }
        let id = ctx.id();
        if let Some(cn) = ctx.server().get_cn(id) {
            ctx.server().broadcast_except(
                id,
                &RedoServer {
                    client_num: cn,
                    payload: packet.payload.clone(),
                },
            );
        }
        Flow::Continue
    }

    async fn copy(&self, ctx: &mut Context, packet: &channel1::CopyClient) -> Flow {
        if ctx.server().game.read().mode != GameMode::CoopEdit {
            return Flow::Continue;
        }
        ctx.server()
            .players
            .with_mut(ctx.id(), |p| p.clipboard = None);
        let sel = packet.sel.clone();
        ctx.server()
            .relay_from(ctx.id(), channel1::CopyServer { sel });
        Flow::Continue
    }

    async fn paste(&self, ctx: &mut Context, packet: &channel1::PasteClient) -> Flow {
        let sel = packet.sel.clone();
        relay_coop(ctx.server(), ctx.id(), &channel1::PasteServer { sel });
        Flow::Continue
    }

    async fn clipboard(&self, ctx: &mut Context, packet: &channel1::ClipboardClient) -> Flow {
        let id = ctx.id();
        let server = ctx.server();

        let is_spectator = server
            .players
            .with(id, |p| p.status == Status::Spectator)
            .unwrap_or(false);
        if is_spectator {
            return Flow::Continue;
        }

        server.players.with_mut(id, |p| {
            p.clipboard = packet.payload.clone();
        });

        // An empty one is passed on too, so others drop their copy.
        if let Some(cn) = server.get_cn(id) {
            server.broadcast_except(
                id,
                &ClipboardServer {
                    client_num: cn,
                    payload: packet.payload.clone(),
                },
            );
        }
        Flow::Continue
    }
}

/// One player's ExtInfo stats reply, with their cn.
fn ext_player_stats(server: &Server, id: u64) -> Option<sauer_proto::server_query::PlayerStats> {
    let cn = i32::from(server.get_cn(id)?);
    let insta = server.game.read().mode.is_insta();
    // Only the first three octets go out, like the engine.
    let ip = match server.peer_addr(id).map(|a| a.ip()) {
        Some(std::net::IpAddr::V4(v4)) => {
            let [a, b, c, _] = v4.octets();
            [a, b, c]
        }
        _ => [0, 0, 0],
    };
    server.players.with(id, |p| {
        let state = p
            .state
            .clone()
            .unwrap_or_else(|| crate::players::state::make_spawn_state(insta, 0));
        sauer_proto::server_query::PlayerStats {
            client_num: cn,
            ping: p.ping,
            name: p.name.clone(),
            team: p.team.clone(),
            frags: p.frags,
            flags: p.flags,
            deaths: p.deaths,
            teamkills: p.teamkills,
            accuracy: 0, // TODO
            health: state.health,
            armour: state.armour,
            gunselect: state.gun_select,
            privilege: p.privilege,
            state: p.status.client_state(),
            ip,
        }
    })
}
