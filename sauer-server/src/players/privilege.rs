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
        let clients = self.players.filter_map(|id, p| {
            if p.privilege == Privilege::None {
                return None;
            }
            self.get_cn(id).map(|cn| MasterClient {
                client_num: cn as i32,
                privilege: p.privilege,
            })
        });
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
            .with(id, |p| p.privilege)
            .unwrap_or(Privilege::None)
    }

    /// Sets a player's privilege level and broadcasts the updated master list to everyone.
    pub fn set_privilege(&self, id: u64, privilege: Privilege) {
        self.players.with_mut(id, |p| p.privilege = privilege);
        self.broadcast_current_master();
    }
}
