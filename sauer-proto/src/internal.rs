use crate::ParseError;

pub(crate) use reader_core::ReaderCore;
pub(crate) use writer_core::WriterCore;

pub(crate) mod constants;
mod reader_core;
mod writer_core;

pub(crate) trait Decode: Sized {
    fn decode(msg_type: i32, reader: &mut ReaderCore) -> Result<Self, ParseError>;
}

pub(crate) trait Encode {
    fn encode(&self, writer: &mut WriterCore);
    /// Returns Some(client_num) if this message must be inside an N_CLIENT envelope
    ///
    /// Only for the top level enums
    fn requires_client_wrapper(&self) -> Option<i32> {
        None
    }
}

pub(crate) trait ParseResultExt<T> {
    fn in_packet(self, packet: &'static str) -> Result<T, ParseError>;
    fn in_field(self, field: &'static str) -> Result<T, ParseError>;
}

impl<T> ParseResultExt<T> for Result<T, ParseError> {
    fn in_packet(self, packet: &'static str) -> Result<T, ParseError> {
        self.map_err(|e| ParseError::InPacket {
            packet,
            source: Box::new(e),
        })
    }
    fn in_field(self, field: &'static str) -> Result<T, ParseError> {
        self.map_err(|e| ParseError::InField {
            field,
            source: Box::new(e),
        })
    }
}
