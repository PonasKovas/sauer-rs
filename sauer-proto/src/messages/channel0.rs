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
    Position { pos: PlayerPosition },

    /// `N_TELEPORT` Sent to immediately teleport a player.
    Teleport { client_num: i32, tp: i32, td: i32 },

    /// `N_JUMPPAD` Sent to apply a jump-pad acceleration.
    JumpPad { client_num: i32, jp: i32 },
}

/// Client -> Server (Channel 0)
#[derive(Debug, Clone, PartialEq)]
pub enum Chan0Client {
    /// `N_POS` Standard physical position update sent constantly.
    Position { pos: PlayerPosition },

    /// `N_TELEPORT` Broadcasts the client teleporting.
    Teleport { client_num: i32, tp: i32, td: i32 },

    /// `N_JUMPPAD` Broadcasts the client hitting a jump pad.
    JumpPad { client_num: i32, jp: i32 },
}
