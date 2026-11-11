use crate::aim::Aim;
use crate::map_state::{DeathrunState, Trap, get_server_state};
use crate::trap::frame;
use crate::ui;
use cube_string::IntoCubeStringLossy;
use sauer_map::Map;

use parking_lot::RwLock;
use rand::seq::SliceRandom;
use sauer_proto::data::{PlayerState, Privilege};
use sauer_proto::{
    Client, channel0, channel1,
    data::{EntityType, GameMode, MasterMode, SetTeamReason},
};
use sauer_server::{
    Flow, Server, Status,
    plugin::{Context, Plugin},
};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::collections::HashSet;
use std::sync::Arc;
use tokio::time::{Duration, Instant, sleep};
use tracing::info;

// ─── Config ───────────────────────────────────────────────────────────────────

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PlayConfig {
    /// Seconds without movement before a player is auto-spectated for AFK.
    #[serde(default = "default_afk_timeout")]
    pub afk_timeout_secs: u64,
}

fn default_afk_timeout() -> u64 {
    10
}

impl Default for PlayConfig {
    fn default() -> Self {
        Self {
            afk_timeout_secs: default_afk_timeout(),
        }
    }
}

#[derive(PartialEq, Clone, Copy)]
pub enum RoundState {
    NotStarted,
    MapLoading,
    Playing,
    Ended,
}

pub struct LoadedMap {
    pub state: Map,
    pub traps: Vec<Trap>,
}

pub struct PlayMaps {
    pub maps: HashMap<String, LoadedMap>,
    pub rotation: Vec<String>,
    pub rotation_idx: usize,
    pub current_map: Option<String>,
    pub evil_id: Option<u64>,
    pub round_state: RoundState,
    pub round_epoch: u64,
    pub round_task: Option<tokio::task::JoinHandle<()>>,
    pub spawned_this_round: HashSet<u64>,
    pub round_start_time: Option<Instant>,
    pub finished_players: Vec<(u64, Duration)>,
    /// Its own lock: every position packet updates it.
    pub afk: parking_lot::Mutex<Afk>,
}

impl Default for PlayMaps {
    fn default() -> Self {
        Self {
            maps: HashMap::new(),
            rotation: Vec::new(),
            rotation_idx: 0,
            current_map: None,
            evil_id: None,
            round_state: RoundState::NotStarted,
            round_epoch: 0,
            round_task: None,
            spawned_this_round: HashSet::new(),
            round_start_time: None,
            finished_players: Vec::new(),
            afk: Default::default(),
        }
    }
}

impl LoadedMap {
    /// Loads a map file with its traps. Slow for big maps: call it without
    /// holding the `PlayMaps` lock, then [`PlayMaps::insert`] the result.
    pub fn parse(name: &str, data: &[u8]) -> Result<Self, String> {
        let mut state = Map::from_bytes(data).map_err(|e| e.to_string())?;
        sauer_kit::virt_ents::strip(&mut state.entities);

        let loaded = crate::trap::persist::load(&state);
        for w in &loaded.warnings {
            tracing::warn!("map '{name}': {w}");
        }
        let traps = loaded.traps;
        if !traps.is_empty() {
            info!("Loaded {} traps from map '{name}'", traps.len());
        }
        // Players never save play maps, and the server keeps trap geometry in
        // memory, so the vault is just dead weight for them.
        if let Some(v) = loaded.vault {
            crate::trap::vault::erase(&mut state, &v);
        }

        crate::trap::placement::rebuild_all_trap_buttons(&mut state, &traps);
        Ok(Self { state, traps })
    }
}

impl PlayMaps {
    /// Adds a map to the rotation, or replaces the one with that name.
    pub fn insert(&mut self, name: String, map: LoadedMap) {
        if !self.rotation.contains(&name) {
            self.rotation.push(name.clone());
        }

        if self.current_map.is_none() && !self.rotation.is_empty() {
            self.current_map = Some(self.rotation[0].clone());
        }

        self.maps.insert(name, map);
    }

    pub fn remove_map(&mut self, name: &str) {
        self.maps.remove(name);
        if let Some(pos) = self.rotation.iter().position(|x| x == name) {
            self.rotation.remove(pos);
            if pos < self.rotation_idx {
                self.rotation_idx = self.rotation_idx.saturating_sub(1);
            }
        }
        if self.rotation_idx >= self.rotation.len() {
            self.rotation_idx = 0;
        }
        if self.current_map.as_deref() == Some(name) {
            self.current_map = self.rotation.first().cloned();
        }
    }

    fn make_active_state(&self) -> (Option<String>, DeathrunState) {
        let mut ds = DeathrunState::default();
        if let Some(name) = &self.current_map {
            if let Some(loaded) = self.maps.get(name) {
                ds.map = Some(loaded.state.clone());
                ds.traps = loaded.traps.clone();
                return (Some(name.clone()), ds);
            }
        }
        (None, ds)
    }
}

/// Who is moving, for spectating players who stand still too long.
#[derive(Default)]
pub struct Afk {
    /// Last recorded position per connected player.
    last_pos: HashMap<u64, [f32; 3]>,
    /// Time of the last significant movement per player.
    last_move: HashMap<u64, Instant>,
    /// Players who have already received the AFK warning sound.
    warned: HashSet<u64>,
    /// Players spectated for being AFK who haven't moved since. They are not
    /// picked as terrorist: they would likely waste the whole round.
    spectated: HashSet<u64>,
}

impl Afk {
    pub fn update_pos(&mut self, id: u64, pos: [f32; 3]) {
        let moved = self.last_pos.get(&id).map_or(true, |last| {
            let d = [pos[0] - last[0], pos[1] - last[1], pos[2] - last[2]];
            d[0] * d[0] + d[1] * d[1] + d[2] * d[2] > 0.25 // 0.5 unit threshold
        });
        self.last_pos.insert(id, pos);
        if moved {
            self.last_move.insert(id, Instant::now());
            self.warned.remove(&id);
            self.spectated.remove(&id);
        }
    }

    pub fn remove(&mut self, id: u64) {
        self.last_pos.remove(&id);
        self.last_move.remove(&id);
        self.warned.remove(&id);
        self.spectated.remove(&id);
    }
}

pub struct DeathrunPlay {
    maps: Arc<RwLock<PlayMaps>>,
    config: PlayConfig,
    highlights: crate::trap::highlight::Highlights,
    notes: crate::ServerNotes,
}

impl DeathrunPlay {
    pub fn new(config: PlayConfig) -> Self {
        Self {
            maps: Arc::new(RwLock::new(PlayMaps::default())),
            config,
            highlights: Default::default(),
            notes: Default::default(),
        }
    }

    /// Lines shown in every welcome wall and at the end of `#help` (see `ServerNotes`).
    pub fn with_notes(mut self, notes: Vec<String>) -> Self {
        self.notes = crate::ServerNotes::from_strings(notes);
        self
    }

    pub fn maps_handle(&self) -> Arc<RwLock<PlayMaps>> {
        Arc::clone(&self.maps)
    }

    pub fn add_map(&self, name: String, data: &[u8]) -> Result<(), String> {
        let map = LoadedMap::parse(&name, data)?;
        self.maps.write().insert(name, map);
        Ok(())
    }

    pub fn remove_map(&self, name: &str) {
        self.maps.write().remove_map(name);
    }
}

fn start_round_sequence(server: Arc<Server>, maps: Arc<RwLock<PlayMaps>>, welcome_id: Option<u64>) {
    let mut lock = maps.write();
    let (map, my_epoch, initial_participants) = {
        if lock.rotation.is_empty() {
            return;
        }

        // Abort previous loading task if any, to prevent geometry interleaving
        if let Some(task) = lock.round_task.take() {
            task.abort();
        }

        lock.round_epoch += 1;
        let epoch = lock.round_epoch;
        lock.round_state = RoundState::MapLoading;
        lock.spawned_this_round.clear();
        lock.finished_players.clear();
        lock.evil_id = None;

        if lock.rotation_idx >= lock.rotation.len() {
            lock.rotation.shuffle(&mut rand::thread_rng());
            lock.rotation_idx = 0;
        }
        let next_map = lock.rotation[lock.rotation_idx].clone();
        lock.rotation_idx += 1;
        lock.current_map = Some(next_map);

        let (_, new_state) = lock.make_active_state();
        let Some(map) = new_state.map.clone() else {
            lock.round_state = RoundState::NotStarted;
            return;
        };
        *get_server_state(&server).write() = new_state;
        (map, epoch, server.client_ids())
    };

    // Spawned and stored under the same lock, so a round started meanwhile
    // can't have its handle overwritten by this older one.
    let maps_clone = Arc::clone(&maps);
    lock.round_task = Some(tokio::spawn(async move {
        let ids = initial_participants;
        if ids.is_empty() {
            let mut lock = maps_clone.write();
            if lock.round_epoch == my_epoch {
                lock.round_state = RoundState::NotStarted;
                server.set_master_mode(MasterMode::Veto);
            }
            return;
        }

        let evil_id = if ids.len() > 1 {
            // Skip players who were spectated for being AFK, unless that's everyone.
            let afk = maps_clone.read().afk.lock().spectated.clone();
            let active: Vec<u64> = ids.iter().copied().filter(|id| !afk.contains(id)).collect();
            let pool = if active.is_empty() { &ids } else { &active };
            Some(*pool.choose(&mut rand::thread_rng()).unwrap())
        } else {
            None
        };

        {
            let mut lock = maps_clone.write();
            if lock.round_epoch != my_epoch {
                return;
            }
            lock.evil_id = evil_id;
        }

        server.set_master_mode(MasterMode::Locked);

        for &id in &ids {
            let team = if Some(id) == evil_id { "evil" } else { "good" };
            server.set_team(id, team.lossy_cubestring(), SetTeamReason::Forced);
            server.set_spectator(id, true);
        }

        let has_team_1 = map
            .entities
            .iter()
            .any(|e| e.ent_type == EntityType::PlayerStart && e.attrs[1] == 1);
        let has_team_2 = map
            .entities
            .iter()
            .any(|e| e.ent_type == EntityType::PlayerStart && e.attrs[1] == 2);

        let author_cn = crate::SERVER_BOT_CN;
        crate::recreate::visual_rebuild_map(
            &server,
            &ids,
            author_cn,
            &map,
            20,
            true,
            move |id, _i, ent| {
                let is_evil = Some(id) == evil_id;

                let ent_type = ent.ent_type;
                let mut attrs = ent.attrs;

                if ent_type == EntityType::PlayerStart {
                    let team_attr = attrs[1];
                    if team_attr == 2 {
                        if is_evil {
                            attrs[1] = 0;
                        } else {
                            return None;
                        }
                    } else if team_attr == 1 {
                        if !is_evil {
                            attrs[1] = 0;
                        } else {
                            return None;
                        }
                    } else if team_attr == 0 {
                        if is_evil && has_team_2 {
                            return None;
                        }
                        if !is_evil && has_team_1 {
                            return None;
                        }
                    }
                }

                Some(ogz_rs::data::Entity {
                    position: ent.position,
                    attrs,
                    ent_type,
                    extra_data: vec![],
                })
            },
        )
        .await;

        let current_evil;
        {
            let mut lock = maps_clone.write();
            if lock.round_epoch != my_epoch {
                return;
            }
            lock.round_state = RoundState::Playing;
            lock.round_start_time = Some(Instant::now());
            current_evil = lock.evil_id;
        }

        for &id in &ids {
            if server.players.contains(id) == false {
                continue;
            }

            server.set_spectator(id, false); // Moves them into Dead state safely
            if let Some(cn) = server.get_cn(id) {
                server.send(id, &channel1::ForceDeath { client_num: cn }); // Resets screen
            }

            server.force_spawn(id, |state| {
                state.ammo.pistol = 200;
                if Some(id) == current_evil {
                    state.ammo.grenade_launcher = 0;
                }
            });
            maps_clone.write().spawned_this_round.insert(id);
        }

        let evil_name = current_evil
            .and_then(|id| server.players.with(id, |p| p.name.strip_colors().to_utf8()));

        // Send the generic connection welcome rules to the new player first
        if let Some(w_id) = welcome_id {
            if server.players.contains(w_id) {
                let (is_solo, map_name) = {
                    let lock = maps_clone.read();
                    (lock.evil_id.is_none(), lock.current_map.clone())
                };
                ui::send_welcome_play(&server, w_id, map_name.as_deref(), is_solo);
            }
        }

        if end_if_decided_at_start(&server, &maps_clone, my_epoch, current_evil) {
            let mut lock = maps_clone.write();
            if lock.round_epoch == my_epoch {
                lock.round_task = None;
            }
            return;
        }

        // Then broadcast the round state/gamemode to everyone
        ui::broadcast_round_start(&server, evil_name.as_deref());

        let mut lock = maps_clone.write();
        if lock.round_epoch == my_epoch {
            lock.round_task = None;
        }
    }));
}

fn check_win_conditions(
    server: &Arc<Server>,
    maps: &Arc<RwLock<PlayMaps>>,
    disconnecting_id: Option<u64>,
) {
    let mut lock = maps.write();
    if lock.round_state != RoundState::Playing {
        return;
    }

    let mut ids = server.client_ids();
    if let Some(d_id) = disconnecting_id {
        ids.retain(|&id| id != d_id);
    }

    if ids.is_empty() {
        lock.round_state = RoundState::NotStarted;
        server.set_master_mode(MasterMode::Veto);
        return;
    }

    let evil_id = lock.evil_id;

    if let Some(e_id) = evil_id {
        if !ids.contains(&e_id) {
            declare_win(
                server,
                maps,
                &mut lock,
                "\x0C1Good\x0C7 (Evil disconnected)",
                &[],
            );
            return;
        }

        let evil_out = server.players.with(e_id, |p| {
            matches!(p.status, Status::Dead | Status::Spectator)
        });
        if evil_out == Some(true) {
            let mut top = Vec::new();
            let mut sorted = lock.finished_players.clone();
            sorted.sort_by(|a, b| a.1.cmp(&b.1));
            for (f_id, dur) in sorted.into_iter().take(3) {
                let name = crate::ui::player_name(server, f_id);
                top.push((name, dur.as_secs_f32()));
            }
            declare_win(server, maps, &mut lock, "\x0C1Good\x0C7 (Evil died)", &top);
            return;
        }
    }

    if !lock.spawned_this_round.is_empty() {
        let mut good_still_playing = false;
        for &id in &lock.spawned_this_round {
            if Some(id) == evil_id {
                continue;
            }
            let is_finished = lock.finished_players.iter().any(|(f_id, _)| *f_id == id);
            if !is_finished {
                // If they are still connected and alive, the round is still playing
                if ids.contains(&id) {
                    if server.players.with(id, |p| p.status == Status::Alive) == Some(true) {
                        good_still_playing = true;
                        break;
                    }
                }
            }
        }

        if !good_still_playing {
            // Check if anyone finished first.
            if !lock.finished_players.is_empty() {
                let mut top = Vec::new();
                let mut sorted = lock.finished_players.clone();
                sorted.sort_by(|a, b| a.1.cmp(&b.1));
                for (f_id, dur) in sorted.into_iter().take(3) {
                    let name = crate::ui::player_name(server, f_id);
                    top.push((name, dur.as_secs_f32()));
                }
                declare_win(
                    server,
                    maps,
                    &mut lock,
                    "\x0C1Good\x0C7 (Race Finished)",
                    &top,
                );
            } else {
                declare_win(
                    server,
                    maps,
                    &mut lock,
                    "\x0C3Evil\x0C7 (All Good players died/disconnected)",
                    &[],
                );
            }
        }
    }
}

/// Ends a round that was decided while its map was loading, e.g. because the
/// only good player left: `check_win_conditions` skips rounds that aren't
/// playing yet, so nothing else would notice. Nobody counts as alive until
/// their client confirms the spawn, so this goes by who is still connected.
/// Returns whether the round ended.
fn end_if_decided_at_start(
    server: &Arc<Server>,
    maps: &Arc<RwLock<PlayMaps>>,
    epoch: u64,
    evil: Option<u64>,
) -> bool {
    let mut lock = maps.write();
    if lock.round_epoch != epoch || lock.round_state != RoundState::Playing {
        return false;
    }
    let here = |id: u64| server.players.contains(id);
    let anyone_here = lock.spawned_this_round.iter().any(|&id| here(id));
    if !anyone_here {
        lock.round_state = RoundState::NotStarted;
        server.set_master_mode(MasterMode::Veto);
        return true;
    }
    let Some(evil) = evil else {
        return false; // practice round, nothing to win
    };
    if !here(evil) {
        declare_win(
            server,
            maps,
            &mut lock,
            "\x0C1Good\x0C7 (Evil disconnected)",
            &[],
        );
        return true;
    }
    let good_here = lock
        .spawned_this_round
        .iter()
        .any(|&id| id != evil && here(id));
    if !good_here {
        declare_win(
            server,
            maps,
            &mut lock,
            "\x0C3Evil\x0C7 (All Good players died/disconnected)",
            &[],
        );
        return true;
    }
    false
}

fn declare_win(
    server: &Arc<Server>,
    maps: &Arc<RwLock<PlayMaps>>,
    lock: &mut parking_lot::RwLockWriteGuard<PlayMaps>,
    winner_text: &str,
    top_times: &[(String, f32)],
) {
    lock.round_state = RoundState::Ended;
    let epoch = lock.round_epoch;
    ui::broadcast_round_over(server, winner_text, top_times);

    for id in server.client_ids() {
        if let Some(cn) = server.get_cn(id) {
            server.send(
                id,
                &Client {
                    client_num: cn,
                    packet: channel1::Sound { sound_id: 63 },
                },
            );
        }
    }

    let s = Arc::clone(server);
    let m = Arc::clone(maps);
    tokio::spawn(async move {
        sleep(Duration::from_secs(3)).await;
        // Unless someone started another round meanwhile.
        if m.read().round_epoch == epoch {
            start_round_sequence(s, m, None);
        }
    });
}

fn send_map_to_client(
    server: Arc<Server>,
    target_id: u64,
    maps: &Arc<RwLock<PlayMaps>>,
    welcome: bool,
) -> bool {
    let (map, evil_id) = {
        let lock = maps.read();
        let name = match &lock.current_map {
            Some(n) => n,
            None => return false,
        };
        let loaded = match lock.maps.get(name) {
            Some(m) => m,
            None => return false,
        };
        (loaded.state.clone(), lock.evil_id)
    };

    let maps_arc = Arc::clone(maps);
    tokio::spawn(async move {
        let author_cn = crate::SERVER_BOT_CN;
        crate::recreate::visual_rebuild_map(
            &server,
            &[target_id],
            author_cn,
            &map,
            20,
            true,
            |id, _i, ent| {
                let is_evil = Some(id) == evil_id;

                let ent_type = ent.ent_type;
                let mut attrs = ent.attrs;

                if ent_type == EntityType::PlayerStart {
                    let team_attr = attrs[1];
                    if team_attr == 0 {
                        return None;
                    } else if team_attr == 2 {
                        if is_evil {
                            attrs[1] = 0;
                        } else {
                            return None;
                        }
                    } else if team_attr == 1 {
                        if !is_evil {
                            attrs[1] = 0;
                        } else {
                            return None;
                        }
                    }
                }

                Some(ogz_rs::data::Entity {
                    position: ent.position,
                    attrs,
                    ent_type,
                    extra_data: vec![],
                })
            },
        )
        .await;

        if welcome {
            let (is_solo, map_name) = {
                let lock = maps_arc.read();
                (lock.evil_id.is_none(), lock.current_map.clone())
            };
            ui::send_welcome_play(&server, target_id, map_name.as_deref(), is_solo);
        }
    });

    true
}

fn check_afk(server: &Arc<Server>, maps: &Arc<RwLock<PlayMaps>>, timeout: Duration) {
    let now = Instant::now();
    let warn_timeout = timeout.saturating_sub(Duration::from_secs(5));

    let mut to_spectate = Vec::new();
    let mut to_warn = Vec::new();
    {
        let lock = maps.read();
        if lock.round_state != RoundState::Playing {
            return;
        }
        let ids = server.client_ids();
        if ids.len() <= 1 {
            return;
        }
        let mut afk = lock.afk.lock();
        for id in ids {
            if !server
                .players
                .with(id, |p| p.status == Status::Alive)
                .unwrap_or(false)
            {
                continue;
            }
            if let Some(&t) = afk.last_move.get(&id) {
                let elapsed = now.duration_since(t);
                if elapsed >= timeout {
                    to_spectate.push(id);
                    afk.warned.remove(&id);
                } else if elapsed >= warn_timeout && !afk.warned.contains(&id) {
                    to_warn.push(id);
                    afk.warned.insert(id);
                }
            }
        }
    }

    for id in to_warn {
        if let Some(cn) = server.get_cn(id) {
            server.send(
                id,
                &Client {
                    client_num: cn,
                    packet: channel1::Sound { sound_id: 61 },
                },
            );
        }
    }

    for id in &to_spectate {
        let name = ui::player_name(server, *id);
        ui::broadcast(
            server,
            ui::warn_msg(&format!("{name} was spectated for being AFK.")),
        );
        server.set_spectator(*id, true);
        maps.read().afk.lock().spectated.insert(*id);
    }

    if !to_spectate.is_empty() {
        check_win_conditions(server, maps, None);
    }
}

impl Plugin for DeathrunPlay {
    async fn init(&self, server: &mut Server) {
        self.notes.install(server);
        {
            let game = server.game.get_mut();
            game.map_name = "".lossy_cubestring();
            game.mode = GameMode::TeamPlay;
            game.master_mode = MasterMode::Veto;
        }
        server.extra.insert(crate::recreate::KnownVars::default());
        // Rounds set the master mode, so it must not snap back to open.
        server.config.write().reopen_when_masterless = false;
        crate::add_server_bot(server);

        let (_, state) = self.maps.read().make_active_state();
        server.extra.insert(RwLock::new(state));
        server.extra.insert(Arc::clone(&self.maps));
        server
            .extra
            .insert(crate::trap::placement::SeeThrough::default());
    }

    async fn start(&self, server: &Arc<Server>) {
        let maps = Arc::clone(server.extra.get::<Arc<RwLock<PlayMaps>>>().unwrap());
        let server = Arc::clone(server);
        let timeout = Duration::from_secs(self.config.afk_timeout_secs);
        tokio::spawn(async move {
            loop {
                sleep(Duration::from_secs(1)).await;
                check_afk(&server, &maps, timeout);
            }
        });
    }

    async fn on_client_welcomed(&self, ctx: &mut Context) {
        let id = ctx.id();
        let server = ctx.server();
        let maps = Arc::clone(server.extra.get::<Arc<RwLock<PlayMaps>>>().unwrap());

        let (is_first, is_solo_round, is_waiting) = {
            let lock = maps.read();
            let is_first = server.client_count() == 1;
            let is_solo_round = (lock.round_state == RoundState::Playing
                || lock.round_state == RoundState::MapLoading)
                && lock.evil_id.is_none();
            let is_waiting =
                lock.round_state == RoundState::NotStarted && server.client_count() > 1;
            (is_first, is_solo_round, is_waiting)
        };

        if is_first || is_solo_round || is_waiting {
            // Restart or start if needed to ensure all players are included and roles assigned correctly.
            start_round_sequence(Arc::clone(server), Arc::clone(&maps), Some(id));
        } else {
            let is_playing = maps.read().round_state != RoundState::NotStarted;
            if is_playing {
                send_map_to_client(Arc::clone(server), id, &maps, true);
                server.set_spectator(id, true);
                server.set_team(id, "good".lossy_cubestring(), SetTeamReason::Forced);
            } else {
                let (is_solo, map_name) = {
                    let lock = maps.read();
                    (lock.evil_id.is_none(), lock.current_map.clone())
                };
                ui::send_welcome_play(server, id, map_name.as_deref(), is_solo);
            }
        }
    }

    async fn pos(&self, ctx: &mut Context, packet: &channel0::Position) -> Flow {
        let id = ctx.id();
        let server = ctx.server();
        let maps = &self.maps;
        let pos = [packet.pos.x, packet.pos.y, packet.pos.z];
        maps.read().afk.lock().update_pos(id, pos);

        let (is_playing, is_evil, finished, round_start) = {
            let lock = maps.read();
            (
                lock.round_state == RoundState::Playing,
                lock.evil_id == Some(id),
                lock.finished_players.iter().any(|(f_id, _)| *f_id == id),
                lock.round_start_time,
            )
        };

        // The terrorist sees which trap they aim at.
        if is_evil && is_playing {
            let aim = Aim::new(packet.pos, packet.dir);
            let (target, seethrough, outline) = {
                let state = get_server_state(server).read();
                let target = state.trap_at(aim.hit_point(state.map.as_ref()));
                let trap = target.and_then(|t| state.trap(t));
                let outline = trap
                    .and_then(|t| t.geom.as_ref())
                    .map(|g| frame::outline(g.clip_box(), Some(&aim), rainbow_color()));
                (
                    target,
                    trap.map(crate::trap::placement::button_key),
                    outline,
                )
            };
            crate::trap::placement::update_seethrough_button(server, id, seethrough);
            self.highlights.update(server, id, target, outline);
        } else {
            crate::trap::placement::update_seethrough_button(server, id, None);
            self.highlights.clear(server, id);
        }

        if is_playing && !is_evil && !finished {
            check_finish(server, maps, id, pos, round_start);
        }
        Flow::Continue
    }

    async fn disconnected(&self, ctx: &mut Context) {
        let id = ctx.id();
        let server = ctx.server();
        let maps = Arc::clone(server.extra.get::<Arc<RwLock<PlayMaps>>>().unwrap());
        maps.read().afk.lock().remove(id);
        if let Some(known) = server.extra.get::<crate::recreate::KnownVars>() {
            known.forget(id);
        }
        self.highlights.forget(id);
        crate::trap::placement::forget_seethrough(server, id);

        let mut state_w = crate::map_state::get_server_state(server).write();
        {
            let up = state_w.virt.clear_player(id);
            up.send(server, id, crate::SERVER_BOT_CN);
        }
        drop(state_w);

        check_win_conditions(server, &maps, Some(id));
    }

    async fn on_player_died(&self, ctx: &mut Context, victim_id: u64, _actor_id: u64) {
        let server = ctx.server();
        let maps = Arc::clone(server.extra.get::<Arc<RwLock<PlayMaps>>>().unwrap());

        // Spectate good players when they die
        let is_evil = maps.read().evil_id == Some(victim_id);
        if !is_evil {
            server.set_spectator(victim_id, true);
        }

        check_win_conditions(server, &maps, None);
    }

    async fn on_spawn(&self, ctx: &mut Context, state: &mut PlayerState) -> Flow {
        let id = ctx.id();
        let maps = self.maps.read();
        let is_evil = maps.evil_id == Some(id);

        state.ammo.pistol = 200;
        if is_evil {
            state.ammo.grenade_launcher = 0;
        }
        Flow::Continue
    }

    async fn on_damage(
        &self,
        ctx: &mut Context,
        target_id: u64,
        actor_id: u64,
        _damage: &mut i32,
    ) -> Flow {
        let server = ctx.server();
        let target_team = server.players.with(target_id, |p| p.team.clone());
        let actor_team = server.players.with(actor_id, |p| p.team.clone());

        if target_team.is_some() && target_team == actor_team {
            return Flow::Cancel;
        }

        // The terrorist can only kill with traps, never by shooting.
        let actor_is_evil = self.maps.read().evil_id == Some(actor_id);
        if actor_is_evil && target_id != actor_id {
            return Flow::Cancel;
        }
        Flow::Continue
    }

    async fn try_spawn(&self, ctx: &mut Context, _packet: &channel1::TrySpawn) -> Flow {
        let id = ctx.id();
        let server = ctx.server();
        let maps = server.extra.get::<Arc<RwLock<PlayMaps>>>().unwrap();

        let mut cancel = false;
        {
            let mut lock = maps.write();
            match lock.round_state {
                RoundState::Playing => {
                    let solo = lock.evil_id.is_none();
                    if !solo && lock.spawned_this_round.contains(&id) {
                        cancel = true; // One life only in multiplayer!
                    } else {
                        lock.spawned_this_round.insert(id);
                    }
                }
                RoundState::MapLoading | RoundState::Ended => {
                    cancel = true; // Wait for round to actually start
                }
                RoundState::NotStarted => {}
            }
        }
        if cancel {
            return Flow::Cancel;
        }
        Flow::Continue
    }

    async fn shoot(&self, ctx: &mut Context, packet: &channel1::Shoot) -> Flow {
        let id = ctx.id();
        let server = ctx.server();

        let is_evil = {
            let maps = server.extra.get::<Arc<RwLock<PlayMaps>>>().unwrap();
            maps.read().evil_id == Some(id)
        };
        if !is_evil {
            return Flow::Continue;
        }

        let hit_pt = [packet.to.x, packet.to.y, packet.to.z];
        let trap_idx = {
            let state = crate::map_state::get_server_state(server).read();
            if state.mode.editing().is_some() {
                return Flow::Continue;
            }
            state.trap_at(hit_pt)
        };

        if let Some(idx) = trap_idx {
            crate::trap::trigger_trap(server, idx, id);
        }
        Flow::Continue
    }

    async fn switch_team(&self, _ctx: &mut Context, _packet: &channel1::SwitchTeam) -> Flow {
        Flow::Cancel
    }

    async fn map_vote(&self, _ctx: &mut Context, _packet: &channel1::MapVote) -> Flow {
        Flow::Cancel // Map rotation is strictly managed by PlayMaps
    }

    async fn spectator(&self, ctx: &mut Context, packet: &channel1::SpectatorClient) -> Flow {
        let server = ctx.server();
        let maps = server.extra.get::<Arc<RwLock<PlayMaps>>>().unwrap();
        let round_state = maps.read().round_state;

        // Prevent self-unspectating during the game (even for masters to prevent cheating the 1-life limit).
        if round_state == RoundState::Playing || round_state == RoundState::MapLoading {
            if !packet.val {
                return Flow::Cancel;
            }
        }
        Flow::Continue
    }

    async fn set_master(&self, ctx: &mut Context, packet: &channel1::SetMaster) -> Flow {
        // Only admin (with the password) here: rounds set the master mode, and
        // players handing master around would fight them. Giving up admin and
        // the admin managing others go through to the base handler.
        let server = ctx.server();
        let id = ctx.id();
        let is_admin = server.get_privilege(id) >= Privilege::Admin;
        let claims_admin = server.get_id(packet.client_num) == Some(id)
            && packet.val
            && server.check_admin_pass(id, packet.hash.as_bytes());
        if is_admin || claims_admin {
            return Flow::Continue;
        }
        ui::send_warn(server, id, "Master is disabled on this server.");
        Flow::Cancel
    }

    async fn get_map(&self, ctx: &mut Context, _packet: &channel1::GetMap) -> Flow {
        let id = ctx.id();
        let server = ctx.server();
        let maps = Arc::clone(server.extra.get::<Arc<RwLock<PlayMaps>>>().unwrap());
        if send_map_to_client(Arc::clone(server), id, &maps, false) {
            info!("Sending map rebuild to id {id} on N_GETMAP");
            return Flow::Cancel;
        }
        Flow::Continue
    }
}

/// How close to a flag a good player has to get to finish the map.
const FLAG_REACH: f32 = 24.0;

/// Finishes the map for `id` if they are at a flag.
fn check_finish(
    server: &Arc<Server>,
    maps: &Arc<RwLock<PlayMaps>>,
    id: u64,
    pos: [f32; 3],
    round_start: Option<Instant>,
) {
    let flag = {
        let state = get_server_state(server).read();
        let near = |e: &&ogz_rs::data::Entity| {
            let d: f32 = (0..3).map(|i| (pos[i] - e.position[i]).powi(2)).sum();
            e.ent_type == EntityType::Flag && d < FLAG_REACH * FLAG_REACH
        };
        state
            .map
            .as_ref()
            .and_then(|m| m.entities.iter().find(near).map(|e| e.position))
    };
    let Some(flag) = flag else {
        return;
    };
    let time = round_start.map_or(Duration::ZERO, |t| t.elapsed());
    let first = {
        let mut lock = maps.write();
        if lock.finished_players.iter().any(|(f_id, _)| *f_id == id) {
            return;
        }
        lock.finished_players.push((id, time));
        lock.finished_players.len() == 1
    };

    let text = format!(
        "\x0C1{} \x0C7finished the map in \x0C5{:.2}s\x0C7!",
        ui::player_name(server, id),
        time.as_secs_f32()
    );
    ui::broadcast(server, ui::body(&text));
    for client_id in server.client_ids() {
        if let Some(client_cn) = server.get_cn(client_id) {
            server.send(
                client_id,
                &Client {
                    client_num: client_cn,
                    packet: channel1::Sound { sound_id: 60 },
                },
            );
        }
    }
    if first {
        fireworks(server, flag);
    }
    check_win_conditions(server, maps, None);
}

/// Particle effects at the flag for the first one to finish.
fn fireworks(server: &Server, pos: [f32; 3]) {
    const EFFECTS: [[i16; 5]; 2] = [[0, 1000, 0, -1, 0], [12, 1000, 1000, 0, 0]];
    let placed: Vec<(usize, [i16; 5])> = {
        let mut state = get_server_state(server).write();
        let Some(map) = state.map.as_mut() else {
            return;
        };
        EFFECTS
            .iter()
            .map(|&attrs| {
                let ent = ogz_rs::data::Entity {
                    position: pos,
                    attrs,
                    ent_type: EntityType::Particles,
                    extra_data: Vec::new(),
                };
                let slot = map
                    .entities
                    .iter()
                    .position(|e| e.ent_type == EntityType::Empty);
                let i = match slot {
                    Some(i) => {
                        map.entities[i] = ent;
                        i
                    }
                    None => {
                        map.entities.push(ent);
                        map.entities.len() - 1
                    }
                };
                (i, attrs)
            })
            .collect()
    };
    for (i, attrs) in placed {
        server.broadcast(&Client {
            client_num: crate::SERVER_BOT_CN,
            packet: channel1::EditEnt {
                ent_id: i as i32,
                pos: sauer_proto::data::Vec3 {
                    x: pos[0],
                    y: pos[1],
                    z: pos[2],
                },
                type_: EntityType::Particles,
                attrs: attrs.map(i32::from),
            },
        });
    }
}

/// A color cycling through the rainbow every 2 seconds, as 0xRRGGBB.
fn rainbow_color() -> i32 {
    let time_ms = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap_or_default()
        .as_millis();
    let h = (time_ms % 2000) as f32 / 2000.0 * 6.0;
    let x = 1.0 - ((h % 2.0) - 1.0).abs();
    let (r, g, b) = match h as i32 {
        0 => (1.0, x, 0.0),
        1 => (x, 1.0, 0.0),
        2 => (0.0, 1.0, x),
        3 => (0.0, x, 1.0),
        4 => (x, 0.0, 1.0),
        _ => (1.0, 0.0, x),
    };
    ((r * 255.0) as i32) << 16 | ((g * 255.0) as i32) << 8 | ((b * 255.0) as i32)
}
