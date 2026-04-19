use sauer_proto::data::Vec3;
use sauer_server::Server;

use super::get_players;

pub fn set_position(server: &Server, cn: u32, pos: Vec3<f32>) {
    if let Some(mut p) = get_players(server).try_get_mut(cn) {
        p.position = pos;
    }
}
