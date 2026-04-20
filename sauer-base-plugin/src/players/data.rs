use cube_string::CubeString;
use sauer_proto::data::{ClientState, PlayerModel, PlayerState, Vec3};

/// Stored clipboard geometry data for coop edit clipboard relay.
#[derive(Clone, Debug)]
pub struct ClipboardData {
    pub unpack_len: i32,
    pub data: Vec<u8>,
}

#[derive(Clone, Debug)]
pub struct PlayerData {
    pub session_id: i32,
    pub name: CubeString,
    pub model: PlayerModel,
    pub team: CubeString,
    pub client_state: ClientState,
    pub state: Option<PlayerState>,
    pub position: Vec3<f32>,
    pub frags: i32,
    pub deaths: i32,
    /// Compressed clipboard geometry, set when N_CLIPBOARD is received.
    pub clipboard: Option<ClipboardData>,
}

impl PlayerData {
    pub(crate) fn new(session_id: i32) -> Self {
        Self {
            session_id,
            name: CubeString::new(),
            model: PlayerModel::MrFixit,
            team: CubeString::new(),
            client_state: ClientState::Dead,
            state: None,
            position: Vec3 { x: 0.0, y: 0.0, z: 0.0 },
            frags: 0,
            deaths: 0,
            clipboard: None,
        }
    }
}
