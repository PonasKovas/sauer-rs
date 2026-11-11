use crate::Server;
use cube_string::CubeString;
use sauer_proto::{Client, channel1::SwitchName};

impl Server {
    /// The player's name, or an empty one if there is no such player.
    pub fn player_name(&self, id: u64) -> CubeString {
        self.players
            .with(id, |p| p.name.clone())
            .unwrap_or_default()
    }

    /// Sets the player's name without broadcasting (call during initial connect handshake).
    pub(crate) fn init_name(&self, id: u64, name: CubeString) {
        self.players.with_mut(id, |p| p.name = name);
    }

    /// Sets the player's name and broadcasts N_SWITCHNAME to all clients.
    pub fn set_name(&self, id: u64, name: CubeString) {
        if let Some(cn) = self.get_cn(id) {
            let pkt = Client {
                client_num: cn,
                packet: SwitchName { name },
            };
            self.broadcast(&pkt);
            self.players.with_mut(id, |p| p.name = pkt.packet.name);
        }
    }
}
