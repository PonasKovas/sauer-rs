#![no_std]

extern crate alloc;

mod builder;
mod color;
mod convenience;
mod error;
mod filter;
pub mod raw;
mod segments;
mod str;
mod string;

pub use builder::CubeStringBuilder;
pub use color::Color;
pub use convenience::IntoCubeStringLossy;
pub use error::CubeStringFromUtf8Error;
pub use filter::{filter_text, FilterOptions};
pub use segments::{ColorSegment, ColorSegments};
pub use str::CubeStr;
pub use string::CubeString;
