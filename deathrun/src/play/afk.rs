//! Spectating players who stand still too long.

use super::{PlayMaps, round::check_win_conditions};
use crate::ui;

use parking_lot::RwLock;
use sauer_proto::{Client, channel1};
use sauer_server::{Server, Status};
use std::collections::HashMap;
use std::collections::HashSet;
use std::sync::Arc;
use tokio::time::{Duration, Instant};

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
    pub(super) spectated: HashSet<u64>,
}

impl Afk {
    pub fn update_pos(&mut self, id: u64, pos: [f32; 3]) {
        let moved = self.last_pos.get(&id).is_none_or(|last| {
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

pub(super) fn check_afk(server: &Arc<Server>, maps: &Arc<RwLock<PlayMaps>>, timeout: Duration) {
    let now = Instant::now();
    let warn_timeout = timeout.saturating_sub(Duration::from_secs(5));

    let mut to_spectate = Vec::new();
    let mut to_warn = Vec::new();
    {
        let lock = maps.read();
        if !lock.round_state.is_playing() {
            return;
        }
        let ids = server.joined_ids();
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
