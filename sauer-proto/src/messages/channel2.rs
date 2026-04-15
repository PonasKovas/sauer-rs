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
    SendDemo(SendDemo),

    /// `N_SENDMAP` Sent over Channel 2 to stream a cooperative edit map file to the client.
    SendMap(SendMap),
}


// =====================================================================
// EXTRACTED ENUM VARIANTS
// =====================================================================

/// `N_SENDDEMO` Sent over Channel 2 to stream a recorded demo file to the client.
#[derive(Clone, Debug, PartialEq)]
pub struct SendDemo {
    pub tag: i32,
    pub data: Vec<u8>,
}

impl From<SendDemo> for Chan2Server {
    fn from(v: SendDemo) -> Self {
        Self::SendDemo(v)
    }
}

/// `N_SENDMAP` Sent over Channel 2 to stream a cooperative edit map file to the client.
#[derive(Clone, Debug, PartialEq)]
pub struct SendMap {
    pub data: Vec<u8>,
}

impl From<SendMap> for Chan2Server {
    fn from(v: SendMap) -> Self {
        Self::SendMap(v)
    }
}
