use dashmap::DashMap;
use sauer_proto::data::{ArmourType, Gun, PlayerAmmo, PlayerState};
use sauer_server::Server;

use crate::options::get_options;

#[derive(Clone, Debug)]
pub struct PlayerStates {
    map: DashMap<u32, PlayerState>,
}

impl PlayerStates {
    pub(crate) fn new() -> Self {
        Self {
            map: DashMap::new(),
        }
    }

    pub fn try_get(&self, cn: u32) -> Option<PlayerState> {
        self.map.get(&cn).as_deref().cloned()
    }
    pub fn get(&self, cn: u32) -> PlayerState {
        match self.try_get(cn) {
            Some(x) => x,
            None => panic!("cn {cn} not in PlayerStates list"),
        }
    }
}

impl Default for PlayerStates {
    fn default() -> Self {
        Self::new()
    }
}

pub(crate) fn clear_playerstate(server: &Server, cn: u32) {
    get_playerstates(server).map.remove(&cn);
}

pub(crate) fn set_initial_playerstate(server: &Server, cn: u32) {
    let options = get_options(server).read().unwrap();

    let insta = options.mode.is_insta();

    let health = if insta { 1 } else { 100 };
    let gun_select = if insta { Gun::Rifle } else { Gun::Pistol };
    let ammo = PlayerAmmo {
        shotgun: 0,
        chaingun: 0,
        rocket_launcher: 0,
        rifle: if insta { -1 } else { 0 },
        grenade_launcher: if insta { 0 } else { 1 },
        pistol: if insta { 0 } else { 30 },
    };

    let state = PlayerState {
        life_sequence: 0,
        health,
        max_health: 100,
        armour: 0,
        armour_type: ArmourType::Blue,
        gun_select,
        ammo,
    };

    get_playerstates(server).map.insert(cn, state);
}

pub fn get_playerstates(server: &Server) -> &PlayerStates {
    match server.extra.get() {
        Some(x) => x,
        None => panic!("PlayerStates not initialized"),
    }
}
