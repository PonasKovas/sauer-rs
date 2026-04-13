//! Error types for cube string conversions.
//!
//! This module provides error types for when conversions between UTF-8 strings
//! and cube-encoded strings fail.

use core::fmt;

/// Error returned when a character cannot be represented in the cube charset.
///
/// This error occurs when attempting to convert a UTF-8 string to a cube-encoded
/// string using [`CubeString::from_str`](crate::CubeString::from_str).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CubeStringFromUtf8Error {
    /// The byte offset in the original string where the unmappable character begins.
    pub byte_offset: usize,
    /// The character that could not be represented.
    pub unmappable_char: char,
}

impl fmt::Display for CubeStringFromUtf8Error {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(
            f,
            "character '{}' at byte {} cannot be represented in cube charset",
            self.unmappable_char, self.byte_offset
        )
    }
}

impl core::error::Error for CubeStringFromUtf8Error {}
