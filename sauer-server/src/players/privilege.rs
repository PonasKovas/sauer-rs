use crate::Server;
use sauer_proto::{
    channel1::CurrentMaster,
    data::{MasterClient, MasterMode, Privilege},
};

impl Server {
    pub fn master_mode(&self) -> MasterMode {
        *self.master_mode.read()
    }

    /// Sets the master mode and broadcasts the updated master state to everyone.
    pub fn set_master_mode(&self, mode: MasterMode) {
        *self.master_mode.write() = mode;
        self.broadcast_current_master();
    }

    fn build_current_master(&self) -> CurrentMaster {
        let master_mode = self.master_mode();
        let clients: Vec<MasterClient> = self
            .players
            .map
            .iter()
            .filter(|e| e.privilege != Privilege::None)
            .filter_map(|e| {
                self.get_cn(*e.key()).map(|cn| MasterClient {
                    client_num: cn as i32,
                    privilege: e.privilege,
                })
            })
            .collect();
        CurrentMaster {
            master_mode,
            clients,
        }
    }

    /// Sends the current master state to a single client (call during welcome).
    pub fn send_current_master(&self, id: u64) {
        self.send_chan1(id, &self.build_current_master());
    }

    /// Broadcasts the current master state to all connected clients.
    pub fn broadcast_current_master(&self) {
        self.broadcast_chan1(&self.build_current_master());
    }

    /// Returns a player's current privilege level.
    pub fn get_privilege(&self, id: u64) -> Privilege {
        self.players
            .try_get(id)
            .map_or(Privilege::None, |p| p.privilege)
    }

    /// Sets a player's privilege level and broadcasts the updated master list to everyone.
    pub fn set_privilege(&self, id: u64, privilege: Privilege) {
        if let Some(mut p) = self.players.try_get_mut(id) {
            p.privilege = privilege;
        }
        self.broadcast_current_master();
    }
}
