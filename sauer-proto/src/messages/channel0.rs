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
    /// Demo playback marker. Indicates this packet was injected from a recorded `.dmo` file.
    DemoPacket,

    /// Constant physical position updates.
    Position { pos: PlayerPosition },

    /// Sent to immediately teleport a player.
    Teleport { client_num: i32, tp: i32, td: i32 },

    /// Sent to apply a jump-pad acceleration.
    JumpPad { client_num: i32, jp: i32 },
}

/// Client -> Server (Channel 0)
#[derive(Debug, Clone, PartialEq)]
pub enum Chan0Client {
    /// Standard physical position update sent constantly.
    Position { pos: PlayerPosition },

    /// Broadcasts the client teleporting.
    Teleport { client_num: i32, tp: i32, td: i32 },

    /// Broadcasts the client hitting a jump pad.
    JumpPad { client_num: i32, jp: i32 },
}
