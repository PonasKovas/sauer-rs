#![allow(private_interfaces)]

use crate::data::*;
use crate::internal::{
    DecodeCore, EncodeCore, ParseResultExt, ReaderCore, WriterCore, constants::*,
};
use crate::route::{C2S, Chan0, S2C};
use crate::{Decode, Encode, ParseError};

// =====================================================================
// CHANNEL 0: PHYSICS AND MOVEMENT (High Frequency)
// =====================================================================

// -- Packet structs ---------------------------------------------------

/// `N_POS` Constant physical position updates.
#[derive(Clone, Debug, PartialEq)]
pub struct Position {
    pub pos: PlayerPosition,
}

/// `N_TELEPORT` Sent to immediately teleport a player.
#[derive(Clone, Debug, PartialEq)]
pub struct Teleport {
    pub client_num: i32,
    pub tp: i32,
    pub td: i32,
}

/// `N_JUMPPAD` Sent to apply a jump-pad acceleration.
#[derive(Clone, Debug, PartialEq)]
pub struct JumpPad {
    pub client_num: i32,
    pub jp: i32,
}

/// `N_DEMOPACKET` Demo playback marker (server only).
#[derive(Clone, Debug, PartialEq)]
pub struct DemoPacket;

// -- Encode impls for individual structs ------------------------------

fn encode_position(v: &Position, w: &mut WriterCore) {
    w.put_int(N_POS);
    v.pos.encode(w);
}

fn encode_teleport(v: &Teleport, w: &mut WriterCore) {
    w.put_int(N_TELEPORT);
    w.put_int(v.client_num);
    w.put_int(v.tp);
    w.put_int(v.td);
}

fn encode_jumppad(v: &JumpPad, w: &mut WriterCore) {
    w.put_int(N_JUMPPAD);
    w.put_int(v.client_num);
    w.put_int(v.jp);
}

fn decode_position(r: &mut ReaderCore) -> Result<Position, ParseError> {
    Ok(Position {
        pos: PlayerPosition::decode(r).in_field("pos")?,
    })
}

fn decode_teleport(r: &mut ReaderCore) -> Result<Teleport, ParseError> {
    Ok(Teleport {
        client_num: r.get_int()?,
        tp: r.get_int()?,
        td: r.get_int()?,
    })
}

fn decode_jumppad(r: &mut ReaderCore) -> Result<JumpPad, ParseError> {
    Ok(JumpPad {
        client_num: r.get_int()?,
        jp: r.get_int()?,
    })
}

// -- Position: valid on both S2C<Chan0> and C2S<Chan0> ----------------

impl Encode<S2C<Chan0>> for Position {
    fn encode(&self, w: &mut WriterCore) {
        encode_position(self, w);
    }
}
impl Encode<C2S<Chan0>> for Position {
    fn encode(&self, w: &mut WriterCore) {
        encode_position(self, w);
    }
}
impl Decode<S2C<Chan0>> for Position {
    fn decode(msg_type: i32, r: &mut ReaderCore) -> Result<Self, ParseError> {
        if msg_type != N_POS {
            return Err(ParseError::WrongMessageType {
                expected: N_POS,
                expected_name: "Position",
                actual: msg_type,
            });
        }
        decode_position(r).in_packet("Position")
    }
}
impl Decode<C2S<Chan0>> for Position {
    #[allow(private_interfaces)]
    fn decode(msg_type: i32, r: &mut ReaderCore) -> Result<Self, ParseError> {
        if msg_type != N_POS {
            return Err(ParseError::WrongMessageType {
                expected: N_POS,
                expected_name: "Position",
                actual: msg_type,
            });
        }
        decode_position(r).in_packet("Position")
    }
}

// -- Teleport: valid on both S2C<Chan0> and C2S<Chan0> ----------------

impl Encode<S2C<Chan0>> for Teleport {
    fn encode(&self, w: &mut WriterCore) {
        encode_teleport(self, w);
    }
}
impl Encode<C2S<Chan0>> for Teleport {
    fn encode(&self, w: &mut WriterCore) {
        encode_teleport(self, w);
    }
}
impl Decode<S2C<Chan0>> for Teleport {
    fn decode(msg_type: i32, r: &mut ReaderCore) -> Result<Self, ParseError> {
        if msg_type != N_TELEPORT {
            return Err(ParseError::WrongMessageType {
                expected: N_TELEPORT,
                expected_name: "Teleport",
                actual: msg_type,
            });
        }
        decode_teleport(r).in_packet("Teleport")
    }
}
impl Decode<C2S<Chan0>> for Teleport {
    fn decode(msg_type: i32, r: &mut ReaderCore) -> Result<Self, ParseError> {
        if msg_type != N_TELEPORT {
            return Err(ParseError::WrongMessageType {
                expected: N_TELEPORT,
                expected_name: "Teleport",
                actual: msg_type,
            });
        }
        decode_teleport(r).in_packet("Teleport")
    }
}

// -- JumpPad: valid on both S2C<Chan0> and C2S<Chan0> -----------------

impl Encode<S2C<Chan0>> for JumpPad {
    fn encode(&self, w: &mut WriterCore) {
        encode_jumppad(self, w);
    }
}
impl Encode<C2S<Chan0>> for JumpPad {
    fn encode(&self, w: &mut WriterCore) {
        encode_jumppad(self, w);
    }
}
impl Decode<S2C<Chan0>> for JumpPad {
    fn decode(msg_type: i32, r: &mut ReaderCore) -> Result<Self, ParseError> {
        if msg_type != N_JUMPPAD {
            return Err(ParseError::WrongMessageType {
                expected: N_JUMPPAD,
                expected_name: "JumpPad",
                actual: msg_type,
            });
        }
        decode_jumppad(r).in_packet("JumpPad")
    }
}
impl Decode<C2S<Chan0>> for JumpPad {
    fn decode(msg_type: i32, r: &mut ReaderCore) -> Result<Self, ParseError> {
        if msg_type != N_JUMPPAD {
            return Err(ParseError::WrongMessageType {
                expected: N_JUMPPAD,
                expected_name: "JumpPad",
                actual: msg_type,
            });
        }
        decode_jumppad(r).in_packet("JumpPad")
    }
}

// -- DemoPacket: server only ------------------------------------------

impl Encode<S2C<Chan0>> for DemoPacket {
    fn encode(&self, w: &mut WriterCore) {
        w.put_int(N_DEMOPACKET);
    }
}
impl Decode<S2C<Chan0>> for DemoPacket {
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

// =====================================================================
// ENUMS (for dynamic / iterator-based reading)
// =====================================================================

/// Server -> Client (Channel 0)
#[derive(Debug, Clone, PartialEq)]
pub enum Chan0Server {
    /// `N_DEMOPACKET` Demo playback marker.
    DemoPacket(DemoPacket),
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

// -- From impls -------------------------------------------------------

impl From<Position> for Chan0Server {
    fn from(v: Position) -> Self {
        Self::Position(v)
    }
}
impl From<Teleport> for Chan0Server {
    fn from(v: Teleport) -> Self {
        Self::Teleport(v)
    }
}
impl From<JumpPad> for Chan0Server {
    fn from(v: JumpPad) -> Self {
        Self::JumpPad(v)
    }
}
impl From<DemoPacket> for Chan0Server {
    fn from(v: DemoPacket) -> Self {
        Self::DemoPacket(v)
    }
}

impl From<Position> for Chan0Client {
    fn from(v: Position) -> Self {
        Self::Position(v)
    }
}
impl From<Teleport> for Chan0Client {
    fn from(v: Teleport) -> Self {
        Self::Teleport(v)
    }
}
impl From<JumpPad> for Chan0Client {
    fn from(v: JumpPad) -> Self {
        Self::JumpPad(v)
    }
}

// -- Enum Encode / Decode for S2C<Chan0> (Chan0Server) ----------------

impl Encode<S2C<Chan0>> for Chan0Server {
    fn encode(&self, w: &mut WriterCore) {
        match self {
            Chan0Server::DemoPacket(v) => Encode::<S2C<Chan0>>::encode(v, w),
            Chan0Server::Position(v) => Encode::<S2C<Chan0>>::encode(v, w),
            Chan0Server::Teleport(v) => Encode::<S2C<Chan0>>::encode(v, w),
            Chan0Server::JumpPad(v) => Encode::<S2C<Chan0>>::encode(v, w),
        }
    }
}

impl Decode<S2C<Chan0>> for Chan0Server {
    fn decode(msg_type: i32, r: &mut ReaderCore) -> Result<Self, ParseError> {
        match msg_type {
            N_DEMOPACKET => Ok(Chan0Server::DemoPacket(DemoPacket)),
            N_POS => decode_position(r)
                .map(Chan0Server::Position)
                .in_packet("Position"),
            N_TELEPORT => decode_teleport(r)
                .map(Chan0Server::Teleport)
                .in_packet("Teleport"),
            N_JUMPPAD => decode_jumppad(r)
                .map(Chan0Server::JumpPad)
                .in_packet("JumpPad"),
            _ => Err(ParseError::UnknownMessageType {
                channel: 0,
                id: msg_type,
            }),
        }
    }
}

// -- Enum Encode / Decode for C2S<Chan0> (Chan0Client) ----------------

impl Encode<C2S<Chan0>> for Chan0Client {
    fn encode(&self, w: &mut WriterCore) {
        match self {
            Chan0Client::Position(v) => Encode::<C2S<Chan0>>::encode(v, w),
            Chan0Client::Teleport(v) => Encode::<C2S<Chan0>>::encode(v, w),
            Chan0Client::JumpPad(v) => Encode::<C2S<Chan0>>::encode(v, w),
        }
    }
}

impl Decode<C2S<Chan0>> for Chan0Client {
    fn decode(msg_type: i32, r: &mut ReaderCore) -> Result<Self, ParseError> {
        match msg_type {
            N_POS => decode_position(r)
                .map(Chan0Client::Position)
                .in_packet("Position"),
            N_TELEPORT => decode_teleport(r)
                .map(Chan0Client::Teleport)
                .in_packet("Teleport"),
            N_JUMPPAD => decode_jumppad(r)
                .map(Chan0Client::JumpPad)
                .in_packet("JumpPad"),
            _ => Err(ParseError::UnknownMessageType {
                channel: 0,
                id: msg_type,
            }),
        }
    }
}
