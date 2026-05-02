use crate::Server;
use cube_string::CubeString;
use sauer_proto::{Client, channel1::SwitchName};

impl Server {
    /// Sets the player's name without broadcasting (call during initial connect handshake).
    pub(crate) fn init_name(&self, id: u64, name: CubeString) {
        if let Some(mut p) = self.players.try_get_mut(id) {
            p.name = name;
        }
    }

    /// Sets the player's name and broadcasts N_SWITCHNAME to all clients.
    pub fn set_name(&self, id: u64, name: CubeString) {
        if let Some(cn) = self.get_cn(id) {
            let pkt = Client {
                client_num: cn as i32,
                packet: SwitchName { name },
            };
            self.broadcast_chan1(&pkt);
            if let Some(mut p) = self.players.try_get_mut(id) {
                p.name = pkt.packet.name;
            }
        }
    }
}
