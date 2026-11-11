use crate::{
    ParseError,
    internal::{SauerRead, SauerWrite, read_slice},
};
use cube_string::{CubeStr, CubeString};

impl<const N: usize, T: SauerWrite> SauerWrite for [T; N] {
    fn sauer_write(&self, out: &mut Vec<u8>) {
        for el in self {
            el.sauer_write(out);
        }
    }
}
impl<const N: usize, T: SauerRead> SauerRead for [T; N] {
    fn sauer_read(reader: &mut &[u8]) -> Result<Self, ParseError> {
        // waiting for array:try_map to stabilize
        let vec: Vec<T> = (0..N)
            .map(|_| T::sauer_read(reader))
            .collect::<Result<_, _>>()?;

        if let Ok(arr) = vec.try_into() {
            Ok(arr)
        } else {
            unreachable!()
        }
    }
}

impl SauerWrite for u8 {
    fn sauer_write(&self, out: &mut Vec<u8>) {
        out.push(*self);
    }
}
impl SauerRead for u8 {
    fn sauer_read(reader: &mut &[u8]) -> Result<Self, ParseError> {
        Ok(read_slice(reader, 1)?[0])
    }
}

impl SauerWrite for bool {
    fn sauer_write(&self, out: &mut Vec<u8>) {
        (if *self { 1i32 } else { 0i32 }).sauer_write(out);
    }
}
impl SauerRead for bool {
    fn sauer_read(reader: &mut &[u8]) -> Result<Self, ParseError> {
        let b = i32::sauer_read(reader)?;
        Ok(b != 0)
    }
}

impl SauerWrite for i32 {
    fn sauer_write(&self, out: &mut Vec<u8>) {
        sauer_core::io::putint(out, *self);
    }
}
impl SauerRead for i32 {
    fn sauer_read(reader: &mut &[u8]) -> Result<Self, ParseError> {
        sauer_core::io::getint(reader).ok_or(ParseError::UnexpectedEof { expected: 1 })
    }
}

impl SauerWrite for u32 {
    fn sauer_write(&self, out: &mut Vec<u8>) {
        sauer_core::io::putuint(out, *self as i32);
    }
}
impl SauerRead for u32 {
    fn sauer_read(reader: &mut &[u8]) -> Result<Self, ParseError> {
        Ok(
            sauer_core::io::getuint(reader).ok_or(ParseError::UnexpectedEof { expected: 1 })?
                as u32,
        )
    }
}

impl SauerWrite for f32 {
    fn sauer_write(&self, out: &mut Vec<u8>) {
        sauer_core::io::putfloat(out, *self);
    }
}
impl SauerRead for f32 {
    fn sauer_read(reader: &mut &[u8]) -> Result<Self, ParseError> {
        sauer_core::io::getfloat(reader).ok_or(ParseError::UnexpectedEof { expected: 4 })
    }
}

impl SauerWrite for &CubeStr {
    fn sauer_write(&self, out: &mut Vec<u8>) {
        sauer_core::io::putstring(out, self);
    }
}
impl SauerWrite for CubeString {
    fn sauer_write(&self, out: &mut Vec<u8>) {
        (&**self).sauer_write(out);
    }
}
impl SauerRead for CubeString {
    fn sauer_read(reader: &mut &[u8]) -> Result<Self, ParseError> {
        sauer_core::io::getstring(reader).ok_or(ParseError::UnexpectedEof { expected: 1 })
    }
}

// DMF quantized float as varint
pub(crate) fn read_varint_float<const Q: u32>(reader: &mut &[u8]) -> Result<f32, ParseError> {
    Ok(i32::sauer_read(reader)? as f32 / Q as f32)
}
pub(crate) fn write_varint_float<const Q: u32>(val: &f32, out: &mut Vec<u8>) {
    ((*val * Q as f32) as i32).sauer_write(out);
}

// SEQUENCES
////////////////////////////

// Bytes without any length prefix or terminator
pub(crate) fn read_all_remaining(reader: &mut &[u8]) -> Result<Vec<u8>, ParseError> {
    let data = reader.to_vec();
    *reader = &[]; // Advance the reader to the end
    Ok(data)
}
pub(crate) fn write_all_remaining(val: &Vec<u8>, out: &mut Vec<u8>) {
    out.extend_from_slice(val);
}

// u16 length prefixed block, the extra data of edit messages
pub(crate) fn read_extra_block<'a>(reader: &mut &'a [u8]) -> Result<&'a [u8], ParseError> {
    let l = read_slice(reader, 2)?;
    let len = u16::from_le_bytes([l[0], l[1]]) as usize;
    read_slice(reader, len)
}
pub(crate) fn write_extra_block(out: &mut Vec<u8>, write: impl FnOnce(&mut Vec<u8>)) {
    let start = out.len();
    out.extend_from_slice(&[0, 0]);
    write(out);
    let len = (out.len() - start - 2) as u16;
    out[start..start + 2].copy_from_slice(&len.to_le_bytes());
}

// A packed vslot alone in an extra block
pub(crate) fn read_vslot_block(reader: &mut &[u8]) -> Result<sauer_core::VSlotProps, ParseError> {
    sauer_core::VSlotProps::sauer_read(&mut read_extra_block(reader)?)
}
pub(crate) fn write_vslot_block(val: &sauer_core::VSlotProps, out: &mut Vec<u8>) {
    write_extra_block(out, |out| val.sauer_write(out));
}

// -1 (or any negative number) terminated sequence
pub(crate) fn read_neg_term_seq<T: SauerRead>(reader: &mut &[u8]) -> Result<Vec<T>, ParseError> {
    let mut vec = Vec::new();

    loop {
        // peek if -1
        let mut temp_reader = *reader;
        if i32::sauer_read(&mut temp_reader)? < 0 {
            *reader = temp_reader;
            break;
        }

        vec.push(T::sauer_read(reader)?);
    }

    Ok(vec)
}
pub(crate) fn write_neg_term_seq<T: SauerWrite>(val: &Vec<T>, out: &mut Vec<u8>) {
    for item in val {
        item.sauer_write(out);
    }
    (-1i32).sauer_write(out);
}

// Nonpositive - 0 (or any negative number) terminated sequence
pub(crate) fn read_nonpos_term_seq<T: SauerRead>(reader: &mut &[u8]) -> Result<Vec<T>, ParseError> {
    let mut vec = Vec::new();

    loop {
        // peek if 0
        let mut temp_reader = *reader;
        if i32::sauer_read(&mut temp_reader)? <= 0 {
            *reader = temp_reader;
            break;
        }

        vec.push(T::sauer_read(reader)?);
    }

    Ok(vec)
}
pub(crate) fn write_nonpos_term_seq<T: SauerWrite>(val: &Vec<T>, out: &mut Vec<u8>) {
    for item in val {
        item.sauer_write(out);
    }
    0i32.sauer_write(out);
}

// varint length prefixed sequence
pub(crate) fn read_varint_pref_seq<const MAX: usize, T: SauerRead>(
    reader: &mut &[u8],
) -> Result<Vec<T>, ParseError> {
    let count = i32::sauer_read(reader)?;
    if count < 0 || count > MAX as i32 {
        return Err(ParseError::ValueOutOfRange {
            value: count,
            max: MAX as i32,
        });
    }

    let mut vec = Vec::with_capacity(count as usize);

    for _ in 0..count {
        vec.push(T::sauer_read(reader)?);
    }

    Ok(vec)
}
pub(crate) fn write_varint_pref_seq<T: SauerWrite>(val: &Vec<T>, out: &mut Vec<u8>) {
    (val.len() as i32).sauer_write(out);
    for item in val {
        item.sauer_write(out);
    }
}
