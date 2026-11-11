use crate::Server;
use sauer_proto::{Client, channel1::SwitchModel, data::PlayerModel};

impl Server {
    /// Sets the player's model without broadcasting (call during initial connect handshake).
    pub(crate) fn init_model(&self, id: u64, model: PlayerModel) {
        self.players.with_mut(id, |p| p.model = model);
    }

    /// Sets the player's model and broadcasts N_SWITCHMODEL to all clients.
    pub fn set_model(&self, id: u64, model: PlayerModel) {
        if let Some(cn) = self.get_cn(id) {
            let pkt = Client {
                client_num: cn,
                packet: SwitchModel { model },
            };
            self.broadcast(&pkt);
            self.players.with_mut(id, |p| p.model = model);
        }
    }
}
