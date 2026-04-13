//! Text filtering utilities for cube strings.
//!
//! This module provides options and functions for filtering and sanitizing
//! text content in cube strings.
//!
//! # Example
//!
//! ```
//! use cube_string::{filter_text, FilterOptions};
//!
//! let input = b"Hello World";
//! let filtered = filter_text(input, FilterOptions::default());
//! ```

use crate::raw::{self, COLOR_ESCAPE};
use crate::CubeString;

/// Options for text filtering.
///
/// These options control how text is processed when filtering.
#[derive(Clone, Debug)]
pub struct FilterOptions {
    /// Whether to remove color escape sequences.
    pub strip_colors: bool,
    /// Whether to preserve whitespace characters as-is.
    pub preserve_whitespace: bool,
    /// Whether to collapse multiple consecutive spaces into one.
    pub collapse_spaces: bool,
    /// Maximum length of the output string, or `None` for no limit.
    pub max_len: Option<usize>,
}

impl Default for FilterOptions {
    fn default() -> Self {
        Self {
            strip_colors: true,
            preserve_whitespace: true,
            collapse_spaces: false,
            max_len: None,
        }
    }
}

impl FilterOptions {
    /// Creates options for strict filtering.
    ///
    /// Removes colors, removes all whitespace, and allows any length.
    pub fn strict() -> Self {
        Self {
            strip_colors: true,
            preserve_whitespace: false,
            collapse_spaces: false,
            max_len: None,
        }
    }

    /// Creates options for filtering chat messages.
    ///
    /// Removes colors, preserves whitespace, collapses multiple spaces,
    /// and allows any length.
    pub fn for_chat() -> Self {
        Self {
            strip_colors: true,
            preserve_whitespace: true,
            collapse_spaces: true,
            max_len: None,
        }
    }

    /// Creates options for filtering descriptions.
    ///
    /// Removes colors, preserves whitespace, does not collapse spaces,
    /// and allows any length.
    pub fn for_description() -> Self {
        Self {
            strip_colors: true,
            preserve_whitespace: true,
            collapse_spaces: false,
            max_len: None,
        }
    }
}

/// Filters text according to the given options.
///
/// This function processes the input bytes and returns a filtered `CubeString`.
pub fn filter_text(input: &[u8], options: FilterOptions) -> CubeString {
    let mut result = CubeString::with_capacity(input.len());
    let mut i = 0;
    let mut last_was_space = false;

    while i < input.len() {
        if let Some(max_len) = options.max_len {
            if result.len() >= max_len {
                break;
            }
        }

        let byte = input[i];

        if byte == COLOR_ESCAPE {
            if options.strip_colors {
                if i + 1 < input.len() {
                    i += 2;
                    continue;
                } else {
                    break;
                }
            } else {
                if let Some(max_len) = options.max_len {
                    if result.len() + 2 > max_len {
                        break;
                    }
                }

                result.push(byte);
                i += 1;
                if i < input.len() {
                    result.push(input[i]);
                    i += 1;
                }
                continue;
            }
        }

        if raw::is_printable(byte) {
            if raw::is_space(byte) {
                if options.collapse_spaces {
                    if !last_was_space {
                        result.push(b' ');
                        last_was_space = true;
                    }
                } else if options.preserve_whitespace {
                    result.push(byte);
                    last_was_space = byte == b' ';
                }
            } else {
                result.push(byte);
                last_was_space = false;
            }
        }
        i += 1;
    }

    result
}
