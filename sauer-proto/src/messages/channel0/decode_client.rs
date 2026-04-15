use super::*;
use crate::{
    Chan0Client, ParseError,
    data::PlayerPosition,
    internal::{Decode, ParseResultExt, ReaderCore, constants::*},
};

fn packet_name(msg_type: i32) -> &'static str {
    match msg_type {
        N_POS => "Position",
        N_TELEPORT => "Teleport",
        N_JUMPPAD => "JumpPad",
        _ => "unknown",
    }
}

impl Decode for Chan0Client {
    fn decode(msg_type: i32, r: &mut ReaderCore) -> Result<Self, ParseError> {
        decode_inner(msg_type, r).in_packet(packet_name(msg_type))
    }
}

fn decode_inner(msg_type: i32, r: &mut ReaderCore) -> Result<Chan0Client, ParseError> {
    match msg_type {
        N_POS => Ok(Chan0Client::Position(Position {
            pos: PlayerPosition::decode(r).in_field("pos")?,
        })),
        N_TELEPORT => Ok(Chan0Client::Teleport(Teleport {
            client_num: r.get_int()?,
            tp: r.get_int()?,
            td: r.get_int()?,
        })),
        N_JUMPPAD => Ok(Chan0Client::JumpPad(JumpPad {
            client_num: r.get_int()?,
            jp: r.get_int()?,
        })),
        _ => Err(ParseError::UnknownMessageType {
            channel: 0,
            id: msg_type,
        }),
    }
}
