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
    pub(crate) map: DashMap<u64, PlayerData>,
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

    pub fn try_get(&self, id: u64) -> Option<impl Deref<Target = PlayerData> + '_> {
        self.map.get(&id)
    }

    pub fn get(&self, id: u64) -> impl Deref<Target = PlayerData> + '_ {
        self.map
            .get(&id)
            .unwrap_or_else(|| panic!("id {id} not in Players"))
    }

    pub fn try_get_mut(&self, id: u64) -> Option<impl DerefMut<Target = PlayerData> + '_> {
        self.map.get_mut(&id)
    }
}

/// Creates a new player entry with a random session id. Returns the session id.
pub(crate) fn init_player(players: &Players, id: u64) -> i32 {
    let session_id = random();
    players.map.insert(id, PlayerData::new(session_id));
    session_id
}

pub(crate) fn clear_player(players: &Players, id: u64) {
    players.map.remove(&id);
}
