use crate::data::Vector3;

#[derive(Debug, Clone, PartialEq)]
pub struct TokenBaseSpawn {
    pub team: i32,
    pub pos: Vector3<f32>,
}
