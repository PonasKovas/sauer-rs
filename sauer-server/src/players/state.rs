use crate::Server;
use sauer_proto::{
    channel1::SpawnState,
    data::{ArmourType, ClientState, Gun, PlayerAmmo, PlayerState},
};

pub(crate) fn make_spawn_state(insta: bool, life_sequence: i32) -> PlayerState {
    PlayerState {
        life_sequence,
        health: if insta { 1 } else { 100 },
        max_health: 100,
        armour: 0,
        armour_type: ArmourType::Blue,
        gun_select: if insta { Gun::Rifle } else { Gun::Pistol },
        ammo: PlayerAmmo {
            shotgun: 0,
            chaingun: 0,
            rocket_launcher: 0,
            rifle: if insta { -1 } else { 0 },
            grenade_launcher: if insta { 0 } else { 1 },
            pistol: if insta { 0 } else { 30 },
        },
    }
}

impl Server {
    /// Initialises the player's spawn state when first welcomed. Only call from `welcome()`.
    pub(crate) fn set_initial_state(&self, cn: u32) {
        let insta = self.game_mode.read().unwrap().is_insta();
        if let Some(mut p) = self.players.try_get_mut(cn) {
            p.state = Some(make_spawn_state(insta, 0));
        }
    }

    /// Computes the new spawn state for `cn` without applying any side effects.
    /// Returns `None` if the player is not in `Dead` state or hasn't been initialised yet.
    pub(crate) fn compute_spawn_state(&self, cn: u32) -> Option<PlayerState> {
        let insta = self.game_mode.read().unwrap().is_insta();
        let p = self.players.try_get(cn)?;
        if p.client_state != ClientState::Dead {
            return None;
        }
        let old_seq = p.state.as_ref()?.life_sequence;
        Some(make_spawn_state(insta, (old_seq + 1) & 0x7F))
    }

    /// Applies a previously computed spawn state to `cn` and sends `N_SPAWNSTATE`.
    pub(crate) fn apply_spawn_state(&self, cn: u32, state: PlayerState) {
        if let Some(mut p) = self.players.try_get_mut(cn) {
            p.state = Some(state.clone());
            p.quad_millis = 0;
        }
        self.send_chan1(
            cn,
            &SpawnState {
                client_num: cn as i32,
                state,
            },
        );
    }

    /// Prepare a respawn: increment life_sequence, reset state, send N_SPAWNSTATE to this client.
    /// Does nothing if the player is not dead or hasn't been welcomed.
    /// Note: this bypasses the `on_spawn` plugin hook. Prefer using the hook when possible.
    pub fn prepare_spawn(&self, cn: u32) {
        if let Some(state) = self.compute_spawn_state(cn) {
            self.apply_spawn_state(cn, state);
        }
    }

    /// Broadcasts N_SPAWNSTATE for a newly welcomed player to everyone.
    pub(crate) fn broadcast_spawn_state(&self, cn: u32) {
        let state = self
            .players
            .get(cn)
            .state
            .clone()
            .expect("broadcast_spawn_state called before state was initialized");
        self.broadcast_chan1(&SpawnState {
            client_num: cn as i32,
            state,
        });
    }
}
