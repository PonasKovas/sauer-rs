use dashmap::{DashMap, Entry};
use sauer_proto::{Client, channel1::SwitchModel, data::PlayerModel};
use sauer_server::Server;

#[derive(Clone, Debug)]
pub struct PlayerModels {
    map: DashMap<u32, PlayerModel>,
}

impl PlayerModels {
    pub(crate) fn new() -> Self {
        Self {
            map: DashMap::new(),
        }
    }

    pub fn try_get(&self, cn: u32) -> Option<PlayerModel> {
        self.map.get(&cn).as_deref().copied()
    }
    pub fn get(&self, cn: u32) -> PlayerModel {
        match self.try_get(cn) {
            Some(x) => x,
            None => panic!("cn {cn} not in playermodel list"),
        }
    }
    /// Sets the model of the player in local data. Use [`set_playermodel`] to automatically send `N_SWITCHMODEL` packets to all clients
    pub fn raw_set(&self, cn: u32, name: PlayerModel) {
        // only set if the entry is there at all
        if let Entry::Occupied(mut entry) = self.map.entry(cn) {
            entry.insert(name);
        }
    }
}

impl Default for PlayerModels {
    fn default() -> Self {
        Self::new()
    }
}

pub(crate) fn init_playermodel(server: &Server, cn: u32, model: PlayerModel) {
    get_playermodels(server).map.insert(cn, model);
}

pub(crate) fn clear_playermodel(server: &Server, cn: u32) {
    get_playermodels(server).map.remove(&cn);
}

pub fn get_playermodels(server: &Server) -> &PlayerModels {
    match server.extra.get() {
        Some(x) => x,
        None => panic!("PlayerModels not initialized"),
    }
}

pub fn set_playermodel(server: &Server, cn: u32, model: PlayerModel) {
    let pkt = Client {
        client_num: cn as i32,
        packet: SwitchModel { model },
    };

    server.broadcast_chan1(&pkt);

    get_playermodels(server).raw_set(cn, model);
}
