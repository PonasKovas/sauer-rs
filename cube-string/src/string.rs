//! A growable string type for cube engine strings.
//!
//! This module provides [`CubeString`], a mutable, growable string type
//! for storing text in the cube engine's character encoding.
//!
//! # Example
//!
//! ```
//! use cube_string::{CubeString, Color};
//!
//! let mut s = CubeString::from_str_lossy("Hello");
//! s.push_color(Color::Red);
//! s.push(b'W');
//! ```

use alloc::string::String;
use alloc::vec::Vec;
use core::borrow::Borrow;
use core::fmt;
use core::hash::Hash;
use core::ops::{
    Deref, Index, Range, RangeFrom, RangeFull, RangeInclusive, RangeTo, RangeToInclusive,
};

use crate::raw::{self, COLOR_ESCAPE};
use crate::str::CubeStr;
use crate::{CubeStringBuilder, CubeStringFromUtf8Error};

fn cube_str_from_bytes(bytes: &[u8]) -> &CubeStr {
    zerocopy::transmute_ref!(bytes)
}

/// A mutable string for cube engine character encoding.
///
/// `CubeString` provides a growable string type that stores text in the cube
/// engine's modified Latin-1 encoding. It supports color escape codes
/// for text coloring.
///
/// # Character Encoding
///
/// The cube character set is based on Latin-1 (ISO-8859-1) with some
/// modifications. Characters 0x00-0x7F match ASCII, while 0x80-0xFF
/// contain extended characters including accented letters and special symbols.
///
/// # Color Codes
///
/// Color codes are inserted using [`push_color`](CubeString::push_color):
/// - `\x0CN` where N is '0'-'8': Set color
/// - `\x0Cs`: Save color
/// - `\x0Cr`: Restore color
///
/// # Example
///
/// ```
/// use cube_string::{CubeString, Color};
///
/// let mut s = CubeString::from_str_lossy("Hello");
/// s.push_color(Color::Red);
/// s.push(b' ');
/// ```
#[derive(Clone, PartialEq, Eq, PartialOrd, Ord, Hash, Default)]
pub struct CubeString {
    pub(crate) buf: Vec<u8>,
}

#[cfg(feature = "serde")]
mod serde_impl {
    use alloc::vec::Vec;
    use serde::{Deserialize, Deserializer, Serialize, Serializer};

    impl Serialize for super::CubeString {
        fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
        where
            S: Serializer,
        {
            serializer.serialize_bytes(&self.buf)
        }
    }

    impl<'de> Deserialize<'de> for super::CubeString {
        fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
        where
            D: Deserializer<'de>,
        {
            let bytes = Vec::<u8>::deserialize(deserializer)?;
            Ok(super::CubeString { buf: bytes })
        }
    }
}

impl CubeString {
    /// Returns a builder for CubeString with colors
    pub fn builder() -> CubeStringBuilder {
        CubeStringBuilder::new()
    }
    /// Creates a new empty `CubeString`.
    ///
    /// # Example
    ///
    /// ```
    /// use cube_string::CubeString;
    ///
    /// let s = CubeString::new();
    /// assert!(s.is_empty());
    /// ```
    pub fn new() -> Self {
        Self { buf: Vec::new() }
    }

    /// Creates a new empty `CubeString` with the given capacity.
    ///
    /// The string will be able to hold exactly `capacity` bytes without
    /// reallocating.
    pub fn with_capacity(capacity: usize) -> Self {
        Self {
            buf: Vec::with_capacity(capacity),
        }
    }

    /// Creates a `CubeString` from a UTF-8 string.
    ///
    /// Characters that cannot be represented in the cube character set
    /// are silently dropped.
    ///
    /// To get an error instead, use [`from_str`](CubeString::from_str).
    ///
    /// # Example
    ///
    /// ```
    /// use cube_string::CubeString;
    ///
    /// let s = CubeString::from_str_lossy("Hello, world!");
    /// ```
    #[allow(clippy::should_implement_trait)]
    pub fn from_str_lossy(s: &str) -> Self {
        let mut result = Self::with_capacity(s.len());
        for c in s.chars() {
            if let Some(byte) = raw::uni2cube(c) {
                result.push(byte);
            }
        }
        result
    }

    /// Creates a `CubeString` from a UTF-8 string, returning an error
    /// if any character cannot be represented.
    ///
    /// # Example
    ///
    /// ```
    /// use cube_string::CubeString;
    ///
    /// let result = CubeString::from_str("Hello");
    /// assert!(result.is_ok());
    /// ```
    #[allow(clippy::should_implement_trait)]
    pub fn from_str(s: &str) -> Result<Self, CubeStringFromUtf8Error> {
        let mut result = Self::with_capacity(s.len());
        let mut byte_offset = 0;
        for c in s.chars() {
            if let Some(byte) = raw::uni2cube(c) {
                result.push(byte);
            } else {
                return Err(CubeStringFromUtf8Error {
                    byte_offset,
                    unmappable_char: c,
                });
            }
            byte_offset += c.len_utf8();
        }
        Ok(result)
    }

    /// Creates a `CubeString` from raw bytes.
    ///
    /// No validation is performed on the bytes.
    pub fn from_bytes(bytes: Vec<u8>) -> Self {
        Self { buf: bytes }
    }

    /// Appends a byte to the end of this string.
    pub fn push(&mut self, byte: u8) {
        self.buf.push(byte);
    }

    /// Appends a UTF-8 string, converting characters to the cube charset.
    ///
    /// Characters that cannot be represented are silently dropped.
    pub fn push_str(&mut self, s: &str) {
        for c in s.chars() {
            if let Some(byte) = raw::uni2cube(c) {
                self.buf.push(byte);
            }
        }
    }

    /// Appends a color escape sequence to the end of this string.
    ///
    /// The color will affect subsequent text until another color
    /// is set or the string ends.
    ///
    /// # Example
    ///
    /// ```
    /// use cube_string::{CubeString, Color};
    ///
    /// let mut s = CubeString::from_str_lossy("Error: ");
    /// s.push_color(Color::Red);
    /// s.push_str("file not found");
    /// ```
    pub fn push_color(&mut self, color: crate::Color) {
        self.buf.push(COLOR_ESCAPE);
        self.buf.push(b'0' + color as u8);
    }

    fn as_cube_str(&self) -> &CubeStr {
        cube_str_from_bytes(&self.buf)
    }
}

impl Deref for CubeString {
    type Target = CubeStr;
    fn deref(&self) -> &CubeStr {
        self.as_cube_str()
    }
}

impl AsRef<[u8]> for CubeString {
    fn as_ref(&self) -> &[u8] {
        &self.buf
    }
}

impl AsRef<CubeStr> for CubeString {
    fn as_ref(&self) -> &CubeStr {
        self.as_cube_str()
    }
}

impl From<Vec<u8>> for CubeString {
    fn from(buf: Vec<u8>) -> Self {
        Self { buf }
    }
}

impl From<CubeString> for Vec<u8> {
    fn from(cs: CubeString) -> Self {
        cs.buf
    }
}

impl From<&CubeStr> for CubeString {
    fn from(s: &CubeStr) -> Self {
        Self {
            buf: s.deref().to_vec(),
        }
    }
}

impl TryFrom<&str> for CubeString {
    type Error = CubeStringFromUtf8Error;
    fn try_from(s: &str) -> Result<Self, Self::Error> {
        Self::from_str(s)
    }
}

impl TryFrom<String> for CubeString {
    type Error = CubeStringFromUtf8Error;
    fn try_from(s: String) -> Result<Self, Self::Error> {
        Self::from_str(&s)
    }
}

impl PartialEq<CubeStr> for CubeString {
    fn eq(&self, other: &CubeStr) -> bool {
        self.buf == **other
    }
}

impl PartialEq<&CubeStr> for CubeString {
    fn eq(&self, other: &&CubeStr) -> bool {
        self.buf == ***other
    }
}

impl PartialEq<CubeString> for CubeStr {
    fn eq(&self, other: &CubeString) -> bool {
        **self == other.buf
    }
}

impl PartialEq<CubeString> for &CubeStr {
    fn eq(&self, other: &CubeString) -> bool {
        ***self == other.buf
    }
}

impl PartialEq<str> for CubeString {
    fn eq(&self, other: &str) -> bool {
        **self == other
    }
}

impl PartialEq<&str> for CubeString {
    fn eq(&self, other: &&str) -> bool {
        **self == *other
    }
}

impl PartialEq<CubeString> for str {
    fn eq(&self, other: &CubeString) -> bool {
        **other == self
    }
}

impl PartialEq<CubeString> for &str {
    fn eq(&self, other: &CubeString) -> bool {
        **other == *self
    }
}

impl PartialEq<&CubeString> for str {
    fn eq(&self, other: &&CubeString) -> bool {
        self == *other
    }
}

impl PartialEq<CubeString> for &&str {
    fn eq(&self, other: &CubeString) -> bool {
        *self == other
    }
}

impl fmt::Display for CubeString {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        self.as_cube_str().fmt(f)
    }
}

impl fmt::Debug for CubeString {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        self.as_cube_str().fmt(f)
    }
}

impl Extend<u8> for CubeString {
    fn extend<T: IntoIterator<Item = u8>>(&mut self, iter: T) {
        self.buf.extend(iter);
    }
}

impl<'a> Extend<&'a CubeStr> for CubeString {
    fn extend<T: IntoIterator<Item = &'a CubeStr>>(&mut self, iter: T) {
        for s in iter {
            self.buf.extend_from_slice(s.deref());
        }
    }
}

impl Borrow<CubeStr> for CubeString {
    fn borrow(&self) -> &CubeStr {
        self.as_cube_str()
    }
}

impl Index<RangeFull> for CubeString {
    type Output = CubeStr;
    fn index(&self, _: RangeFull) -> &CubeStr {
        self.as_cube_str()
    }
}

impl Index<Range<usize>> for CubeString {
    type Output = CubeStr;
    fn index(&self, index: Range<usize>) -> &CubeStr {
        cube_str_from_bytes(&self.buf[index])
    }
}

impl Index<RangeTo<usize>> for CubeString {
    type Output = CubeStr;
    fn index(&self, index: RangeTo<usize>) -> &CubeStr {
        cube_str_from_bytes(&self.buf[index])
    }
}

impl Index<RangeFrom<usize>> for CubeString {
    type Output = CubeStr;
    fn index(&self, index: RangeFrom<usize>) -> &CubeStr {
        cube_str_from_bytes(&self.buf[index.start..])
    }
}

impl Index<RangeInclusive<usize>> for CubeString {
    type Output = CubeStr;
    fn index(&self, index: RangeInclusive<usize>) -> &CubeStr {
        let start = *index.start();
        let end = *index.end() + 1;
        cube_str_from_bytes(&self.buf[start..end])
    }
}

impl Index<RangeToInclusive<usize>> for CubeString {
    type Output = CubeStr;
    fn index(&self, index: RangeToInclusive<usize>) -> &CubeStr {
        cube_str_from_bytes(&self.buf[0..=index.end])
    }
}
