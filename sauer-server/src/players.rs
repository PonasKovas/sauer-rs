use dashmap::DashMap;
use rand::random;
use std::ops::{Deref, DerefMut};

pub mod combat;
mod data;
pub mod model;
pub mod name;
pub mod position;
pub mod privilege;
pub mod spectator;
pub mod state;
pub mod team;

pub use data::{ClipboardData, PlayerData};

pub struct Players {
    pub(crate) map: DashMap<u32, PlayerData>,
}

impl Default for Players {
    fn default() -> Self {
        Self {
            map: DashMap::new(),
        }
    }
}

impl Players {
    pub(crate) fn new() -> Self {
        Self::default()
    }

    pub fn try_get(&self, cn: u32) -> Option<impl Deref<Target = PlayerData> + '_> {
        self.map.get(&cn)
    }

    pub fn get(&self, cn: u32) -> impl Deref<Target = PlayerData> + '_ {
        self.map
            .get(&cn)
            .unwrap_or_else(|| panic!("cn {cn} not in Players"))
    }

    pub fn try_get_mut(&self, cn: u32) -> Option<impl DerefMut<Target = PlayerData> + '_> {
        self.map.get_mut(&cn)
    }
}

/// Creates a new player entry with a random session id. Returns the session id.
pub(crate) fn init_player(players: &Players, cn: u32) -> i32 {
    let session_id = random();
    players.map.insert(cn, PlayerData::new(session_id));
    session_id
}

pub(crate) fn clear_player(players: &Players, cn: u32) {
    players.map.remove(&cn);
}
