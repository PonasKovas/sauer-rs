pub mod data;
mod error;
pub mod extinfo;
mod internal;
mod messages;
pub mod protocol_route;
mod reader;
mod writer;

pub use error::ParseError;
pub use messages::{channel0::*, channel1::*, channel2::*};
pub use reader::Reader;
pub use writer::Writer;

pub use cube_string;

pub const CHANNEL_COUNT: usize = 3;

pub const CHANNEL_POSITIONS: u8 = 0;
pub const CHANNEL_MESSAGES: u8 = 1;
pub const CHANNEL_MAPS: u8 = 2;

pub const PROTOCOL_VERSION: i32 = 260;
