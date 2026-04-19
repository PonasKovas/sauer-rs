use cube_string::CubeString;
use sauer_proto::data::{PlayerModel, PlayerState, Vec3};

#[derive(Clone, Debug)]
pub struct PlayerData {
    pub session_id: i32,
    pub name: CubeString,
    pub model: PlayerModel,
    pub team: CubeString,
    pub state: Option<PlayerState>,
    pub position: Vec3<f32>,
}

impl PlayerData {
    pub(crate) fn new(session_id: i32) -> Self {
        Self {
            session_id,
            name: CubeString::new(),
            model: PlayerModel::MrFixit,
            team: CubeString::new(),
            state: None,
            position: Vec3 { x: 0.0, y: 0.0, z: 0.0 },
        }
    }
}
