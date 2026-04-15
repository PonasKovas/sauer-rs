use crate::data::*;

mod decode_client;
mod decode_server;
mod encode_client;
mod encode_server;

// =====================================================================
// CHANNEL 0: PHYSICS AND MOVEMENT (High Frequency)
// =====================================================================

/// Server -> Client (Channel 0)
#[derive(Debug, Clone, PartialEq)]
pub enum Chan0Server {
    /// `N_DEMOPACKET` Demo playback marker. Indicates this packet was injected from a recorded `.dmo` file.
    DemoPacket,

    /// `N_POS` Constant physical position updates.
    Position(Position),

    /// `N_TELEPORT` Sent to immediately teleport a player.
    Teleport(Teleport),

    /// `N_JUMPPAD` Sent to apply a jump-pad acceleration.
    JumpPad(JumpPad),
}

/// Client -> Server (Channel 0)
#[derive(Debug, Clone, PartialEq)]
pub enum Chan0Client {
    /// `N_POS` Standard physical position update sent constantly.
    Position(Position),

    /// `N_TELEPORT` Broadcasts the client teleporting.
    Teleport(Teleport),

    /// `N_JUMPPAD` Broadcasts the client hitting a jump pad.
    JumpPad(JumpPad),
}

// =====================================================================
// EXTRACTED ENUM VARIANTS
// =====================================================================

/// `N_POS` Constant physical position updates.
#[derive(Clone, Debug, PartialEq)]
pub struct Position {
    pub pos: PlayerPosition,
}

impl From<Position> for Chan0Server {
    fn from(v: Position) -> Self {
        Self::Position(v)
    }
}

/// `N_TELEPORT` Sent to immediately teleport a player.
#[derive(Clone, Debug, PartialEq)]
pub struct Teleport {
    pub client_num: i32,
    pub tp: i32,
    pub td: i32,
}

impl From<Teleport> for Chan0Server {
    fn from(v: Teleport) -> Self {
        Self::Teleport(v)
    }
}

/// `N_JUMPPAD` Sent to apply a jump-pad acceleration.
#[derive(Clone, Debug, PartialEq)]
pub struct JumpPad {
    pub client_num: i32,
    pub jp: i32,
}

impl From<JumpPad> for Chan0Server {
    fn from(v: JumpPad) -> Self {
        Self::JumpPad(v)
    }
}
