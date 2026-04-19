use cube_string::CubeString;
use sauer_proto::{Client, channel1::SwitchName};
use sauer_server::Server;

use super::get_players;

pub(crate) fn init_name(server: &Server, cn: u32, name: CubeString) {
    if let Some(mut p) = get_players(server).try_get_mut(cn) {
        p.name = name;
    }
}

pub fn set_name(server: &Server, cn: u32, name: CubeString) {
    let pkt = Client {
        client_num: cn as i32,
        packet: SwitchName { name },
    };
    server.broadcast_chan1(&pkt);
    if let Some(mut p) = get_players(server).try_get_mut(cn) {
        p.name = pkt.packet.name;
    }
}
