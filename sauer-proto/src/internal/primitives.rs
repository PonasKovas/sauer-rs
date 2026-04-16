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

// i32 = varint
impl SauerWrite for i32 {
    fn sauer_write(&self, out: &mut Vec<u8>) {
        if (-126..=127).contains(self) {
            out.push(*self as u8);
        } else if (-32768..=32767).contains(self) {
            out.push(0x80);
            out.extend_from_slice(&(*self as i16).to_le_bytes());
        } else {
            out.push(0x81);
            out.extend_from_slice(&self.to_le_bytes());
        }
    }
}
impl SauerRead for i32 {
    fn sauer_read(reader: &mut &[u8]) -> Result<Self, ParseError> {
        let c = u8::sauer_read(reader)? as i8;
        if c == -128 {
            // 0x80
            let s = read_slice(reader, 2)?;
            Ok(i16::from_le_bytes([s[0], s[1]]) as i32)
        } else if c == -127 {
            // 0x81
            let s = read_slice(reader, 4)?;
            Ok(i32::from_le_bytes([s[0], s[1], s[2], s[3]]))
        } else {
            Ok(c as i32)
        }
    }
}

impl SauerWrite for u32 {
    fn sauer_write(&self, out: &mut Vec<u8>) {
        if *self <= 0x7E || *self == 0xFF {
            // 0x7E = 126, 0xFF = 255. (Cube 2 logic shifts negative space for uints)
            out.push(*self as u8);
        } else if *self <= 0xFFFF {
            out.push(0x80);
            out.extend_from_slice(&(*self as u16).to_le_bytes());
        } else {
            out.push(0x81);
            out.extend_from_slice(&self.to_le_bytes());
        }
    }
}
impl SauerRead for u32 {
    fn sauer_read(reader: &mut &[u8]) -> Result<Self, ParseError> {
        let c = u8::sauer_read(reader)?;
        if c == 0x80 {
            let s = read_slice(reader, 2)?;
            Ok(u16::from_le_bytes([s[0], s[1]]) as u32)
        } else if c == 0x81 {
            let s = read_slice(reader, 4)?;
            Ok(u32::from_le_bytes([s[0], s[1], s[2], s[3]]))
        } else {
            Ok(c as u32)
        }
    }
}

impl SauerWrite for f32 {
    fn sauer_write(&self, out: &mut Vec<u8>) {
        out.extend_from_slice(&self.to_le_bytes());
    }
}
impl SauerRead for f32 {
    fn sauer_read(reader: &mut &[u8]) -> Result<Self, ParseError> {
        let s = read_slice(reader, 4)?;
        Ok(f32::from_le_bytes([s[0], s[1], s[2], s[3]]))
    }
}

impl SauerWrite for &CubeStr {
    fn sauer_write(&self, out: &mut Vec<u8>) {
        out.extend_from_slice(self);
        out.push(0);
    }
}
impl SauerWrite for CubeString {
    fn sauer_write(&self, out: &mut Vec<u8>) {
        (&**self).sauer_write(out);
    }
}
impl SauerRead for CubeString {
    fn sauer_read(reader: &mut &[u8]) -> Result<Self, ParseError> {
        let mut string_bytes = Vec::new();
        loop {
            let b = u8::sauer_read(reader)?;
            if b == 0 {
                break;
            }
            string_bytes.push(b);
        }

        Ok(CubeString::from_bytes(string_bytes))
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
    Ok(reader.to_vec())
}
pub(crate) fn write_all_remaining(val: &Vec<u8>, out: &mut Vec<u8>) {
    out.extend_from_slice(val);
}

// u16 length prefixed bytes
pub(crate) fn read_short_pref_data(reader: &mut &[u8]) -> Result<Vec<u8>, ParseError> {
    let l = read_slice(reader, 2)?;
    let len = u16::from_le_bytes([l[0], l[1]]) as usize;

    let data = read_slice(reader, len)?.to_vec();

    Ok(data)
}
pub(crate) fn write_short_pref_data(val: &Vec<u8>, out: &mut Vec<u8>) {
    (val.len() as u8).sauer_write(out);
    ((val.len() >> 8) as u8).sauer_write(out);
    out.extend_from_slice(val);
}

// varint length prefixed bytes
pub(crate) fn read_varint_pref_data(reader: &mut &[u8]) -> Result<Vec<u8>, ParseError> {
    let len = i32::sauer_read(reader)?;
    let len = if len > 0 { len as usize } else { 0 };
    let data = read_slice(reader, len)?.to_vec();

    Ok(data)
}
pub(crate) fn write_varint_pref_data(val: &Vec<u8>, out: &mut Vec<u8>) {
    (val.len() as i32).sauer_write(out);
    out.extend_from_slice(val);
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
