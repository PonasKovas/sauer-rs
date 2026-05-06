use cube_string::CubeString;
use sauer_proto::data::{ClientState, PlayerModel, PlayerState, Privilege, Vec3};

/// Stored clipboard geometry data for coop edit clipboard relay.
#[derive(Clone, Debug)]
pub struct ClipboardData {
    pub payload: sauer_proto::data::ClipboardPayload,
}

#[derive(Clone, Debug)]
pub struct PlayerData {
    pub session_id: i32,
    pub name: CubeString,
    pub model: PlayerModel,
    pub team: CubeString,
    pub client_state: ClientState,
    pub edit_state: ClientState,
    /// `None` until `welcome()` is called (sentinel for "has been welcomed").
    pub state: Option<PlayerState>,
    pub position: Vec3<f32>,
    pub ping: i32,
    pub frags: i32,
    pub flags: i32,
    pub deaths: i32,
    pub teamkills: i32,
    /// Compressed clipboard geometry, set when N_CLIPBOARD is received.
    pub clipboard: Option<ClipboardData>,
    /// Remaining quad damage milliseconds. Decremented each server tick.
    pub quad_millis: i32,
    pub privilege: Privilege,
    pub known_vars: std::collections::HashMap<CubeString, ogz_rs::data::Variable>,
}

impl PlayerData {
    pub(crate) fn new(session_id: i32) -> Self {
        Self {
            session_id,
            name: CubeString::new(),
            model: PlayerModel::MrFixit,
            team: CubeString::new(),
            client_state: ClientState::Dead,
            edit_state: ClientState::Dead,
            state: None,
            position: Vec3 {
                x: 0.0,
                y: 0.0,
                z: 0.0,
            },
            ping: 0,
            frags: 0,
            flags: 0,
            deaths: 0,
            teamkills: 0,
            clipboard: None,
            quad_millis: 0,
            privilege: Privilege::None,
            known_vars: std::collections::HashMap::new(),
        }
    }
}
