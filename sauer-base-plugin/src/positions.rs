use dashmap::{DashMap, Entry};
use sauer_proto::data::Vec3;
use sauer_server::Server;

#[derive(Clone, Debug)]
pub struct Positions {
    map: DashMap<u32, Vec3<f32>>,
}

impl Positions {
    pub(crate) fn new() -> Self {
        Self {
            map: DashMap::new(),
        }
    }

    pub fn try_get(&self, cn: u32) -> Option<Vec3<f32>> {
        self.map.get(&cn).as_deref().cloned()
    }
    pub fn get(&self, cn: u32) -> Vec3<f32> {
        match self.try_get(cn) {
            Some(x) => x,
            None => panic!("cn {cn} not in positions list"),
        }
    }
    pub fn set(&self, cn: u32, pos: Vec3<f32>) {
        // only set if the entry is there at all
        if let Entry::Occupied(mut entry) = self.map.entry(cn) {
            entry.insert(pos);
        }
    }
}

impl Default for Positions {
    fn default() -> Self {
        Self::new()
    }
}

pub(crate) fn init_position(server: &Server, cn: u32) {
    get_positions(server).map.insert(
        cn,
        Vec3 {
            x: 0.0,
            y: 0.0,
            z: 0.0,
        },
    );
}

pub(crate) fn clear_positions(server: &Server, cn: u32) {
    get_positions(server).map.remove(&cn);
}

pub fn get_positions(server: &Server) -> &Positions {
    match server.extra.get() {
        Some(x) => x,
        None => panic!("Positions not initialized"),
    }
}
