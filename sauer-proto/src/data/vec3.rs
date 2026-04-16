use crate::{
    ParseError,
    internal::{SauerRead, SauerWrite},
};

/// 3D Vector
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash, Copy)]
pub struct Vec3<T> {
    pub x: T,
    pub y: T,
    pub z: T,
}

pub(crate) fn read_quantized<const Q: u32>(reader: &mut &[u8]) -> Result<Vec3<f32>, ParseError> {
    Ok(Vec3 {
        x: i32::sauer_read(reader)? as f32 / Q as f32,
        y: i32::sauer_read(reader)? as f32 / Q as f32,
        z: i32::sauer_read(reader)? as f32 / Q as f32,
    })
}
pub(crate) fn write_quantized<const Q: u32>(val: &Vec3<f32>, out: &mut Vec<u8>) {
    ((val.x * Q as f32) as i32).sauer_write(out);
    ((val.y * Q as f32) as i32).sauer_write(out);
    ((val.z * Q as f32) as i32).sauer_write(out);
}

impl SauerWrite for Vec3<i32> {
    fn sauer_write(&self, out: &mut Vec<u8>) {
        self.x.sauer_write(out);
        self.y.sauer_write(out);
        self.z.sauer_write(out);
    }
}
impl SauerRead for Vec3<i32> {
    fn sauer_read(reader: &mut &[u8]) -> Result<Self, ParseError> {
        Ok(Vec3 {
            x: i32::sauer_read(reader)?,
            y: i32::sauer_read(reader)?,
            z: i32::sauer_read(reader)?,
        })
    }
}
