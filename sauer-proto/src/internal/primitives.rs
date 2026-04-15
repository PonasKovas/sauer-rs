use crate::{
    ParseError,
    internal::{SauerRead, SauerWrite, read_slice},
};
use cube_string::{CubeStr, CubeString};

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
