use crate::data::Vector3;

#[derive(Debug, Clone, PartialEq)]
pub struct BaseSpawn {
    pub ammotype: i32,
    pub pos: Vector3<f32>,
}
