use crate::Server;
use sauer_proto::data::Vec3;

impl Server {
    pub(crate) fn set_position(&self, id: u64, pos: Vec3<f32>) {
        self.players.with_mut(id, |p| p.position = pos);
    }
}
