use sauer_proto::{Client, channel1::SwitchModel, data::PlayerModel};
use sauer_server::Server;

use super::get_players;

pub(crate) fn init_model(server: &Server, cn: u32, model: PlayerModel) {
    if let Some(mut p) = get_players(server).try_get_mut(cn) {
        p.model = model;
    }
}

pub fn set_model(server: &Server, cn: u32, model: PlayerModel) {
    let pkt = Client {
        client_num: cn as i32,
        packet: SwitchModel { model },
    };
    server.broadcast_chan1(&pkt);
    if let Some(mut p) = get_players(server).try_get_mut(cn) {
        p.model = model;
    }
}
