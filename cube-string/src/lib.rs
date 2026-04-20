#![no_std]

extern crate alloc;

mod builder;
mod color;
mod convenience;
mod error;
#[cfg(feature = "lua")]
mod lua;
mod segments;
mod str;
mod string;

pub mod raw;

pub use builder::CubeStringBuilder;
pub use color::Color;
pub use convenience::IntoCubeStringLossy;
pub use error::CubeStringFromUtf8Error;
pub use segments::{ColorSegment, ColorSegments};
pub use str::CubeStr;
pub use string::CubeString;
