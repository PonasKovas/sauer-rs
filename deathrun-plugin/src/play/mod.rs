use crate::aim::Aim;
use crate::map_state::get_server_state;
use crate::trap::frame;
use crate::ui;
use cube_string::IntoCubeStringLossy;

use parking_lot::RwLock;
use sauer_proto::data::{PlayerState, Privilege};
use sauer_proto::{
    channel0, channel1,
    data::{GameMode, MasterMode, SetTeamReason},
};
use sauer_server::{
    Flow, Server,
    plugin::{Context, Plugin},
};
use serde::{Deserialize, Serialize};
use std::sync::Arc;
use tokio::time::{Duration, sleep};
use tracing::info;

mod afk;
mod finish;
mod rotation;
mod round;

pub use afk::Afk;
use afk::check_afk;
use finish::check_finish;
pub use rotation::{LoadedMap, PlayMaps};
pub use round::RoundState;
use round::{check_win_conditions, send_map_to_client, start_round_sequence};

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

impl Plugin for DeathrunPlay {
    async fn init(&self, server: &mut Server) {
        self.notes.install(server);
        {
            let game = server.game.get_mut();
            game.map_name = "".lossy_cubestring();
            game.mode = GameMode::TeamPlay;
            game.master_mode = MasterMode::Veto;
        }
        server.extra.insert(crate::map_sync::KnownVars::default());
        // Rounds set the master mode, so it must not snap back to open.
        server.config.write().reopen_when_masterless = false;
        crate::add_server_bot(server);

        let state = self.maps.read().make_active_state().unwrap_or_default();
        server.extra.insert(RwLock::new(state));
        server.extra.insert(Arc::clone(&self.maps));
        server
            .extra
            .insert(crate::trap::placement::SeeThrough::default());
    }

    async fn start(&self, server: &Arc<Server>) {
        let maps = Arc::clone(&self.maps);
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
        let maps = Arc::clone(&self.maps);

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
                let target = state.trap_at(aim.hit_point(&state.map));
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
        let maps = Arc::clone(&self.maps);
        maps.read().afk.lock().remove(id);
        if let Some(known) = server.extra.get::<crate::map_sync::KnownVars>() {
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
        let maps = Arc::clone(&self.maps);

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
        let maps = &self.maps;

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
            let maps = &self.maps;
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

    async fn spectator(&self, _ctx: &mut Context, packet: &channel1::SpectatorClient) -> Flow {
        let maps = &self.maps;
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
        let maps = Arc::clone(&self.maps);
        if send_map_to_client(Arc::clone(server), id, &maps, false) {
            info!("Sending map rebuild to id {id} on N_GETMAP");
            return Flow::Cancel;
        }
        Flow::Continue
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
