//! Builder pattern for constructing cube strings.
//!
//! This module provides a builder API for constructing `CubeString` values
//! with color formatting.
//!
//! # Example
//!
//! ```
//! use cube_string::{CubeStringBuilder, Color};
//!
//! let s = CubeStringBuilder::new().push_str("Hello").color(Color::Red).push_str("World").build();
//! ```

use alloc::vec::Vec;

use crate::raw::{self, COLOR_ESCAPE};
use crate::{Color, CubeString};

/// A builder for constructing `CubeString` values.
///
/// This builder provides a fluent API for appending text and color codes
/// to a `CubeString`.
pub struct CubeStringBuilder {
    pub(crate) buf: Vec<u8>,
}

impl CubeStringBuilder {
    /// Creates a new builder with an empty buffer.
    pub fn new() -> Self {
        Self { buf: Vec::new() }
    }

    /// Creates a new builder with the given capacity.
    pub fn with_capacity(capacity: usize) -> Self {
        Self {
            buf: Vec::with_capacity(capacity),
        }
    }

    /// Appends raw bytes to the string.
    pub fn push(mut self, bytes: impl AsRef<[u8]>) -> Self {
        self.buf.extend_from_slice(bytes.as_ref());
        self
    }

    /// Appends a UTF-8 string, converting characters to the cube charset.
    ///
    /// Characters that cannot be represented are silently dropped.
    pub fn push_str(mut self, s: &str) -> Self {
        for c in s.chars() {
            if let Some(byte) = raw::uni2cube(c) {
                self.buf.push(byte);
            }
        }
        self
    }

    /// Sets the current color.
    ///
    /// Subsequent text will be displayed in this color.
    pub fn color(mut self, color: Color) -> Self {
        self.buf.push(COLOR_ESCAPE);
        self.buf.push(b'0' + color as u8);
        self
    }

    /// Saves the current color to the color stack.
    ///
    /// The color can be restored later with [`restore_color`](CubeStringBuilder::restore_color).
    pub fn save_color(mut self) -> Self {
        self.buf.push(COLOR_ESCAPE);
        self.buf.push(b's');
        self
    }

    /// Restores the most recently saved color.
    ///
    /// See [`save_color`](CubeStringBuilder::save_color).
    pub fn restore_color(mut self) -> Self {
        self.buf.push(COLOR_ESCAPE);
        self.buf.push(b'r');
        self
    }

    /// Resets the color to the default (white).
    pub fn reset_color(self) -> Self {
        self.color(Color::White)
    }

    /// Consumes the builder and returns the constructed `CubeString`.
    pub fn build(self) -> CubeString {
        CubeString { buf: self.buf }
    }
}

impl Default for CubeStringBuilder {
    fn default() -> Self {
        Self::new()
    }
}
