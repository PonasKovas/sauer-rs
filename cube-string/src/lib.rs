#![no_std]

extern crate alloc;

mod builder;
mod error;
mod filter;
pub mod raw;
mod segments;
mod str;
mod string;

pub use builder::CubeStringBuilder;
pub use error::CubeStringFromUtf8Error;
pub use filter::{filter_text, FilterOptions};
use raw::COLOR_ESCAPE;
pub use segments::{ColorSegment, ColorSegments};
pub use str::CubeStr;
pub use string::CubeString;

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum Color {
    Green = 0,
    Blue = 1,
    Yellow = 2,
    Red = 3,
    Gray = 4,
    Magenta = 5,
    Orange = 6,
    White = 7,
    Cyan = 8,
}

impl Color {
    pub fn escape_seq(self) -> [u8; 2] {
        [COLOR_ESCAPE, b'0' + self as u8]
    }
}

impl CubeString {
    pub fn builder() -> CubeStringBuilder {
        CubeStringBuilder::new()
    }
}
