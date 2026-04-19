use sauer_proto::data::{ArmourType, Gun, PlayerAmmo, PlayerState};
use sauer_server::Server;

use crate::options::get_options;

use super::get_players;

pub(crate) fn set_initial_state(server: &Server, cn: u32) {
    let insta = {
        let options = get_options(server).read().unwrap();
        options.mode.is_insta()
    };

    let state = PlayerState {
        life_sequence: 0,
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
    };

    if let Some(mut p) = get_players(server).try_get_mut(cn) {
        p.state = Some(state);
    }
}
