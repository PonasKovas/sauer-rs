use crate::map_state::{DeathrunState, Trap, get_server_state};
use sauer_map::Map;
use crate::trap::StoredTrap;
use crate::ui;
use cube_string::IntoCubeStringLossy;

use rand::seq::SliceRandom;
use sauer_proto::data::PlayerState;
use sauer_proto::{
    Client, channel0, channel1,
    data::{ClientState, EntityType, GameMode, MasterMode, SetTeamReason},
};
use sauer_server::{
    Server,
    plugin::{Context, Plugin},
};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::collections::HashSet;
use std::sync::{Arc, RwLock};
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
    pub spawned_this_round: HashSet<u64>,
    pub round_start_time: Option<Instant>,
    pub finished_players: Vec<(u64, Duration)>,
    /// Last recorded position per connected player.
    afk_last_pos: HashMap<u64, [f32; 3]>,
    /// Time of the last significant movement per player.
    afk_last_move: HashMap<u64, Instant>,
    /// Players who have already received the AFK warning sound.
    afk_warned: HashSet<u64>,
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
            spawned_this_round: HashSet::new(),
            round_start_time: None,
            finished_players: Vec::new(),
            afk_last_pos: HashMap::new(),
            afk_last_move: HashMap::new(),
            afk_warned: HashSet::new(),
        }
    }
}

impl PlayMaps {
    pub fn add_map(&mut self, name: String, data: &[u8]) -> Result<(), String> {
        let mut state = Map::from_bytes(data).map_err(|e| e.to_string())?;
        let mut traps = Vec::new();
        // Universal cleanup of magic active trap particles
        for ent in &mut state.entities {
            if ent.ent_type == ogz_rs::data::EntityType::Particles
                && ent.attrs[3] == 1337
                && ent.attrs[4] == 7331
            {
                ent.ent_type = ogz_rs::data::EntityType::Empty;
                ent.attrs = [0; 5];
            }
        }

        if let Ok((entries, _stats)) = state.db_scan::<StoredTrap>() {
            let mut stored = Vec::new();
            for entry in entries {
                let st = entry.data;
                let bpos = st.button_pos;
                let valid = entry.instances.iter().any(|inst| {
                    inst.pos[0] >= bpos[0] && inst.pos[0] <= bpos[0] + 8 &&
                    inst.pos[1] >= bpos[1] && inst.pos[1] <= bpos[1] + 8 &&
                    inst.pos[2] >= bpos[2] && inst.pos[2] <= bpos[2] + 8
                });
                if valid {
                    stored.push(st);
                } else {
                    tracing::warn!("Rejected copied trap payload outside expected bounds at {:?}", entry.instances.first().map(|i| i.pos));
                }
            }

            for st in &stored {
                st.apply_reset_to_world(&mut state);
            }
            traps = stored.into_iter().filter_map(|st| st.into_trap()).collect();
            if !traps.is_empty() {
                info!("Loaded {} traps from map '{name}'", traps.len());
            }
        }

        crate::trap::placement::rebuild_all_trap_buttons(&mut state, &traps);
        for trap in &traps {
            let b_max = [trap.button_pos[0] + trap.button_size, trap.button_pos[1] + trap.button_size, trap.button_pos[2] + trap.button_size];
            state.protect_region(trap.button_pos, b_max);

            let packets = crate::trap::stored::generate_trap_vslot_packets(
                &crate::trap::StoredTrap::from_trap(trap),
            );
            for p in &packets {
                state.apply_vslots(&p.sel, p.all_faces, p.delta != 0, &p.extra_data);
            }
        }

        if !self.rotation.contains(&name) {
            self.rotation.push(name.clone());
        }

        if self.current_map.is_none() && !self.rotation.is_empty() {
            self.current_map = Some(self.rotation[0].clone());
        }

        self.maps.insert(name, LoadedMap { state, traps });
        Ok(())
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

    pub fn update_afk_pos(&mut self, id: u64, pos: [f32; 3]) {
        let moved = self.afk_last_pos.get(&id).map_or(true, |last| {
            let d = [pos[0] - last[0], pos[1] - last[1], pos[2] - last[2]];
            d[0] * d[0] + d[1] * d[1] + d[2] * d[2] > 0.25 // 0.5 unit threshold
        });
        self.afk_last_pos.insert(id, pos);
        if moved {
            self.afk_last_move.insert(id, Instant::now());
            self.afk_warned.remove(&id);
        }
    }

    pub fn remove_player_afk(&mut self, id: u64) {
        self.afk_last_pos.remove(&id);
        self.afk_last_move.remove(&id);
        self.afk_warned.remove(&id);
    }
}

pub struct DeathrunPlay {
    maps: Arc<RwLock<PlayMaps>>,
    config: PlayConfig,
}

impl DeathrunPlay {
    pub fn new(config: PlayConfig) -> Self {
        Self {
            maps: Arc::new(RwLock::new(PlayMaps::default())),
            config,
        }
    }

    pub fn maps_handle(&self) -> Arc<RwLock<PlayMaps>> {
        Arc::clone(&self.maps)
    }

    pub fn add_map(&self, name: String, data: &[u8]) -> Result<(), String> {
        self.maps.write().unwrap().add_map(name, data)
    }

    pub fn remove_map(&self, name: &str) {
        self.maps.write().unwrap().remove_map(name);
    }
}

fn start_round_sequence(server: Arc<Server>, maps: Arc<RwLock<PlayMaps>>, welcome_id: Option<u64>) {
    let (map, my_epoch) = {
        let mut lock = maps.write().unwrap();
        if lock.rotation.is_empty() {
            return;
        }
        lock.round_epoch += 1;
        let epoch = lock.round_epoch;
        lock.round_state = RoundState::MapLoading;
        lock.spawned_this_round.clear();
        lock.finished_players.clear();

        if lock.rotation_idx >= lock.rotation.len() {
            lock.rotation.shuffle(&mut rand::thread_rng());
            lock.rotation_idx = 0;
        }
        let next_map = lock.rotation[lock.rotation_idx].clone();
        lock.rotation_idx += 1;
        lock.current_map = Some(next_map);

        let (_, new_state) = lock.make_active_state();
        *get_server_state(&server).write().unwrap() = new_state;

        let state_lock = get_server_state(&server);
        let state_read = state_lock.read().unwrap();
        let map = state_read.map.as_ref().unwrap();
        (
            map.clone(),
            epoch,
        )
    };

    tokio::spawn(async move {
        let ids = server.client_ids();
        if ids.is_empty() {
            let mut lock = maps.write().unwrap();
            if lock.round_epoch == my_epoch {
                lock.round_state = RoundState::NotStarted;
                server.set_master_mode(MasterMode::Veto);
            }
            return;
        }

        let evil_id = if ids.len() > 1 {
            Some(*ids.choose(&mut rand::thread_rng()).unwrap())
        } else {
            None
        };

        {
            let mut lock = maps.write().unwrap();
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

        let has_team_1 = map.entities
            .iter()
            .any(|e| e.ent_type == EntityType::PlayerStart && e.attrs[1] == 1);
        let has_team_2 = map.entities
            .iter()
            .any(|e| e.ent_type == EntityType::PlayerStart && e.attrs[1] == 2);

        let author_cn = crate::SERVER_BOT_CN as i32;
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

        let current_ids;
        let current_evil;
        {
            let mut lock = maps.write().unwrap();
            if lock.round_epoch != my_epoch {
                return;
            }
            lock.round_state = RoundState::Playing;
            lock.round_start_time = Some(Instant::now());
            current_ids = server.client_ids();
            current_evil = lock.evil_id;
        }

        for &id in &current_ids {
            server.set_spectator(id, false); // Moves them into Dead state safely
            if let Some(cn) = server.get_cn(id) {
                server.send_chan1(
                    id,
                    &channel1::ForceDeath {
                        client_num: cn as i32,
                    },
                ); // Resets screen
            }

            server.force_spawn(id, |state| {
                state.ammo.pistol = 200;
                if Some(id) == current_evil {
                    state.ammo.grenade_launcher = 0;
                }
            });
            maps.write().unwrap().spawned_this_round.insert(id);
        }

        let evil_name = current_evil
            .and_then(|id| server.players.try_get(id))
            .map(|p| p.name.strip_colors().to_utf8());

        // Send the generic connection welcome rules to the new player first
        if let Some(w_id) = welcome_id {
            let (is_solo, map_name) = {
                let lock = maps.read().unwrap();
                (lock.evil_id.is_none(), lock.current_map.clone())
            };
            ui::send_welcome_play(&server, w_id, map_name.as_deref(), is_solo);
        }

        // Then broadcast the round state/gamemode to everyone
        ui::broadcast_round_start(&server, evil_name.as_deref());
    });
}

fn check_win_conditions(
    server: &Arc<Server>,
    maps: &Arc<RwLock<PlayMaps>>,
    disconnecting_id: Option<u64>,
) {
    let mut lock = maps.write().unwrap();
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

        let evil_state = server.players.try_get(e_id).map(|p| p.client_state);
        if evil_state == Some(ClientState::Dead) || evil_state == Some(ClientState::Spectator) {
            let mut top = Vec::new();
            let mut sorted = lock.finished_players.clone();
            sorted.sort_by(|a, b| a.1.cmp(&b.1));
            for (f_id, dur) in sorted.into_iter().take(3) {
                let name = server
                    .players
                    .try_get(f_id)
                    .map(|p| p.name.strip_colors().to_utf8())
                    .unwrap_or_else(|| "Unknown".to_string());
                top.push((name, dur.as_secs_f32()));
            }
            declare_win(server, maps, &mut lock, "\x0C1Good\x0C7 (Evil died)", &top);
            return;
        }
    }

    for &id in &ids {
        if Some(id) == evil_id {
            continue;
        }
        if lock.spawned_this_round.contains(&id) {
            let is_finished = lock.finished_players.iter().any(|(f_id, _)| *f_id == id);
            if let Some(p) = server.players.try_get(id) {
                if p.client_state == ClientState::Alive {
                    if !is_finished {
                        // Unfinished alive player found
                    }
                }
            }
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
                    if let Some(p) = server.players.try_get(id) {
                        if p.client_state == ClientState::Alive {
                            good_still_playing = true;
                            break;
                        }
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
                    let name = server
                        .players
                        .try_get(f_id)
                        .map(|p| p.name.strip_colors().to_utf8())
                        .unwrap_or_else(|| "Unknown".to_string());
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

fn declare_win(
    server: &Arc<Server>,
    maps: &Arc<RwLock<PlayMaps>>,
    lock: &mut std::sync::RwLockWriteGuard<PlayMaps>,
    winner_text: &str,
    top_times: &[(String, f32)],
) {
    lock.round_state = RoundState::Ended;
    ui::broadcast_round_over(server, winner_text, top_times);

    for id in server.client_ids() {
        if let Some(cn) = server.get_cn(id) {
            server.send_chan1(
                id,
                &Client {
                    client_num: cn as i32,
                    packet: channel1::Sound { sound_id: 63 },
                },
            );
        }
    }

    let s = Arc::clone(server);
    let m = Arc::clone(maps);
    tokio::spawn(async move {
        sleep(Duration::from_secs(3)).await;
        start_round_sequence(s, m, None);
    });
}

fn send_map_to_client(
    server: Arc<Server>,
    target_id: u64,
    maps: &Arc<RwLock<PlayMaps>>,
    welcome: bool,
) -> bool {
    let (map, evil_id) = {
        let lock = maps.read().unwrap();
        let name = match &lock.current_map {
            Some(n) => n,
            None => return false,
        };
        let loaded = match lock.maps.get(name) {
            Some(m) => m,
            None => return false,
        };
        (
            loaded.state.clone(),
            lock.evil_id,
        )
    };

    let maps_arc = Arc::clone(maps);
    tokio::spawn(async move {
        let author_cn = crate::SERVER_BOT_CN as i32;
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
                let lock = maps_arc.read().unwrap();
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
        let mut lock = maps.write().unwrap();
        if lock.round_state != RoundState::Playing {
            return;
        }
        let ids = server.client_ids();
        if ids.len() <= 1 {
            return;
        }
        for id in ids {
            if !server
                .players
                .try_get(id)
                .map_or(false, |p| p.client_state == ClientState::Alive)
            {
                continue;
            }
            if let Some(&t) = lock.afk_last_move.get(&id) {
                let elapsed = now.duration_since(t);
                if elapsed >= timeout {
                    to_spectate.push(id);
                    lock.afk_warned.remove(&id);
                } else if elapsed >= warn_timeout && !lock.afk_warned.contains(&id) {
                    to_warn.push(id);
                    lock.afk_warned.insert(id);
                }
            }
        }
    }

    for id in to_warn {
        if let Some(cn) = server.get_cn(id) {
            server.send_chan1(
                id,
                &Client {
                    client_num: cn as i32,
                    packet: channel1::Sound { sound_id: 61 },
                },
            );
        }
    }

    for id in &to_spectate {
        let name = server
            .players
            .try_get(*id)
            .map(|p| p.name.strip_colors().to_utf8())
            .unwrap_or_default();
        ui::broadcast(
            server,
            ui::warn_msg(&format!("{name} was spectated for being AFK.")),
        );
        server.set_spectator(*id, true);
    }

    if !to_spectate.is_empty() {
        check_win_conditions(server, maps, None);
    }
}

impl Plugin for DeathrunPlay {
    async fn init(&self, server: &mut Server) {
        *server.map_name.write().unwrap() = "".lossy_cubestring();
        *server.game_mode.write().unwrap() = GameMode::TeamPlay;
        *server.master_mode.write().unwrap() = MasterMode::Veto;
        server.reserve_cn(crate::SERVER_BOT_CN);

        let (_, state) = self.maps.read().unwrap().make_active_state();
        server.extra.insert(RwLock::new(state));
        server.extra.insert(Arc::clone(&self.maps));
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

        server.send_chan1(
            id,
            &sauer_proto::channel1::InitClient {
                client_num: crate::SERVER_BOT_CN as i32,
                name: "Server".lossy_cubestring(),
                player_model: sauer_proto::data::PlayerModel::MrFixit,
                team: "".lossy_cubestring(),
            },
        );
        server.send_chan1(
            id,
            &sauer_proto::channel1::SpectatorServer {
                client_num: crate::SERVER_BOT_CN as i32,
                is_spectator: true,
            },
        );

        let (is_first, is_solo_round, is_waiting) = {
            let lock = maps.read().unwrap();
            let is_first = server.client_count() == 1;
            let is_solo_round = lock.round_state == RoundState::Playing && lock.evil_id.is_none();
            let is_waiting =
                lock.round_state == RoundState::NotStarted && server.client_count() > 1;
            (is_first, is_solo_round, is_waiting)
        };

        if is_first || is_solo_round || is_waiting {
            // Restart or start if needed to ensure all players are included and roles assigned correctly.
            start_round_sequence(Arc::clone(server), Arc::clone(&maps), Some(id));
        } else {
            let is_playing = maps.read().unwrap().round_state != RoundState::NotStarted;
            if is_playing {
                send_map_to_client(Arc::clone(server), id, &maps, true);
                server.set_spectator(id, true);
                server.set_team(id, "good".lossy_cubestring(), SetTeamReason::Forced);
            } else {
                let (is_solo, map_name) = {
                    let lock = maps.read().unwrap();
                    (lock.evil_id.is_none(), lock.current_map.clone())
                };
                ui::send_welcome_play(server, id, map_name.as_deref(), is_solo);
            }
        }
    }

    async fn pos(&self, ctx: &mut Context, packet: &channel0::Position) {
        let id = ctx.id();
        let server = ctx.server();
        let maps = server.extra.get::<Arc<RwLock<PlayMaps>>>().unwrap();
        maps.write()
            .unwrap()
            .update_afk_pos(id, [packet.pos.x, packet.pos.y, packet.pos.z]);

        let (is_playing, is_good, not_finished, round_start) = {
            let lock = maps.read().unwrap();
            (
                lock.round_state == RoundState::Playing,
                lock.evil_id != Some(id),
                !lock.finished_players.iter().any(|(f_id, _)| *f_id == id),
                lock.round_start_time,
            )
        };

        if is_playing && is_good && not_finished {
            let mut reached_flag = false;
            let mut flag_pos = None;
            if let Some(map_state) = crate::map_state::get_server_state(server)
                .read()
                .unwrap()
                .map
                .as_ref()
            {
                let px = packet.pos.x;
                let py = packet.pos.y;
                let pz = packet.pos.z;

                for ent in &map_state.entities {
                    if ent.ent_type == ogz_rs::data::EntityType::Flag {
                        let fx = ent.position[0];
                        let fy = ent.position[1];
                        let fz = ent.position[2];

                        let dist_sq = (px - fx).powi(2) + (py - fy).powi(2) + (pz - fz).powi(2);
                        if dist_sq < 24.0 * 24.0 {
                            reached_flag = true;
                            flag_pos = Some(ent.position);
                            break;
                        }
                    }
                }
            }

            if reached_flag {
                let time_taken = round_start.map(|t| t.elapsed()).unwrap_or(Duration::ZERO);
                let mut first_finish = false;
                {
                    let mut lock = maps.write().unwrap();
                    if !lock.finished_players.iter().any(|(f_id, _)| *f_id == id) {
                        if lock.finished_players.is_empty() {
                            first_finish = true;
                        }
                        lock.finished_players.push((id, time_taken));
                    } else {
                        reached_flag = false;
                    }
                }

                if reached_flag {
                    let name = server
                        .players
                        .try_get(id)
                        .map(|p| p.name.strip_colors().to_utf8())
                        .unwrap_or_else(|| "Someone".to_string());
                    let secs = time_taken.as_secs_f32();
                    ui::broadcast(
                        server,
                        ui::body(&format!(
                            "\x0C1{} \x0C7finished the map in \x0C5{:.2}s\x0C7!",
                            name, secs
                        )),
                    );

                    for client_id in server.client_ids() {
                        if let Some(client_cn) = server.get_cn(client_id) {
                            server.send_chan1(
                                client_id,
                                &sauer_proto::Client {
                                    client_num: client_cn as i32,
                                    packet: sauer_proto::channel1::Sound { sound_id: 60 },
                                },
                            );
                        }
                    }

                    if first_finish {
                        if let Some(pos) = flag_pos {
                            let mut state =
                                crate::map_state::get_server_state(server).write().unwrap();
                            let mut ent_ids = (None, None);
                            if let Some(map) = state.map.as_mut() {
                                let new_ent1 = ogz_rs::data::Entity {
                                    position: pos,
                                    attrs: [0, 1000, 0, -1, 0],
                                    ent_type: ogz_rs::data::EntityType::Particles,
                                    extra_data: vec![],
                                };
                                let new_ent2 = ogz_rs::data::Entity {
                                    position: pos,
                                    attrs: [12, 1000, 1000, 0, 0],
                                    ent_type: ogz_rs::data::EntityType::Particles,
                                    extra_data: vec![],
                                };

                                let find_empty = |m: &mut sauer_map::Map| {
                                    m.entities
                                        .iter()
                                        .position(|e| e.ent_type == ogz_rs::data::EntityType::Empty)
                                        .unwrap_or_else(|| {
                                            let i = m.entities.len();
                                            m.entities.push(ogz_rs::data::Entity {
                                                position: [0.0; 3],
                                                attrs: [0; 5],
                                                ent_type: ogz_rs::data::EntityType::Empty,
                                                extra_data: vec![],
                                            });
                                            i
                                        })
                                };

                                let id1 = find_empty(map);
                                map.entities[id1] = new_ent1;
                                let id2 = find_empty(map);
                                map.entities[id2] = new_ent2;
                                ent_ids = (Some(id1), Some(id2));
                            }
                            drop(state);

                            if let (Some(id1), Some(id2)) = ent_ids {
                                let actor_cn = crate::SERVER_BOT_CN as i32;
                                for client_id in server.client_ids() {
                                    server.send_chan1(
                                        client_id,
                                        &sauer_proto::Client {
                                            client_num: actor_cn,
                                            packet: sauer_proto::channel1::EditEnt {
                                                ent_id: id1 as i32,
                                                pos: sauer_proto::data::Vec3 {
                                                    x: pos[0],
                                                    y: pos[1],
                                                    z: pos[2],
                                                },
                                                type_: sauer_proto::data::EntityType::Particles,
                                                attrs: [0, 1000, 0, -1, 0],
                                            },
                                        },
                                    );
                                    server.send_chan1(
                                        client_id,
                                        &sauer_proto::Client {
                                            client_num: actor_cn,
                                            packet: sauer_proto::channel1::EditEnt {
                                                ent_id: id2 as i32,
                                                pos: sauer_proto::data::Vec3 {
                                                    x: pos[0],
                                                    y: pos[1],
                                                    z: pos[2],
                                                },
                                                type_: sauer_proto::data::EntityType::Particles,
                                                attrs: [12, 1000, 1000, 0, 0],
                                            },
                                        },
                                    );
                                }
                            }
                        }
                    }
                }

                check_win_conditions(server, maps, None);
            }
        }
    }

    async fn disconnected(&self, ctx: &mut Context) {
        let id = ctx.id();
        let server = ctx.server();
        let maps = Arc::clone(server.extra.get::<Arc<RwLock<PlayMaps>>>().unwrap());
        maps.write().unwrap().remove_player_afk(id);
        check_win_conditions(server, &maps, Some(id));
    }

    async fn on_player_died(&self, ctx: &mut Context, victim_id: u64, _actor_id: u64) {
        let server = ctx.server();
        let maps = Arc::clone(server.extra.get::<Arc<RwLock<PlayMaps>>>().unwrap());

        // Spectate good players when they die
        let is_evil = maps.read().unwrap().evil_id == Some(victim_id);
        if !is_evil {
            server.set_spectator(victim_id, true);
        }

        check_win_conditions(server, &maps, None);
    }

    async fn on_spawn(&self, ctx: &mut Context, state: &mut PlayerState) {
        let id = ctx.id();
        let maps = self.maps.read().unwrap();
        let is_evil = maps.evil_id == Some(id);

        state.ammo.pistol = 200;
        if is_evil {
            state.ammo.grenade_launcher = 0;
        }
    }

    async fn on_damage(&self, ctx: &mut Context, target_id: u64, actor_id: u64, _damage: &mut i32) {
        let server = ctx.server();
        let target_team = server.players.try_get(target_id).map(|p| p.team.clone());
        let actor_team = server.players.try_get(actor_id).map(|p| p.team.clone());

        if target_team.is_some() && target_team == actor_team {
            ctx.cancel();
        }
    }

    async fn try_spawn(&self, ctx: &mut Context, _packet: &channel1::TrySpawn) {
        let id = ctx.id();
        let server = ctx.server();
        let maps = server.extra.get::<Arc<RwLock<PlayMaps>>>().unwrap();

        let mut cancel = false;
        {
            let mut lock = maps.write().unwrap();
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
            ctx.cancel();
        }
    }

    async fn shoot(&self, ctx: &mut Context, packet: &channel1::Shoot) {
        let id = ctx.id();
        let server = ctx.server();

        let is_evil = {
            let maps = server.extra.get::<Arc<RwLock<PlayMaps>>>().unwrap();
            maps.read().unwrap().evil_id == Some(id)
        };
        if !is_evil {
            return;
        }

        let hit_pt = [packet.to.x, packet.to.y, packet.to.z];
        let trap_idx = {
            let state = crate::map_state::get_server_state(server).read().unwrap();
            state.traps.iter().position(|trap| {
                let b_min = trap.button_pos;
                let b_max = [
                    b_min[0] + trap.button_size,
                    b_min[1] + trap.button_size,
                    b_min[2] + trap.button_size,
                ];

                (0..3).all(|ax| {
                    hit_pt[ax] >= b_min[ax] as f32 - 0.5 && hit_pt[ax] <= b_max[ax] as f32 + 0.5
                })
            })
        };

        if let Some(idx) = trap_idx {
            crate::trap::trigger_trap(server, idx, id);
        }
    }

    async fn switch_team(&self, ctx: &mut Context, _packet: &channel1::SwitchTeam) {
        ctx.cancel();
    }

    async fn map_vote(&self, ctx: &mut Context, _packet: &channel1::MapVote) {
        ctx.cancel(); // Map rotation is strictly managed by PlayMaps
    }

    async fn spectator(&self, ctx: &mut Context, packet: &channel1::SpectatorClient) {
        let server = ctx.server();
        let maps = server.extra.get::<Arc<RwLock<PlayMaps>>>().unwrap();
        let round_state = maps.read().unwrap().round_state;

        // Prevent self-unspectating during the game (even for masters to prevent cheating the 1-life limit).
        if round_state == RoundState::Playing || round_state == RoundState::MapLoading {
            if !packet.val {
                ctx.cancel();
            }
        }
    }

    async fn get_map(&self, ctx: &mut Context, _packet: &channel1::GetMap) {
        let id = ctx.id();
        let server = ctx.server();
        let maps = Arc::clone(server.extra.get::<Arc<RwLock<PlayMaps>>>().unwrap());
        if send_map_to_client(Arc::clone(server), id, &maps, false) {
            info!("Sending map rebuild to id {id} on N_GETMAP");
            ctx.cancel();
        }
    }
}
