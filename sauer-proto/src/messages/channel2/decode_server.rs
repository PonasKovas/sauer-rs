use crate::{
    Chan2Server, ParseError,
    internal::{Decode, ParseResultExt, ReaderCore, constants::*},
};

fn packet_name(msg_type: i32) -> &'static str {
    match msg_type {
        N_DEMOPACKET => "DemoPacket",
        N_SENDDEMO => "SendDemo",
        N_SENDMAP => "SendMap",
        _ => "unknown",
    }
}

impl Decode for Chan2Server {
    fn decode(msg_type: i32, r: &mut ReaderCore) -> Result<Self, ParseError> {
        decode_inner(msg_type, r).in_packet(packet_name(msg_type))
    }
}

fn decode_inner(msg_type: i32, r: &mut ReaderCore) -> Result<Chan2Server, ParseError> {
    match msg_type {
        N_DEMOPACKET => Ok(Chan2Server::DemoPacket),
        N_SENDDEMO => {
            let tag = r.get_int()?;
            let data = r.get_bytes(r.remaining())?.to_vec();
            Ok(Chan2Server::SendDemo { tag, data })
        }
        N_SENDMAP => {
            let data = r.get_bytes(r.remaining())?.to_vec();
            Ok(Chan2Server::SendMap { data })
        }
        _ => Err(ParseError::UnknownMessageType {
            channel: 2,
            id: msg_type,
        }),
    }
}
