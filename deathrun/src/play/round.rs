//! Rounds: picking the terrorist, loading the map, deciding who won.

use super::PlayMaps;
use crate::map_state::get_server_state;
use crate::ui;
use cube_string::IntoCubeStringLossy;

use parking_lot::RwLock;
use rand::seq::SliceRandom;
use sauer_proto::{
    Client, channel1,
    data::{MasterMode, SetTeamReason},
};
use sauer_server::{Server, Status};
use std::sync::Arc;
use tokio::time::{Duration, Instant, sleep};

#[derive(PartialEq, Clone, Copy)]
pub enum RoundState {
    NotStarted,
    MapLoading,
    Playing { started: Instant },
    Ended,
}

impl RoundState {
    pub fn is_playing(self) -> bool {
        matches!(self, RoundState::Playing { .. })
    }

    /// When the running round started; `None` outside a round.
    pub fn started(self) -> Option<Instant> {
        match self {
            RoundState::Playing { started } => Some(started),
            _ => None,
        }
    }
}

pub(super) fn start_round_sequence(
    server: Arc<Server>,
    maps: Arc<RwLock<PlayMaps>>,
    welcome_id: Option<u64>,
) {
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

        let Some(new_state) = lock.make_active_state() else {
            lock.round_state = RoundState::NotStarted;
            return;
        };
        let map = new_state.map.clone();
        *get_server_state(&server).write() = new_state;
        (map, epoch, server.joined_ids())
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

        crate::map_sync::rebuild(&server, &ids, &map, evil_id).await;

        let current_evil;
        {
            let mut lock = maps_clone.write();
            if lock.round_epoch != my_epoch {
                return;
            }
            lock.round_state = RoundState::Playing {
                started: Instant::now(),
            };
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

pub(super) fn check_win_conditions(
    server: &Arc<Server>,
    maps: &Arc<RwLock<PlayMaps>>,
    disconnecting_id: Option<u64>,
) {
    let mut lock = maps.write();
    if !lock.round_state.is_playing() {
        return;
    }

    let mut ids = server.joined_ids();
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
    if lock.round_epoch != epoch || !lock.round_state.is_playing() {
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

    for id in server.joined_ids() {
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

pub(super) fn send_map_to_client(
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
        crate::map_sync::rebuild(&server, &[target_id], &map, evil_id).await;

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
