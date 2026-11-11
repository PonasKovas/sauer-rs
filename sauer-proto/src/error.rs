use num_enum::TryFromPrimitive;
use thiserror::Error;

#[derive(Error, Debug)]
pub enum ParseError {
    #[error("Unexpected end of buffer while reading {expected} bytes")]
    UnexpectedEof { expected: usize },

    #[error("Unknown message type on route {route}: wire id {id}")]
    UnknownMessageType { route: &'static str, id: i32 },

    #[error(
        "Wrong message type: expected wire id {expected} `{expected_name}`, but got wire id {actual} `{actual_name}`"
    )]
    WrongMessageType {
        expected: i32,
        expected_name: &'static str,
        actual: i32,
        actual_name: &'static str,
    },

    #[error("Invalid enum value {value} for type `{enum_name}`")]
    InvalidEnumValue { value: i32, enum_name: &'static str },

    #[error("Message `{message_type}` requires an N_CLIENT envelope, but none was active")]
    MissingClientWrapper { message_type: &'static str },

    #[error("Illegal double client wrapper")]
    DoubleClientWrapper,

    #[error("This packet cant be wrapped in N_CLIENT")]
    BadClientWrapper,

    #[error("Value {value} out of range (max {max})")]
    ValueOutOfRange { value: i32, max: i32 },

    #[error("bad zlib data in an edit block")]
    InvalidZlib,

    #[error("extinfo packet invalid")]
    InvalidExtInfo,

    #[error("extinfo packet version mismatch: {0}")]
    ExtInfoVersionMismatch(i32),

    #[error("unknown extinfo message type {0}")]
    InvalidExtInfoType(i32),

    #[error("while parsing structure `{name}`: {source}")]
    InStruct {
        name: &'static str,
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

// ---------------------------------------------------------------------------
// Error-context helpers -- attach packet / field names to errors for better
// diagnostics while deserializing.
// ---------------------------------------------------------------------------

pub(crate) trait ParseResultExt<T> {
    fn in_struct(self, name: &'static str) -> Result<T, ParseError>;
    fn in_field(self, field: &'static str) -> Result<T, ParseError>;
}

impl<T> ParseResultExt<T> for Result<T, ParseError> {
    fn in_struct(self, name: &'static str) -> Result<T, ParseError> {
        self.map_err(|e| ParseError::InStruct {
            name,
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

pub(crate) trait EnumConvertResultExt<T> {
    fn in_enum(self, enum_name: &'static str) -> Result<T, ParseError>;
}

impl<T: TryFromPrimitive> EnumConvertResultExt<T> for Result<T, num_enum::TryFromPrimitiveError<T>>
where
    T::Primitive: Into<i32>,
{
    fn in_enum(self, enum_name: &'static str) -> Result<T, ParseError> {
        self.map_err(|e| ParseError::InvalidEnumValue {
            value: e.number.into(),
            enum_name,
        })
    }
}
