use cube_string::CubeString;
use dashmap::{DashMap, Entry};
use sauer_proto::{Client, channel1::SwitchName};
use sauer_server::Server;
use std::ops::Deref;

#[derive(Clone, Debug)]
pub struct Names {
    map: DashMap<u32, CubeString>,
}

impl Names {
    pub(crate) fn new() -> Self {
        Self {
            map: DashMap::new(),
        }
    }

    pub fn try_get(&self, cn: u32) -> Option<impl Deref<Target = CubeString>> {
        self.map.get(&cn)
    }
    pub fn get(&self, cn: u32) -> impl Deref<Target = CubeString> {
        match self.try_get(cn) {
            Some(x) => x,
            None => panic!("cn {cn} not in Names list"),
        }
    }
    /// Sets the name of the player in local data. Use [`set_name`] to automatically send `N_SWITCHNAME` packets to all clients
    pub fn raw_set(&self, cn: u32, name: CubeString) {
        // only set if the entry is there at all
        if let Entry::Occupied(mut entry) = self.map.entry(cn) {
            entry.insert(name);
        }
    }
}

impl Default for Names {
    fn default() -> Self {
        Self::new()
    }
}

pub(crate) fn init_name(server: &Server, cn: u32, name: CubeString) {
    get_names(server).map.insert(cn, name);
}

pub(crate) fn clear_name(server: &Server, cn: u32) {
    get_names(server).map.remove(&cn);
}

pub fn get_names(server: &Server) -> &Names {
    match server.extra.get() {
        Some(x) => x,
        None => panic!("Names not initialized"),
    }
}

pub fn set_name(server: &Server, cn: u32, name: CubeString) {
    let pkt = Client {
        client_num: cn as i32,
        packet: SwitchName { name },
    };

    server.broadcast_chan1(&pkt);

    get_names(server).raw_set(cn, pkt.packet.name);
}
