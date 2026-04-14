// =====================================================================
// CHANNEL 2: BULK FILE TRANSFERS (Reliable)
// =====================================================================

mod decode_server;
mod encode_server;

/// Server -> Client (Channel 2)
#[derive(Debug, Clone, PartialEq)]
pub enum Chan2Server {
    /// `N_DEMOPACKET` Demo playback marker.
    DemoPacket,

    /// `N_SENDDEMO` Sent over Channel 2 to stream a recorded demo file to the client.
    SendDemo { tag: i32, data: Vec<u8> },

    /// `N_SENDMAP` Sent over Channel 2 to stream a cooperative edit map file to the client.
    SendMap { data: Vec<u8> },
}
