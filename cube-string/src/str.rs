//! An immutable string type for cube engine character encoding.
//!
//! This module provides [`CubeStr`], a borrowed, immutable string type
//! for working with text in the cube engine's character encoding.
//!
//! # Example
//!
//! ```
//! use cube_string::CubeStr;
//!
//! fn process(s: &CubeStr) {
//!     let utf8 = s.to_utf8();
//!     println!("String: {}", utf8);
//! }
//! ```

use alloc::borrow::ToOwned;
use alloc::string::String;
use core::fmt;
use core::hash::Hash;
use core::ops::{
    Deref, Index, Range, RangeFrom, RangeFull, RangeInclusive, RangeTo, RangeToInclusive,
};

use crate::raw::{self, COLOR_ESCAPE};
use crate::{ColorSegment, ColorSegments, CubeString};

/// A borrowed string for cube engine character encoding.
///
/// `CubeStr` is an immutable, borrowed string type that stores text in the
/// cube engine's modified Latin-1 encoding. It is similar to `&str` but
/// for cube-encoded strings.
///
/// This type is zero-cost: it is a thin wrapper around a byte slice
/// (`[u8]`) marked with `#[repr(transparent)]`.
///
/// # Character Encoding
///
/// The cube character set is a modified Latin-1 encoding. Characters 0x00-0x7F
/// match ASCII, while 0x80-0xFF contain extended characters including
/// accented letters and special symbols.
///
/// # Example
///
/// ```
/// use cube_string::{CubeStr, CubeString};
///
/// let s = CubeString::from_str_lossy("Hello");
/// let cube_str: &CubeStr = &s[..];
/// assert_eq!(cube_str.len(), 5);
/// ```
#[derive(
    PartialEq,
    Eq,
    Hash,
    PartialOrd,
    Ord,
    zerocopy::FromBytes,
    zerocopy::KnownLayout,
    zerocopy::Immutable,
)]
#[repr(C)]
pub struct CubeStr([u8]);

impl ToOwned for CubeStr {
    type Owned = CubeString;
    fn to_owned(&self) -> Self::Owned {
        self.to_cube_string()
    }
}

#[cfg(feature = "serde")]
mod serde_impl {
    use serde::{Deserialize, Deserializer, Serialize, Serializer};

    impl Serialize for super::CubeStr {
        fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
        where
            S: Serializer,
        {
            serializer.serialize_bytes(&self.0)
        }
    }

    impl<'de> Deserialize<'de> for &'de super::CubeStr {
        fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
        where
            D: Deserializer<'de>,
        {
            let bytes = <&[u8]>::deserialize(deserializer)?;
            Ok(super::CubeStr::from_bytes(bytes))
        }
    }
}

impl CubeStr {
    pub fn as_bytes(&self) -> &[u8] {
        self
    }

    /// Creates a `CubeStr` from a byte slice.
    pub fn from_bytes(bytes: &[u8]) -> &Self {
        cube_str_from_bytes(bytes)
    }

    pub fn try_from_utf8(s: &str) -> Option<&Self> {
        if s.is_ascii() {
            Some(cube_str_from_bytes(s.as_bytes()))
        } else {
            None
        }
    }

    pub fn to_cube_string(&self) -> crate::CubeString {
        crate::CubeString {
            buf: self.0.to_vec(),
        }
    }

    /// Returns `true` if this string contains only color codes with no actual text.
    ///
    /// A string is considered "empty" if it contains no printable characters,
    /// only color escape sequences.
    #[inline]
    pub fn is_empty_text(&self) -> bool {
        self.text_len() == 0
    }

    /// Returns the length of this string's text, excluding color escape sequences.
    ///
    /// Color escape sequences (`\x0C` followed by a code) are not counted.
    pub fn text_len(&self) -> usize {
        let bytes = &self.0;
        let mut count = 0;
        let mut i = 0;
        while i < bytes.len() {
            if bytes[i] == COLOR_ESCAPE {
                if i + 1 < bytes.len() {
                    i += 2;
                } else {
                    i += 1;
                }
            } else {
                count += 1;
                i += 1;
            }
        }
        count
    }

    /// Converts this string to a UTF-8 string slice, if valid.
    ///
    /// Since cube encoding uses a modified Latin-1 that is not valid UTF-8,
    /// this only returns `Some` if the string contains only ASCII characters.
    /// For UTF-8 conversion, use [`to_utf8`](CubeStr::to_utf8).
    pub fn as_str(&self) -> Option<&str> {
        if self.0.is_ascii() {
            core::str::from_utf8(&self.0).ok()
        } else {
            None
        }
    }

    /// Removes all color escape sequences from this string.
    ///
    /// Returns a new `CubeString` containing only the printable characters
    /// from this string.
    ///
    /// # Example
    ///
    /// ```
    /// use cube_string::{CubeString, Color};
    ///
    /// let mut s = CubeString::from_str_lossy("Hello");
    /// s.push_color(Color::Red);
    /// s.push(b'W');
    ///
    /// let plain = s.strip_colors();
    /// assert_eq!(plain.to_utf8(), "HelloW");
    /// ```
    pub fn strip_colors(&self) -> CubeString {
        let mut result = CubeString::with_capacity(self.len());
        let bytes = &self.0;
        let mut i = 0;
        while i < bytes.len() {
            if bytes[i] == COLOR_ESCAPE {
                if i + 1 < bytes.len() {
                    i += 2;
                    continue;
                } else {
                    break;
                }
            }
            result.push(bytes[i]);
            i += 1;
        }
        result
    }

    /// Filters the text in this string according to the given options.
    ///
    /// See [`FilterOptions`](crate::FilterOptions) for available options.
    pub fn filter(&self, options: crate::FilterOptions) -> CubeString {
        crate::filter::filter_text(&self.0, options)
    }

    /// Returns an iterator over the color segments of this string.
    ///
    /// A color segment is a slice of text with an associated color.
    pub fn color_segments(&self) -> ColorSegments<'_> {
        ColorSegments::new(self)
    }

    /// Converts this string to a UTF-8 `String`.
    ///
    /// Color escape sequences are stripped during conversion.
    /// All printable characters are preserved.
    pub fn to_utf8(&self) -> String {
        let bytes = &self.0;
        let mut result = String::with_capacity(bytes.len());
        let mut i = 0;
        while i < bytes.len() {
            let byte = bytes[i];
            if byte == COLOR_ESCAPE {
                if i + 1 < bytes.len() {
                    i += 2;
                    continue;
                } else {
                    break;
                }
            }
            if let Some(c) = raw::cube2uni(byte) {
                result.push(c);
            }
            i += 1;
        }
        result
    }
}

fn cube_str_from_bytes(bytes: &[u8]) -> &CubeStr {
    zerocopy::transmute_ref!(bytes)
}

impl fmt::Display for CubeStr {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.to_utf8())
    }
}

impl fmt::Debug for CubeStr {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> fmt::Result {
        f.debug_tuple("CubeStr").field(&self.to_utf8()).finish()
    }
}

impl PartialEq<[u8]> for CubeStr {
    fn eq(&self, other: &[u8]) -> bool {
        self.0 == *other
    }
}

impl PartialEq<&str> for CubeStr {
    fn eq(&self, other: &&str) -> bool {
        self.eq(*other)
    }
}

impl PartialEq<str> for CubeStr {
    fn eq(&self, other: &str) -> bool {
        let mut cube_bytes = self.0.iter().copied();

        for c in other.chars() {
            let Some(expected_byte) = crate::raw::uni2cube(c) else {
                return false;
            };
            if cube_bytes.next() != Some(expected_byte) {
                return false;
            }
        }

        cube_bytes.next().is_none()
    }
}

impl PartialEq<CubeStr> for str {
    fn eq(&self, other: &CubeStr) -> bool {
        other == self
    }
}

impl PartialEq<&CubeStr> for str {
    fn eq(&self, other: &&CubeStr) -> bool {
        *other == self
    }
}

impl Index<RangeFull> for CubeStr {
    type Output = CubeStr;
    fn index(&self, _: RangeFull) -> &CubeStr {
        self
    }
}

impl Index<Range<usize>> for CubeStr {
    type Output = CubeStr;
    fn index(&self, index: Range<usize>) -> &CubeStr {
        cube_str_from_bytes(&self.0[index])
    }
}

impl Index<RangeTo<usize>> for CubeStr {
    type Output = CubeStr;
    fn index(&self, index: RangeTo<usize>) -> &CubeStr {
        cube_str_from_bytes(&self.0[index])
    }
}

impl Index<RangeFrom<usize>> for CubeStr {
    type Output = CubeStr;
    fn index(&self, index: RangeFrom<usize>) -> &CubeStr {
        cube_str_from_bytes(&self.0[index.start..])
    }
}

impl Index<RangeInclusive<usize>> for CubeStr {
    type Output = CubeStr;
    fn index(&self, index: RangeInclusive<usize>) -> &CubeStr {
        let start = *index.start();
        let end = *index.end() + 1;
        cube_str_from_bytes(&self.0[start..end])
    }
}

impl Index<RangeToInclusive<usize>> for CubeStr {
    type Output = CubeStr;
    fn index(&self, index: RangeToInclusive<usize>) -> &CubeStr {
        cube_str_from_bytes(&self.0[0..=index.end])
    }
}

impl AsRef<[u8]> for CubeStr {
    fn as_ref(&self) -> &[u8] {
        &self.0
    }
}

impl Deref for CubeStr {
    type Target = [u8];
    fn deref(&self) -> &[u8] {
        &self.0
    }
}

impl AsRef<CubeStr> for CubeStr {
    fn as_ref(&self) -> &CubeStr {
        self
    }
}

impl<'a> IntoIterator for &'a CubeStr {
    type Item = ColorSegment<'a>;
    type IntoIter = ColorSegments<'a>;
    fn into_iter(self) -> Self::IntoIter {
        self.color_segments()
    }
}
