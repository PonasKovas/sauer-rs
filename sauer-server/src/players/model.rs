use crate::Server;
use sauer_proto::{Client, channel1::SwitchModel, data::PlayerModel};

impl Server {
    /// Sets the player's model without broadcasting (call during initial connect handshake).
    pub(crate) fn init_model(&self, id: u64, model: PlayerModel) {
        if let Some(mut p) = self.players.try_get_mut(id) {
            p.model = model;
        }
    }

    /// Sets the player's model and broadcasts N_SWITCHMODEL to all clients.
    pub fn set_model(&self, id: u64, model: PlayerModel) {
        if let Some(cn) = self.get_cn(id) {
            let pkt = Client {
                client_num: cn as i32,
                packet: SwitchModel { model },
            };
            self.broadcast_chan1(&pkt);
            if let Some(mut p) = self.players.try_get_mut(id) {
                p.model = model;
            }
        }
    }
}
