use crate::{
    Chan0Server, ParseError,
    data::PlayerPosition,
    internal::{Decode, ReaderCore, constants::*},
};

impl Decode for Chan0Server {
    fn decode(msg_type: i32, r: &mut ReaderCore) -> Result<Self, ParseError> {
        match msg_type {
            N_DEMOPACKET => Ok(Chan0Server::DemoPacket),
            N_POS => Ok(Chan0Server::Position {
                pos: PlayerPosition::decode(r)?,
            }),
            N_TELEPORT => Ok(Chan0Server::Teleport {
                client_num: r.get_int()?,
                tp: r.get_int()?,
                td: r.get_int()?,
            }),
            N_JUMPPAD => Ok(Chan0Server::JumpPad {
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
