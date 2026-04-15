use crate::ParseError;

pub(crate) use reader_core::ReaderCore;
pub(crate) use writer_core::WriterCore;

pub(crate) mod constants;
mod reader_core;
mod writer_core;

// ---------------------------------------------------------------------------
// Internal (crate-private) codec traits for sub-packet data structures.
//
// These are for types like PlayerState, SelInfo, HitInfo, PlayerPosition --
// pieces of data that appear *inside* packets but are not standalone messages.
// ---------------------------------------------------------------------------

pub(crate) trait DecodeCore: Sized {
    fn decode(reader: &mut ReaderCore) -> Result<Self, ParseError>;
}

pub(crate) trait EncodeCore {
    fn encode(&self, writer: &mut WriterCore);
}

// ---------------------------------------------------------------------------
// Error-context helpers -- attach packet / field names to errors for better
// diagnostics while deserializing.
// ---------------------------------------------------------------------------

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
