use dashmap::DashMap;
use rand::random;
use sauer_server::Server;

#[derive(Clone, Debug)]
pub struct SessionIds {
    map: DashMap<u32, i32>,
}

impl SessionIds {
    pub(crate) fn new() -> Self {
        Self {
            map: DashMap::new(),
        }
    }

    pub fn try_get(&self, cn: u32) -> Option<i32> {
        self.map.get(&cn).as_deref().copied()
    }
    pub fn get(&self, cn: u32) -> i32 {
        match self.try_get(cn) {
            Some(x) => x,
            None => panic!("client num doesnt have session id"),
        }
    }
}

impl Default for SessionIds {
    fn default() -> Self {
        Self::new()
    }
}

/// creates a new session id and adds it to the list
pub(crate) fn create_session_id(server: &Server, cn: u32) -> i32 {
    let id = random();

    server.extra.get::<SessionIds>().unwrap().map.insert(cn, id);

    id
}

pub(crate) fn clear_session_id(server: &Server, cn: u32) {
    server.extra.get::<SessionIds>().unwrap().map.remove(&cn);
}
