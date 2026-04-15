#![allow(private_interfaces)]

use crate::internal::{ParseResultExt, ReaderCore, WriterCore, constants::*};
use crate::route::{Chan2, S2C};
use crate::{Decode, Encode, ParseError};

// =====================================================================
// CHANNEL 2: BULK FILE TRANSFERS (Reliable)
// =====================================================================

// -- Packet structs ---------------------------------------------------

/// `N_DEMOPACKET` Demo playback marker.
#[derive(Clone, Debug, PartialEq)]
pub struct DemoPacket;

/// `N_SENDDEMO` Sent over Channel 2 to stream a recorded demo file to the client.
#[derive(Clone, Debug, PartialEq)]
pub struct SendDemo {
    pub tag: i32,
    pub data: Vec<u8>,
}

/// `N_SENDMAP` Sent over Channel 2 to stream a cooperative edit map file to the client.
#[derive(Clone, Debug, PartialEq)]
pub struct SendMap {
    pub data: Vec<u8>,
}

// -- Encode / Decode for individual structs ---------------------------

impl Encode<S2C<Chan2>> for DemoPacket {
    fn encode(&self, w: &mut WriterCore) {
        w.put_int(N_DEMOPACKET);
    }
}
impl Decode<S2C<Chan2>> for DemoPacket {
    fn decode(msg_type: i32, _r: &mut ReaderCore) -> Result<Self, ParseError> {
        if msg_type != N_DEMOPACKET {
            return Err(ParseError::WrongMessageType {
                expected: N_DEMOPACKET,
                expected_name: "DemoPacket",
                actual: msg_type,
            });
        }
        Ok(DemoPacket)
    }
}

impl Encode<S2C<Chan2>> for SendDemo {
    fn encode(&self, w: &mut WriterCore) {
        w.put_int(N_SENDDEMO);
        w.put_int(self.tag);
        w.put_bytes(&self.data);
    }
}
impl Decode<S2C<Chan2>> for SendDemo {
    fn decode(msg_type: i32, r: &mut ReaderCore) -> Result<Self, ParseError> {
        if msg_type != N_SENDDEMO {
            return Err(ParseError::WrongMessageType {
                expected: N_SENDDEMO,
                expected_name: "SendDemo",
                actual: msg_type,
            });
        }
        (|| {
            let tag = r.get_int()?;
            let data = r.get_bytes(r.remaining())?.to_vec();
            Ok(SendDemo { tag, data })
        })()
        .in_packet("SendDemo")
    }
}

impl Encode<S2C<Chan2>> for SendMap {
    fn encode(&self, w: &mut WriterCore) {
        w.put_int(N_SENDMAP);
        w.put_bytes(&self.data);
    }
}
impl Decode<S2C<Chan2>> for SendMap {
    fn decode(msg_type: i32, r: &mut ReaderCore) -> Result<Self, ParseError> {
        if msg_type != N_SENDMAP {
            return Err(ParseError::WrongMessageType {
                expected: N_SENDMAP,
                expected_name: "SendMap",
                actual: msg_type,
            });
        }
        (|| {
            let data = r.get_bytes(r.remaining())?.to_vec();
            Ok(SendMap { data })
        })()
        .in_packet("SendMap")
    }
}

// =====================================================================
// ENUM (for dynamic / iterator-based reading)
// =====================================================================

/// Server -> Client (Channel 2)
#[derive(Debug, Clone, PartialEq)]
pub enum Chan2Server {
    /// `N_DEMOPACKET` Demo playback marker.
    DemoPacket(DemoPacket),
    /// `N_SENDDEMO` Stream a recorded demo file to the client.
    SendDemo(SendDemo),
    /// `N_SENDMAP` Stream a cooperative edit map file to the client.
    SendMap(SendMap),
}

// -- From impls -------------------------------------------------------

impl From<self::DemoPacket> for Chan2Server {
    fn from(v: self::DemoPacket) -> Self {
        Self::DemoPacket(v)
    }
}
impl From<SendDemo> for Chan2Server {
    fn from(v: SendDemo) -> Self {
        Self::SendDemo(v)
    }
}
impl From<SendMap> for Chan2Server {
    fn from(v: SendMap) -> Self {
        Self::SendMap(v)
    }
}

// -- Enum Encode / Decode for S2C<Chan2> ------------------------------

impl Encode<S2C<Chan2>> for Chan2Server {
    fn encode(&self, w: &mut WriterCore) {
        match self {
            Chan2Server::DemoPacket(v) => Encode::<S2C<Chan2>>::encode(v, w),
            Chan2Server::SendDemo(v) => Encode::<S2C<Chan2>>::encode(v, w),
            Chan2Server::SendMap(v) => Encode::<S2C<Chan2>>::encode(v, w),
        }
    }
}

impl Decode<S2C<Chan2>> for Chan2Server {
    fn decode(msg_type: i32, r: &mut ReaderCore) -> Result<Self, ParseError> {
        match msg_type {
            N_DEMOPACKET => Ok(Chan2Server::DemoPacket(self::DemoPacket)),
            N_SENDDEMO => {
                let tag = r.get_int()?;
                let data = r.get_bytes(r.remaining())?.to_vec();
                Ok(Chan2Server::SendDemo(SendDemo { tag, data }))
            }
            N_SENDMAP => {
                let data = r.get_bytes(r.remaining())?.to_vec();
                Ok(Chan2Server::SendMap(SendMap { data }))
            }
            _ => Err(ParseError::UnknownMessageType {
                channel: 2,
                id: msg_type,
            }),
        }
    }
}
