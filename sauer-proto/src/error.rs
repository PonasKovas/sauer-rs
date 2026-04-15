use thiserror::Error;

#[derive(Error, Debug)]
pub enum ParseError {
    #[error("Unexpected end of buffer while reading {expected} bytes")]
    UnexpectedEof { expected: usize },

    #[error("Unknown message type on channel {channel}: wire id {id}")]
    UnknownMessageType { channel: usize, id: i32 },

    #[error("Wrong message type: expected wire id {expected} for `{expected_name}`, but got wire id {actual}")]
    WrongMessageType {
        expected: i32,
        expected_name: &'static str,
        actual: i32,
    },

    #[error("Invalid enum value {value} for type `{enum_name}`")]
    InvalidEnumValue { value: i32, enum_name: &'static str },

    #[error("Message `{message_type}` requires an N_CLIENT envelope, but none was active")]
    MissingClientWrapper { message_type: &'static str },

    #[error("String parsing error: {0}")]
    InvalidString(String),

    #[error("Illegal double client wrapper")]
    DoubleClientWrapper,

    #[error("Value {value} out of range for `{field}` (max {max})")]
    ValueOutOfRange {
        field: &'static str,
        value: i32,
        max: i32,
    },

    #[error("extinfo packet invalid")]
    InvalidExtInfo,

    #[error("extinfo packet version mismatch: {0}")]
    ExtInfoVersionMismatch(i32),

    #[error("unknown extinfo message type {0}")]
    InvalidExtInfoType(i32),

    #[error("while parsing packet `{packet}`: {source}")]
    InPacket {
        packet: &'static str,
        #[source]
        source: Box<ParseError>,
    },

    #[error("while parsing field `{field}`: {source}")]
    InField {
        field: &'static str,
        #[source]
        source: Box<ParseError>,
    },
}
