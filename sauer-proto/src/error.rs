use thiserror::Error;

#[derive(Error, Debug)]
pub enum ParseError {
    #[error("Unexpected end of buffer while reading {expected} bytes")]
    UnexpectedEof { expected: usize },

    #[error("Unknown message type (channel {channel}): {id}")]
    UnknownMessageType { channel: usize, id: i32 },

    #[error("Invalid enum value {value} for type {enum_name}")]
    InvalidEnumValue { value: i32, enum_name: &'static str },

    #[error("Message {message_type} requires an N_CLIENT envelope, but none was active")]
    MissingClientWrapper { message_type: &'static str },

    #[error("String parsing error: {0}")]
    InvalidString(String),

    #[error("Illegal double client wrapper")]
    DoubleClientWrapper,
}
