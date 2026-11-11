use crate::{Server, Status};
use sauer_proto::{
    channel1::SpawnState,
    data::{ArmourType, Gun, PlayerAmmo, PlayerState},
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
    pub(crate) fn set_initial_state(&self, id: u64) {
        let insta = self.game.read().mode.is_insta();
        self.players
            .with_mut(id, |p| p.state = Some(make_spawn_state(insta, 0)));
    }

    /// Computes the new spawn state for `id` without applying any side effects.
    /// Returns `None` if the player is not in `Dead` state or hasn't been initialised yet.
    pub fn compute_spawn_state(&self, id: u64) -> Option<PlayerState> {
        let insta = self.game.read().mode.is_insta();
        let old_seq = self.players.with(id, |p| {
            (p.status == Status::Dead)
                .then(|| p.state.as_ref().map(|s| s.life_sequence))
                .flatten()
        })??;
        Some(make_spawn_state(insta, (old_seq + 1) & 0x7F))
    }

    /// Applies a previously computed spawn state to `id` and sends `N_SPAWNSTATE`.
    pub fn apply_spawn_state(&self, id: u64, state: PlayerState) {
        self.players.with_mut(id, |p| {
            p.state = Some(state.clone());
            p.quad_millis = 0;
        });
        if let Some(cn) = self.get_cn(id) {
            self.send(
                id,
                &SpawnState {
                    client_num: cn,
                    state,
                },
            );
        }
    }

    /// Forces a player to spawn.
    /// It only works when the client is dead, and correctly sets the default loadout and `life_sequence`.
    /// The closure `f` is called with the `PlayerState` before it is applied, allowing for loadout customization.
    pub fn force_spawn<F>(&self, id: u64, mut f: F)
    where
        F: FnMut(&mut PlayerState),
    {
        if let Some(mut state) = self.compute_spawn_state(id) {
            f(&mut state);
            self.apply_spawn_state(id, state);
        }
    }

    /// Broadcasts N_SPAWNSTATE for a newly welcomed player to everyone.
    pub(crate) fn broadcast_spawn_state(&self, id: u64) {
        let Some(state) = self.players.with(id, |p| p.state.clone()).flatten() else {
            return;
        };
        if let Some(cn) = self.get_cn(id) {
            self.broadcast(&SpawnState {
                client_num: cn,
                state,
            });
        }
    }
}
