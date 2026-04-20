use sauer_proto::{
    channel1::SpawnState,
    data::{ArmourType, ClientState, Gun, PlayerAmmo, PlayerState},
};
use sauer_server::Server;

use crate::options::get_options;

use super::get_players;

fn make_spawn_state(insta: bool, life_sequence: i32) -> PlayerState {
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

pub(crate) fn set_initial_state(server: &Server, cn: u32) {
    let insta = {
        let options = get_options(server).read().unwrap();
        options.mode.is_insta()
    };
    if let Some(mut p) = get_players(server).try_get_mut(cn) {
        p.state = Some(make_spawn_state(insta, 0));
    }
}

/// Prepare a respawn: increment life_sequence, reset state, send N_SPAWNSTATE only to this client.
/// Does nothing if the player is not dead or hasn't been welcomed.
pub fn prepare_spawn(server: &Server, cn: u32) {
    let insta = {
        let options = get_options(server).read().unwrap();
        options.mode.is_insta()
    };

    let new_state = {
        let Some(mut p) = get_players(server).try_get_mut(cn) else { return };
        if p.client_state != ClientState::Dead {
            return;
        }
        // Not welcomed yet — state hasn't been initialised.
        let Some(ref s) = p.state else { return };
        let old_seq = s.life_sequence;
        let state = make_spawn_state(insta, (old_seq + 1) & 0x7F);
        p.state = Some(state.clone());
        p.quad_millis = 0;
        state
    };

    server.send_chan1(
        cn,
        &SpawnState {
            client_num: cn as i32,
            state: new_state,
        },
    );
}
