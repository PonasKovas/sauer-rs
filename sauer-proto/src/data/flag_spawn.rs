use crate::data::Vector3;

#[derive(Debug, Clone, PartialEq)]
pub struct FlagSpawn {
    pub team: i32,
    pub pos: Vector3<f32>,
}
