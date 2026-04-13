use crate::data::Vector3;

#[derive(Debug, Clone, PartialEq)]
pub struct TokenState {
    pub id: i32,
    pub team: i32,
    pub yaw: i32,
    pub pos: Vector3<f32>,
}
