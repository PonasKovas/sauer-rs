use dashmap::DashMap;
use rand::random;
use sauer_server::Server;
use std::ops::{Deref, DerefMut};

pub mod combat;
mod data;
pub mod model;
pub mod name;
pub mod position;
pub mod state;
pub mod team;

pub use data::PlayerData;

pub struct Players {
    pub(crate) map: DashMap<u32, PlayerData>,
}

impl Players {
    pub(crate) fn new() -> Self {
        Self {
            map: DashMap::new(),
        }
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

pub fn get_players(server: &Server) -> &Players {
    server.extra.get().expect("Players not initialized")
}

/// Creates a new player entry with a random session id. Returns the session id.
pub(crate) fn init_player(server: &Server, cn: u32) -> i32 {
    let session_id = random();
    get_players(server).map.insert(cn, PlayerData::new(session_id));
    session_id
}

pub(crate) fn clear_player(server: &Server, cn: u32) {
    get_players(server).map.remove(&cn);
}
