//! Low-level lookup tables and utility functions for cube string encoding.
//!
//! This module provides the foundational data structures used throughout the crate:
//!
//! - [`CUBE2UNI`]: Maps cube byte values (0-255) to Unicode code points
//! - [`UNI2CUBE_CHARS`]: Maps Unicode code points to cube byte values
//! - [`CUBECTYPE`]: Character classification flags
//! - [`CUBE_LOWER`], [`CUBE_UPPER`]: Case conversion tables
//!
//! The cube character set is a modified Latin-1 encoding used by the Cube engine.
//! Characters 0x00-0x7F match ASCII, while 0x80-0xFF contain extended characters
//! including accented letters and special symbols.
//!
//! # Color Escape Codes
//!
//! Color codes are two-byte sequences starting with the color escape byte ([`COLOR_ESCAPE`] = 0x0C):
//! - `\x0CN` where N is 0-8: Set color (see [`crate::Color`])
//! - `\x0Cs`: Save current color
//! - `\x0Cr`: Restore saved color
//! - `\x0C7`: Reset to white (default)

/// The form feed character (0x0C) used to introduce color codes.
///
/// Color codes in cube strings are two-byte sequences:
/// - `\x0CN` where N is '0'-'8': Set color
/// - `\x0Cs`: Save color
/// - `\x0Cr`: Restore color
pub const COLOR_ESCAPE: u8 = 0x0C;

/// Maps cube byte values to Unicode code points.
///
/// Index by cube byte (0-255) to get the corresponding Unicode code point.
/// Returns 0 for undefined byte values.
pub const CUBE2UNI: [u32; 256] = [
    0, 192, 193, 194, 195, 196, 197, 198, 199, 9, 10, 11, 12, 13, 200, 201, 202, 203, 204, 205,
    206, 207, 209, 210, 211, 212, 213, 214, 216, 217, 218, 219, 32, 33, 34, 35, 36, 37, 38, 39, 40,
    41, 42, 43, 44, 45, 46, 47, 48, 49, 50, 51, 52, 53, 54, 55, 56, 57, 58, 59, 60, 61, 62, 63, 64,
    65, 66, 67, 68, 69, 70, 71, 72, 73, 74, 75, 76, 77, 78, 79, 80, 81, 82, 83, 84, 85, 86, 87, 88,
    89, 90, 91, 92, 93, 94, 95, 96, 97, 98, 99, 100, 101, 102, 103, 104, 105, 106, 107, 108, 109,
    110, 111, 112, 113, 114, 115, 116, 117, 118, 119, 120, 121, 122, 123, 124, 125, 126, 220, 221,
    223, 224, 225, 226, 227, 228, 229, 230, 231, 232, 233, 234, 235, 236, 237, 238, 239, 241, 242,
    243, 244, 245, 246, 248, 249, 250, 251, 252, 253, 255, 0x104, 0x105, 0x106, 0x107, 0x10C,
    0x10D, 0x10E, 0x10F, 0x118, 0x119, 0x11A, 0x11B, 0x11E, 0x11F, 0x130, 0x131, 0x141, 0x142,
    0x143, 0x144, 0x147, 0x148, 0x150, 0x151, 0x152, 0x153, 0x158, 0x159, 0x15A, 0x15B, 0x15E,
    0x15F, 0x160, 0x161, 0x164, 0x165, 0x16E, 0x16F, 0x170, 0x171, 0x178, 0x179, 0x17A, 0x17B,
    0x17C, 0x17D, 0x17E, 0x404, 0x411, 0x413, 0x414, 0x416, 0x417, 0x418, 0x419, 0x41B, 0x41F,
    0x423, 0x424, 0x426, 0x427, 0x428, 0x429, 0x42A, 0x42B, 0x42C, 0x42D, 0x42E, 0x42F, 0x431,
    0x432, 0x433, 0x434, 0x436, 0x437, 0x438, 0x439, 0x43A, 0x43B, 0x43C, 0x43D, 0x43F, 0x442,
    0x444, 0x446, 0x447, 0x448, 0x449, 0x44A, 0x44B, 0x44C, 0x44D, 0x44E, 0x44F, 0x454, 0x490,
    0x491,
];

/// Block offsets for the Unicode-to-cube lookup table.
///
/// Used for two-level lookup: `block = codepoint >> 8`, then
/// `index = offset\[block\] + (codepoint & 0xFF)`.
pub const UNI2CUBE_OFFSETS: [usize; 8] = [0, 256, 658, 658, 512, 658, 658, 658];

/// Maps Unicode code points to cube byte values using two-level lookup.
///
/// Index via: `offset = UNI2CUBE_OFFSETS[codepoint >> 8]`, then
/// `index = offset + (codepoint & 0xFF)`.
/// A value of 0 means the character cannot be represented.
pub const UNI2CUBE_CHARS: &[u8] = &[
    0, 0, 0, 0, 0, 0, 0, 0, 0, 9, 10, 11, 12, 13, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0,
    0, 0, 32, 33, 34, 35, 36, 37, 38, 39, 40, 41, 42, 43, 44, 45, 46, 47, 48, 49, 50, 51, 52, 53,
    54, 55, 56, 57, 58, 59, 60, 61, 62, 63, 64, 65, 66, 67, 68, 69, 70, 71, 72, 73, 74, 75, 76, 77,
    78, 79, 80, 81, 82, 83, 84, 85, 86, 87, 88, 89, 90, 91, 92, 93, 94, 95, 96, 97, 98, 99, 100,
    101, 102, 103, 104, 105, 106, 107, 108, 109, 110, 111, 112, 113, 114, 115, 116, 117, 118, 119,
    120, 121, 122, 123, 124, 125, 126, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0,
    0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0,
    0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0,
    0, 0, 0, 0, 0, 0, 0, 0, 0, 1, 2, 3, 4, 5, 6, 7, 8, 14, 15, 16, 17, 18, 19, 20, 21, 0, 22, 23,
    24, 25, 26, 27, 0, 28, 29, 30, 31, 127, 128, 0, 129, 130, 131, 132, 133, 134, 135, 136, 137,
    138, 139, 140, 141, 142, 143, 144, 145, 0, 146, 147, 148, 149, 150, 151, 0, 152, 153, 154, 155,
    156, 157, 0, 158, 0, 0, 0, 0, 159, 160, 161, 162, 0, 0, 0, 0, 163, 164, 165, 166, 0, 0, 0, 0,
    0, 0, 0, 0, 167, 168, 169, 170, 0, 0, 171, 172, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0,
    173, 174, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 175, 176, 177, 178, 0, 0, 179,
    180, 0, 0, 0, 0, 0, 0, 0, 181, 182, 183, 184, 0, 0, 0, 0, 185, 186, 187, 188, 0, 0, 189, 190,
    191, 192, 0, 0, 193, 194, 0, 0, 0, 0, 0, 0, 0, 0, 195, 196, 197, 198, 0, 0, 0, 0, 0, 0, 199,
    200, 201, 202, 203, 204, 205, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0,
    0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0,
    0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0,
    0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0,
    0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0,
    0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0,
    0, 0, 0, 0, 0, 0, 0, 17, 0, 0, 206, 83, 73, 21, 74, 0, 0, 0, 0, 0, 0, 0, 65, 207, 66, 208, 209,
    69, 210, 211, 212, 213, 75, 214, 77, 72, 79, 215, 80, 67, 84, 216, 217, 88, 218, 219, 220, 221,
    222, 223, 224, 225, 226, 227, 97, 228, 229, 230, 231, 101, 232, 233, 234, 235, 236, 237, 238,
    239, 111, 240, 112, 99, 241, 121, 242, 120, 243, 244, 245, 246, 247, 248, 249, 250, 251, 252,
    0, 141, 0, 0, 253, 115, 105, 145, 106, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0,
    0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0,
    0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 254, 255, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0,
    0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0,
    0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0,
    0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0,
    0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0,
    0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0,
    0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0,
    0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0,
    0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0,
    0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0,
    0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0,
    0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0,
    0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0,
    0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0,
];

/// Lowercase mapping table for cube characters.
///
/// Index by byte to get its lowercase equivalent. Non-letter bytes map to themselves.
pub const CUBE_LOWER: [u8; 256] = [
    0, 130, 131, 132, 133, 134, 135, 136, 137, 9, 10, 11, 12, 13, 138, 139, 140, 141, 142, 143,
    144, 145, 146, 147, 148, 149, 150, 151, 152, 153, 154, 155, 32, 33, 34, 35, 36, 37, 38, 39, 40,
    41, 42, 43, 44, 45, 46, 47, 48, 49, 50, 51, 52, 53, 54, 55, 56, 57, 58, 59, 60, 61, 62, 63, 64,
    97, 98, 99, 100, 101, 102, 103, 104, 105, 106, 107, 108, 109, 110, 111, 112, 113, 114, 115,
    116, 117, 118, 119, 120, 121, 122, 91, 92, 93, 94, 95, 96, 97, 98, 99, 100, 101, 102, 103, 104,
    105, 106, 107, 108, 109, 110, 111, 112, 113, 114, 115, 116, 117, 118, 119, 120, 121, 122, 123,
    124, 125, 126, 156, 157, 129, 130, 131, 132, 133, 134, 135, 136, 137, 138, 139, 140, 141, 142,
    143, 144, 145, 146, 147, 148, 149, 150, 151, 152, 153, 154, 155, 156, 157, 158, 160, 160, 162,
    162, 164, 164, 166, 166, 168, 168, 170, 170, 172, 172, 173, 174, 176, 176, 178, 178, 180, 180,
    182, 182, 184, 184, 186, 186, 188, 188, 190, 190, 192, 192, 194, 194, 196, 196, 198, 198, 158,
    201, 201, 203, 203, 205, 205, 253, 228, 230, 231, 232, 233, 234, 235, 237, 240, 216, 242, 243,
    244, 245, 246, 247, 248, 249, 250, 251, 252, 228, 229, 230, 231, 232, 233, 234, 235, 236, 237,
    238, 239, 240, 241, 242, 243, 244, 245, 246, 247, 248, 249, 250, 251, 252, 253, 255, 255,
];

/// Uppercase mapping table for cube characters.
///
/// Index by byte to get its uppercase equivalent. Non-letter bytes map to themselves.
pub const CUBE_UPPER: [u8; 256] = [
    0, 1, 2, 3, 4, 5, 6, 7, 8, 9, 10, 11, 12, 13, 14, 15, 16, 17, 18, 19, 20, 21, 22, 23, 24, 25,
    26, 27, 28, 29, 30, 31, 32, 33, 34, 35, 36, 37, 38, 39, 40, 41, 42, 43, 44, 45, 46, 47, 48, 49,
    50, 51, 52, 53, 54, 55, 56, 57, 58, 59, 60, 61, 62, 63, 64, 65, 66, 67, 68, 69, 70, 71, 72, 73,
    74, 75, 76, 77, 78, 79, 80, 81, 82, 83, 84, 85, 86, 87, 88, 89, 90, 91, 92, 93, 94, 95, 96, 65,
    66, 67, 68, 69, 70, 71, 72, 73, 74, 75, 76, 77, 78, 79, 80, 81, 82, 83, 84, 85, 86, 87, 88, 89,
    90, 123, 124, 125, 126, 127, 128, 129, 1, 2, 3, 4, 5, 6, 7, 8, 14, 15, 16, 17, 18, 19, 20, 21,
    22, 23, 24, 25, 26, 27, 28, 29, 30, 31, 127, 128, 199, 159, 159, 161, 161, 163, 163, 165, 165,
    167, 167, 169, 169, 171, 171, 173, 73, 175, 175, 177, 177, 179, 179, 181, 181, 183, 183, 185,
    185, 187, 187, 189, 189, 191, 191, 193, 193, 195, 195, 197, 197, 199, 200, 200, 202, 202, 204,
    204, 206, 207, 208, 209, 210, 211, 212, 213, 214, 215, 216, 217, 218, 219, 220, 221, 222, 223,
    224, 225, 226, 227, 207, 229, 208, 209, 210, 211, 212, 213, 236, 214, 238, 239, 215, 241, 217,
    218, 219, 220, 221, 222, 223, 224, 225, 226, 227, 206, 254, 254,
];

/// Character classification flags for cube bytes.
///
/// Each byte has a flags value indicating its character class.
/// Use bitwise AND with flags like [`CTYPE_FLAG_SPACE`] to test.
pub const CUBECTYPE: [u8; 256] = [
    1, 40, 40, 40, 40, 40, 40, 40, 40, 12, 12, 12, 1, 12, 40, 40, 40, 40, 40, 40, 40, 40, 40, 40,
    40, 40, 40, 40, 40, 40, 40, 40, 12, 8, 8, 8, 8, 8, 8, 8, 8, 8, 8, 8, 8, 8, 8, 8, 24, 24, 24,
    24, 24, 24, 24, 24, 24, 24, 8, 8, 8, 8, 8, 8, 8, 40, 40, 40, 40, 40, 40, 40, 40, 40, 40, 40,
    40, 40, 40, 40, 40, 40, 40, 40, 40, 40, 40, 40, 40, 40, 40, 8, 8, 8, 8, 8, 8, 40, 40, 40, 40,
    40, 40, 40, 40, 40, 40, 40, 40, 40, 40, 40, 40, 40, 40, 40, 40, 40, 40, 40, 40, 40, 40, 8, 8,
    8, 8, 40, 40, 40, 40, 40, 40, 40, 40, 40, 40, 40, 40, 40, 40, 40, 40, 40, 40, 40, 40, 40, 40,
    40, 40, 40, 40, 40, 40, 40, 40, 40, 40, 40, 40, 40, 40, 40, 40, 40, 40, 40, 40, 40, 40, 40, 40,
    40, 40, 40, 40, 40, 40, 40, 40, 40, 40, 40, 40, 40, 40, 40, 40, 40, 40, 40, 40, 40, 40, 40, 40,
    40, 40, 40, 40, 40, 40, 40, 40, 40, 40, 40, 40, 40, 40, 40, 40, 40, 40, 40, 40, 40, 40, 40, 40,
    40, 40, 40, 40, 40, 40, 40, 40, 40, 40, 40, 40, 40, 40, 40, 40, 40, 40, 40, 40, 40, 40, 40, 40,
    40, 40, 40, 40, 40, 40, 40, 40, 40, 40, 40,
];

/// Flag indicating whitespace characters.
pub const CTYPE_FLAG_SPACE: u8 = 0x04;
/// Flag indicating printable characters.
pub const CTYPE_FLAG_PRINT: u8 = 0x08;
/// Flag indicating digit characters.
pub const CTYPE_FLAG_DIGIT: u8 = 0x10;
/// Flag indicating alphabetic characters.
pub const CTYPE_FLAG_ALPHA: u8 = 0x20;

/// Converts a cube byte to a Unicode character.
///
/// Returns `Some(char)` if the byte has a valid mapping, or `None` if undefined.
///
/// # Examples
///
/// ```
/// use cube_string::raw::cube2uni;
///
/// assert_eq!(cube2uni(b'A'), Some('A'));
/// assert_eq!(cube2uni(0), None); // 0 is undefined
/// ```
#[inline]
pub const fn cube2uni(byte: u8) -> Option<char> {
    let cp = CUBE2UNI[byte as usize];
    if cp == 0 {
        None
    } else {
        core::char::from_u32(cp)
    }
}

/// Converts a Unicode character to a cube byte.
///
/// Returns `Some(u8)` if the character can be represented in the cube charset,
/// or `None` if there is no mapping.
///
/// ASCII characters (0x00-0x7F) always map to themselves.
///
/// # Examples
///
/// ```
/// use cube_string::raw::uni2cube;
///
/// assert_eq!(uni2cube('A'), Some(b'A'));
/// assert_eq!(uni2cube('!'), Some(b'!'));
/// assert_eq!(uni2cube('🚀'), None); // Rocket emoji cannot be represented
/// ```
#[inline]
pub const fn uni2cube(c: char) -> Option<u8> {
    let cp = c as u32;
    if cp < 128 {
        return Some(cp as u8);
    }
    let block = (cp >> 8) as usize;
    if block >= UNI2CUBE_OFFSETS.len() {
        return None;
    }
    let offset = UNI2CUBE_OFFSETS[block];
    if offset >= UNI2CUBE_CHARS.len() {
        return None;
    }
    let idx = offset + (cp & 0xFF) as usize;
    if idx >= UNI2CUBE_CHARS.len() {
        return None;
    }
    let result = UNI2CUBE_CHARS[idx];
    if result == 0 {
        None
    } else {
        Some(result)
    }
}

/// Tests if a byte is printable in the cube charset.
///
/// Printable characters include all displayable characters except control codes.
/// Whitespace is considered printable.
///
/// # Examples
///
/// ```
/// use cube_string::raw::is_printable;
///
/// assert!(is_printable(b'A'));
/// assert!(is_printable(b' '));
/// assert!(!is_printable(0)); // Null
/// ```
#[inline]
pub const fn is_printable(byte: u8) -> bool {
    CUBECTYPE[byte as usize] & CTYPE_FLAG_PRINT != 0
}

/// Tests if a byte is whitespace in the cube charset.
///
/// Whitespace includes space, tab, newline, carriage return, and form feed.
///
/// # Examples
///
/// ```
/// use cube_string::raw::is_space;
///
/// assert!(is_space(b' '));
/// assert!(is_space(b'\t'));
/// assert!(is_space(b'\n'));
/// assert!(!is_space(b'A'));
/// ```
#[inline]
pub const fn is_space(byte: u8) -> bool {
    CUBECTYPE[byte as usize] & CTYPE_FLAG_SPACE != 0
}

/// Tests if a byte is alphabetic in the cube charset.
///
/// Includes both uppercase and lowercase letters, as well as extended alphabetic characters.
///
/// # Examples
///
/// ```
/// use cube_string::raw::is_alpha;
///
/// assert!(is_alpha(b'A'));
/// assert!(is_alpha(b'z'));
/// assert!(is_alpha(0xE1)); // Latin small letter a with acute
/// assert!(!is_alpha(b'5'));
/// assert!(!is_alpha(b' '));
/// ```
#[inline]
pub const fn is_alpha(byte: u8) -> bool {
    CUBECTYPE[byte as usize] & CTYPE_FLAG_ALPHA != 0
}

/// Tests if a byte is a digit in the cube charset.
///
/// # Examples
///
/// ```
/// use cube_string::raw::is_digit;
///
/// assert!(is_digit(b'0'));
/// assert!(is_digit(b'9'));
/// assert!(!is_digit(b'A'));
/// assert!(!is_digit(b' '));
/// ```
#[inline]
pub const fn is_digit(byte: u8) -> bool {
    CUBECTYPE[byte as usize] & CTYPE_FLAG_DIGIT != 0
}

/// Tests if a byte is alphanumeric (alphabetic or digit).
///
/// # Examples
///
/// ```
/// use cube_string::raw::is_alnum;
///
/// assert!(is_alnum(b'A'));
/// assert!(is_alnum(b'5'));
/// assert!(!is_alnum(b' '));
/// assert!(!is_alnum(b'!'));
/// ```
#[inline]
pub const fn is_alnum(byte: u8) -> bool {
    CUBECTYPE[byte as usize] & (CTYPE_FLAG_ALPHA | CTYPE_FLAG_DIGIT) != 0
}

/// Tests if a byte is lowercase.
///
/// A byte is lowercase if it is alphabetic and not uppercase.
///
/// # Examples
///
/// ```
/// use cube_string::raw::is_lower;
///
/// assert!(is_lower(b'a'));
/// assert!(is_lower(b'z'));
/// assert!(!is_lower(b'A'));
/// assert!(!is_lower(b'5'));
/// ```
#[inline]
pub const fn is_lower(byte: u8) -> bool {
    is_alpha(byte) && CUBE_LOWER[byte as usize] == byte
}

/// Tests if a byte is uppercase.
///
/// A byte is uppercase if it is alphabetic and not lowercase.
///
/// # Examples
///
/// ```
/// use cube_string::raw::is_upper;
///
/// assert!(is_upper(b'A'));
/// assert!(is_upper(b'Z'));
/// assert!(!is_upper(b'a'));
/// assert!(!is_upper(b'5'));
/// ```
#[inline]
pub const fn is_upper(byte: u8) -> bool {
    is_alpha(byte) && CUBE_UPPER[byte as usize] == byte
}

/// Converts a byte to its lowercase equivalent.
///
/// Non-alphabetic bytes are returned unchanged.
///
/// # Examples
///
/// ```
/// use cube_string::raw::to_lower;
///
/// assert_eq!(to_lower(b'A'), b'a');
/// assert_eq!(to_lower(b'Z'), b'z');
/// assert_eq!(to_lower(b'5'), b'5'); // Unchanged
/// ```
#[inline]
pub const fn to_lower(byte: u8) -> u8 {
    CUBE_LOWER[byte as usize]
}

/// Converts a byte to its uppercase equivalent.
///
/// Non-alphabetic bytes are returned unchanged.
///
/// # Examples
///
/// ```
/// use cube_string::raw::to_upper;
///
/// assert_eq!(to_upper(b'a'), b'A');
/// assert_eq!(to_upper(b'z'), b'Z');
/// assert_eq!(to_upper(b'5'), b'5'); // Unchanged
/// ```
#[inline]
pub const fn to_upper(byte: u8) -> u8 {
    CUBE_UPPER[byte as usize]
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_color_escape_value() {
        assert_eq!(COLOR_ESCAPE, 0x0C);
    }

    #[test]
    fn test_cube2uni_basic_ascii() {
        assert_eq!(cube2uni(b'A'), Some('A'));
        assert_eq!(cube2uni(b'Z'), Some('Z'));
        assert_eq!(cube2uni(b'a'), Some('a'));
        assert_eq!(cube2uni(b'z'), Some('z'));
        assert_eq!(cube2uni(b'0'), Some('0'));
        assert_eq!(cube2uni(b'9'), Some('9'));
        assert_eq!(cube2uni(b' '), Some(' '));
    }

    #[test]
    fn test_cube2uni_extended() {
        assert_eq!(cube2uni(0x80), Some('Ý')); // Latin capital letter Y with acute
        assert_eq!(cube2uni(0xFF), Some('ґ')); // Cyrillic small letter ghe with upturn
    }

    #[test]
    fn test_cube2uni_undefined() {
        assert_eq!(cube2uni(0), None);
    }

    #[test]
    fn test_uni2cube_basic_ascii() {
        assert_eq!(uni2cube('A'), Some(b'A'));
        assert_eq!(uni2cube('Z'), Some(b'Z'));
        assert_eq!(uni2cube('0'), Some(b'0'));
        assert_eq!(uni2cube('9'), Some(b'9'));
    }

    #[test]
    fn test_uni2cube_extended() {
        assert_eq!(uni2cube('!'), Some(b'!'));
        assert_eq!(uni2cube('@'), Some(b'@'));
        assert_eq!(uni2cube('~'), Some(b'~'));
    }

    #[test]
    fn test_uni2cube_unrepresentable() {
        assert_eq!(uni2cube('🚀'), None);
        assert_eq!(uni2cube('中'), None);
    }

    #[test]
    fn test_roundtrip_ascii() {
        for byte in 32u8..=126 {
            let c = cube2uni(byte).unwrap();
            assert_eq!(uni2cube(c), Some(byte));
        }
    }

    #[test]
    fn test_is_printable() {
        assert!(is_printable(b'A'));
        assert!(is_printable(b' '));
        assert!(is_printable(b'9'));
        assert!(!is_printable(0));
    }

    #[test]
    fn test_is_space() {
        assert!(is_space(b' '));
        assert!(is_space(b'\t'));
        assert!(is_space(b'\n'));
        assert!(is_space(b'\r'));
        assert!(!is_space(b'A'));
        assert!(!is_space(b'5'));
    }

    #[test]
    fn test_is_alpha() {
        assert!(is_alpha(b'A'));
        assert!(is_alpha(b'z'));
        assert!(!is_alpha(b'5'));
        assert!(!is_alpha(b' '));
    }

    #[test]
    fn test_is_digit() {
        assert!(is_digit(b'0'));
        assert!(is_digit(b'9'));
        assert!(!is_digit(b'A'));
        assert!(!is_digit(b' '));
    }

    #[test]
    fn test_is_alnum() {
        assert!(is_alnum(b'A'));
        assert!(is_alnum(b'5'));
        assert!(!is_alnum(b' '));
        assert!(!is_alnum(b'!'));
    }

    #[test]
    fn test_is_lower() {
        assert!(is_lower(b'a'));
        assert!(is_lower(b'z'));
        assert!(!is_lower(b'A'));
        assert!(!is_lower(b'5'));
    }

    #[test]
    fn test_is_upper() {
        assert!(is_upper(b'A'));
        assert!(is_upper(b'Z'));
        assert!(!is_upper(b'a'));
        assert!(!is_upper(b'5'));
    }

    #[test]
    fn test_to_lower() {
        assert_eq!(to_lower(b'A'), b'a');
        assert_eq!(to_lower(b'Z'), b'z');
        assert_eq!(to_lower(b'5'), b'5'); // Unchanged
        assert_eq!(to_lower(b' '), b' '); // Unchanged
    }

    #[test]
    fn test_to_upper() {
        assert_eq!(to_upper(b'a'), b'A');
        assert_eq!(to_upper(b'z'), b'Z');
        assert_eq!(to_upper(b'5'), b'5'); // Unchanged
        assert_eq!(to_upper(b' '), b' '); // Unchanged
    }

    #[test]
    fn test_ctype_flags() {
        const {
            assert!(CTYPE_FLAG_SPACE < CTYPE_FLAG_PRINT);
        }
        const {
            assert!(CTYPE_FLAG_PRINT < CTYPE_FLAG_DIGIT);
        }
        const {
            assert!(CTYPE_FLAG_DIGIT < CTYPE_FLAG_ALPHA);
        }
    }
}
