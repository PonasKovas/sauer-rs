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

use core::ops::Deref;

use crate::raw::COLOR_ESCAPE;
use crate::str::CubeStr;
use crate::Color;

fn cube_str_from_bytes(bytes: &[u8]) -> &CubeStr {
    zerocopy::transmute_ref!(bytes)
}

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

/// Decoded meaning of a color-escape sequence (the byte after `COLOR_ESCAPE`).
enum EscapeCode {
    /// `\x0C0`–`\x0C8`: switch to the given color.
    SetColor(Color),
    /// `\x0Cs` (save) or `\x0Cr` (restore): ignored by the segment iterator.
    SaveRestore,
}

/// Decode the byte that follows a `COLOR_ESCAPE`, if it forms a valid escape.
fn parse_escape(code: u8) -> Option<EscapeCode> {
    match code {
        b's' | b'r' => Some(EscapeCode::SaveRestore),
        b'0' => Some(EscapeCode::SetColor(Color::Green)),
        b'1' => Some(EscapeCode::SetColor(Color::Blue)),
        b'2' => Some(EscapeCode::SetColor(Color::Yellow)),
        b'3' => Some(EscapeCode::SetColor(Color::Red)),
        b'4' => Some(EscapeCode::SetColor(Color::Gray)),
        b'5' => Some(EscapeCode::SetColor(Color::Magenta)),
        b'6' => Some(EscapeCode::SetColor(Color::Orange)),
        b'7' => Some(EscapeCode::SetColor(Color::White)),
        b'8' => Some(EscapeCode::SetColor(Color::Cyan)),
        _ => None,
    }
}

/// An iterator over the color segments of a cube string.
pub struct ColorSegments<'a> {
    bytes: &'a [u8],
    pos: usize,
}

impl<'a> ColorSegments<'a> {
    /// Creates a new iterator over the color segments of a cube string.
    pub fn new(cube_str: &'a CubeStr) -> Self {
        Self {
            bytes: cube_str.deref(),
            pos: 0,
        }
    }
}

impl<'a> Iterator for ColorSegments<'a> {
    type Item = ColorSegment<'a>;

    fn next(&mut self) -> Option<Self::Item> {
        let mut current_color = None;

        while self.pos < self.bytes.len() {
            if self.bytes[self.pos] == COLOR_ESCAPE {
                if self.pos + 1 < self.bytes.len() {
                    match parse_escape(self.bytes[self.pos + 1]) {
                        Some(EscapeCode::SetColor(c)) => {
                            current_color = Some(c);
                            self.pos += 2;
                            continue;
                        }
                        Some(EscapeCode::SaveRestore) => {
                            self.pos += 2;
                            continue;
                        }
                        None => {}
                    }
                } else {
                    self.pos += 1;
                    break;
                }
            }
            break;
        }

        if self.pos >= self.bytes.len() {
            return None;
        }

        let text_start = self.pos;

        let mut i = text_start;
        while i < self.bytes.len() && self.bytes[i] != COLOR_ESCAPE {
            i += 1;
        }

        let text_end = i;
        self.pos = text_end;

        if text_start >= self.bytes.len() || text_start == text_end {
            return None;
        }

        let text = &self.bytes[text_start..text_end];

        Some(ColorSegment {
            color: current_color,
            text: cube_str_from_bytes(text),
        })
    }
}
