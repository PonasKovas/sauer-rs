use crate::ParseError;
use cube_string::CubeString;

pub(crate) struct ReaderCore<'a> {
    buffer: &'a [u8],
    pub cursor: usize,
    active_wrapper: Option<ClientWrapperState>,
}

/// Contains the active state of an N_CLIENT wrapper during deserialization
struct ClientWrapperState {
    client_num: i32,
    end_cursor: usize,
}

impl<'a> ReaderCore<'a> {
    pub fn new(buffer: &'a [u8]) -> Self {
        Self {
            buffer,
            cursor: 0,
            active_wrapper: None,
        }
    }

    pub fn remaining(&self) -> usize {
        self.buffer
            .len()
            .checked_sub(self.cursor)
            .expect("cursor exceeded buffer len")
    }

    pub fn peek(&self, n: usize) -> Option<&[u8]> {
        if self.remaining() >= n {
            Some(&self.buffer[self.cursor..self.cursor + n])
        } else {
            None
        }
    }

    pub fn get_byte(&mut self) -> Result<u8, ParseError> {
        if self.remaining() == 0 {
            return Err(ParseError::UnexpectedEof { expected: 1 });
        }
        let b = self.buffer[self.cursor];
        self.cursor += 1;
        Ok(b)
    }

    pub fn get_bytes(&mut self, len: usize) -> Result<&'a [u8], ParseError> {
        if self.remaining() < len {
            return Err(ParseError::UnexpectedEof { expected: len });
        }
        let slice = &self.buffer[self.cursor..self.cursor + len];
        self.cursor += len;
        Ok(slice)
    }

    /// Sauerbraten variable-length integer compression
    pub fn get_int(&mut self) -> Result<i32, ParseError> {
        let c = self.get_byte()? as i8;
        if c == -128 {
            // 0x80
            let a = self.get_byte()? as i32;
            let b = self.get_byte()? as i32;
            Ok(a | (b << 8))
        } else if c == -127 {
            // 0x81
            let a = self.get_byte()? as i32;
            let b = self.get_byte()? as i32;
            let c_byte = self.get_byte()? as i32;
            let d = self.get_byte()? as i32;
            Ok(a | (b << 8) | (c_byte << 16) | (d << 24))
        } else {
            Ok(c as i32)
        }
    }

    pub fn get_uint(&mut self) -> Result<u32, ParseError> {
        let c = self.get_byte()?;
        if c == 0x80 {
            let a = self.get_byte()? as u32;
            let b = self.get_byte()? as u32;
            Ok(a | (b << 8))
        } else if c == 0x81 {
            let a = self.get_byte()? as u32;
            let b = self.get_byte()? as u32;
            let c_byte = self.get_byte()? as u32;
            let d = self.get_byte()? as u32;
            Ok(a | (b << 8) | (c_byte << 16) | (d << 24))
        } else {
            Ok(c as u32)
        }
    }

    pub fn get_float(&mut self) -> Result<f32, ParseError> {
        let mut buf = [0u8; 4];
        buf.copy_from_slice(self.get_bytes(4)?);
        Ok(f32::from_le_bytes(buf))
    }

    pub fn get_string(&mut self) -> Result<CubeString, ParseError> {
        let mut string_bytes = Vec::new();
        loop {
            let b = self.get_byte()?;
            if b == 0 {
                break;
            }
            string_bytes.push(b);
        }

        Ok(CubeString::from_bytes(string_bytes))
    }

    pub fn active_client_num(&self, msg_name: &'static str) -> Result<i32, ParseError> {
        self.active_wrapper
            .as_ref()
            .filter(|w| self.cursor <= w.end_cursor)
            .map(|w| w.client_num)
            .ok_or(ParseError::MissingClientWrapper {
                message_type: msg_name,
            })
    }
    pub fn set_client_num(&mut self, client_num: i32, length: usize) {
        self.active_wrapper = Some(ClientWrapperState {
            client_num,
            end_cursor: self.cursor + length,
        });
    }
}
