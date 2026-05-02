use crate::Server;
use sauer_proto::data::Vec3;

impl Server {
    pub(crate) fn set_position(&self, id: u64, pos: Vec3<f32>) {
        if let Some(mut p) = self.players.try_get_mut(id) {
            p.position = pos;
        }
    }
}
