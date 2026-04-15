use crate::data::{ItemType, Vec3};

#[derive(Debug, Clone, PartialEq)]
pub struct BaseSpawn {
    /// The ammo type stored at this base, or `None` if the base holds no ammo.
    pub ammotype: Option<ItemType>,
    pub pos: Vec3<f32>,
}
