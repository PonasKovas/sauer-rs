//! Color segment iteration for cube strings.
//!
//! This module provides types for iterating over the color segments of a
//! cube string. Each color segment consists of a color and the text
//! that follows it until the next color change.
//!
//! # Example
//!
//! ```
//! use cube_string::{CubeString, Color};
//!
//! let mut s = CubeString::from_str_lossy("Hello");
//! s.push_color(Color::Red);
//! s.push(b'W');
//!
//! for segment in &s[..] {
//!     let color = segment.color;
//!     let text_len = segment.text.len();
//!     println!("{:?}: {} bytes", color, text_len);
//! }
//! ```

use crate::Color;
use crate::raw::COLOR_ESCAPE;
use crate::str::CubeStr;
use core::ops::Deref;

/// A segment of text with an associated color.
///
/// This represents a contiguous block of text that shares the same color.
#[derive(Debug, Clone)]
pub struct ColorSegment<'a> {
    /// The color of this segment, or `None` for the default color.
    pub color: Option<Color>,
    /// The text content of this segment.
    pub text: &'a CubeStr,
}

impl<'a> ColorSegment<'a> {
    /// Returns the raw bytes of this segment.
    pub fn as_bytes(&self) -> &[u8] {
        self.text.deref()
    }

    /// Returns the text as a UTF-8 string slice, if valid.
    pub fn as_str(&self) -> Option<&str> {
        self.text.as_str()
    }
}

/// The color a `\x0C0`–`\x0C8` escape sets.
fn escape_color(code: u8) -> Option<Color> {
    Some(match code {
        b'0' => Color::Green,
        b'1' => Color::Blue,
        b'2' => Color::Yellow,
        b'3' => Color::Red,
        b'4' => Color::Gray,
        b'5' => Color::Magenta,
        b'6' => Color::Orange,
        b'7' => Color::White,
        b'8' => Color::Cyan,
        _ => return None,
    })
}

/// Depth of the engine's color stack (`colorstack` in `rendertext.cpp`).
const STACK_SIZE: usize = 10;

/// An iterator over the color segments of a cube string.
///
/// Colors carry over from one segment to the next, `\x0Cs` saves the
/// current color and `\x0Cr` restores it, like the engine draws text. Other
/// escapes are skipped.
pub struct ColorSegments<'a> {
    bytes: &'a [u8],
    pos: usize,
    stack: [Option<Color>; STACK_SIZE],
    sp: usize,
}

impl<'a> ColorSegments<'a> {
    /// Creates a new iterator over the color segments of a cube string.
    pub fn new(cube_str: &'a CubeStr) -> Self {
        Self {
            bytes: cube_str.deref(),
            pos: 0,
            stack: [None; STACK_SIZE],
            sp: 0,
        }
    }

    /// Engine `text_color`.
    fn apply_escape(&mut self, code: u8) {
        match code {
            b's' => {
                if self.sp < STACK_SIZE - 1 {
                    self.stack[self.sp + 1] = self.stack[self.sp];
                    self.sp += 1;
                }
            }
            b'r' => self.sp = self.sp.saturating_sub(1),
            _ => {
                if let Some(color) = escape_color(code) {
                    self.stack[self.sp] = Some(color);
                }
            }
        }
    }
}

impl<'a> Iterator for ColorSegments<'a> {
    type Item = ColorSegment<'a>;

    fn next(&mut self) -> Option<Self::Item> {
        loop {
            let rest = &self.bytes[self.pos..];
            match *rest {
                [] => return None,
                // A trailing escape with no code.
                [COLOR_ESCAPE] => {
                    self.pos += 1;
                    return None;
                }
                [COLOR_ESCAPE, code, ..] => {
                    self.apply_escape(code);
                    self.pos += 2;
                }
                _ => {
                    let len = rest
                        .iter()
                        .position(|&b| b == COLOR_ESCAPE)
                        .unwrap_or(rest.len());
                    self.pos += len;
                    return Some(ColorSegment {
                        color: self.stack[self.sp],
                        text: CubeStr::from_bytes(&rest[..len]),
                    });
                }
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use crate::{Color, CubeString};
    use alloc::vec::Vec;

    fn segments(bytes: &[u8]) -> Vec<(Option<Color>, Vec<u8>)> {
        CubeString::from_bytes(bytes.to_vec())
            .color_segments()
            .map(|s| (s.color, s.as_bytes().to_vec()))
            .collect()
    }

    #[test]
    fn colors_carry_over_and_unknown_escapes_are_skipped() {
        assert_eq!(
            segments(b"a\x0c3red\x0csmore\x0cZstill\x0c1blue\x0crred"),
            [
                (None, b"a".to_vec()),
                (Some(Color::Red), b"red".to_vec()),
                (Some(Color::Red), b"more".to_vec()),
                (Some(Color::Red), b"still".to_vec()),
                (Some(Color::Blue), b"blue".to_vec()),
                (Some(Color::Red), b"red".to_vec()),
            ]
        );
    }
}
