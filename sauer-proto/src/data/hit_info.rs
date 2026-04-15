use crate::{
    data::Vector3,
    internal::{constants::*, DecodeCore, EncodeCore, ReaderCore, WriterCore},
    ParseError,
};

/// Contains ray/hit information from a weapon shot or explosion.
#[derive(Debug, Clone, PartialEq)]
pub struct HitInfo {
    pub target_cn: i32,
    pub life_sequence: i32,
    pub dist: f32,
    pub rays: i32,
    pub dir: Vector3<f32>,
}

impl DecodeCore for HitInfo {
    fn decode(r: &mut ReaderCore) -> Result<Self, ParseError> {
        Ok(HitInfo {
            target_cn: r.get_int()?,
            life_sequence: r.get_int()?,
            dist: r.get_int()? as f32 / DMF,
            rays: r.get_int()?,
            dir: Vector3 {
                x: r.get_int()? as f32 / 100.0,
                y: r.get_int()? as f32 / 100.0,
                z: r.get_int()? as f32 / 100.0,
            },
        })
    }
}

impl EncodeCore for HitInfo {
    fn encode(&self, w: &mut WriterCore) {
        w.put_int(self.target_cn);
        w.put_int(self.life_sequence);
        w.put_int((self.dist * DMF) as i32);
        w.put_int(self.rays);
        w.put_int((self.dir.x * 100.0) as i32);
        w.put_int((self.dir.y * 100.0) as i32);
        w.put_int((self.dir.z * 100.0) as i32);
    }
}
