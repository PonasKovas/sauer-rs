use crate::data::PlayerState;
use num_enum::TryFromPrimitive;

/// State for a client resuming a game (e.g., late join).
#[derive(Debug, Clone, PartialEq)]
pub struct ClientResumeState {
    pub client_num: i32,
    pub state: ClientState,
    pub frags: i32,
    pub flags: i32,
    pub deaths: i32,
    pub quad_millis: i32,
    pub player_state: PlayerState,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord, TryFromPrimitive)]
#[repr(i32)]
pub enum ClientState {
    Alive = 0,
    Dead = 1,
    Spawning = 2,
    Lagged = 3,
    Editing = 4,
    Spectator = 5,
}
