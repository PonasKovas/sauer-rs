use crate::{
    Chan0Client, ParseError,
    data::PlayerPosition,
    internal::{Decode, ReaderCore, constants::*},
};

impl Decode for Chan0Client {
    fn decode(msg_type: i32, r: &mut ReaderCore) -> Result<Self, ParseError> {
        match msg_type {
            N_POS => Ok(Chan0Client::Position {
                pos: PlayerPosition::decode(r)?,
            }),
            N_TELEPORT => Ok(Chan0Client::Teleport {
                client_num: r.get_int()?,
                tp: r.get_int()?,
                td: r.get_int()?,
            }),
            N_JUMPPAD => Ok(Chan0Client::JumpPad {
                client_num: r.get_int()?,
                jp: r.get_int()?,
            }),
            _ => Err(ParseError::UnknownMessageType {
                channel: 0,
                id: msg_type,
            }),
        }
    }
}
