use cube_string::CubeStr;

pub(crate) struct WriterCore {
    pub(crate) buffer: Vec<u8>,
}

impl WriterCore {
    pub fn new() -> Self {
        Self { buffer: Vec::new() }
    }

    pub fn put_byte(&mut self, b: u8) {
        self.buffer.push(b);
    }

    pub fn put_bytes(&mut self, bytes: &[u8]) {
        self.buffer.extend_from_slice(bytes);
    }

    /// Sauerbraten variable-length integer compression
    pub fn put_int(&mut self, n: i32) {
        if (-126..=127).contains(&n) {
            self.put_byte(n as u8);
        } else if (-32768..=32767).contains(&n) {
            self.put_byte(0x80);
            self.put_byte((n & 0xFF) as u8);
            self.put_byte(((n >> 8) & 0xFF) as u8);
        } else {
            self.put_byte(0x81);
            self.put_byte((n & 0xFF) as u8);
            self.put_byte(((n >> 8) & 0xFF) as u8);
            self.put_byte(((n >> 16) & 0xFF) as u8);
            self.put_byte(((n >> 24) & 0xFF) as u8);
        }
    }

    pub fn put_uint(&mut self, n: u32) {
        if n <= 0x7E || n == 0xFF {
            // 0x7E = 126, 0xFF = 255. (Cube 2 logic shifts negative space for uints)
            self.put_byte(n as u8);
        } else if n <= 0xFFFF {
            self.put_byte(0x80);
            self.put_byte((n & 0xFF) as u8);
            self.put_byte(((n >> 8) & 0xFF) as u8);
        } else {
            self.put_byte(0x81);
            self.put_byte((n & 0xFF) as u8);
            self.put_byte(((n >> 8) & 0xFF) as u8);
            self.put_byte(((n >> 16) & 0xFF) as u8);
            self.put_byte(((n >> 24) & 0xFF) as u8);
        }
    }

    pub fn put_float(&mut self, f: f32) {
        self.put_bytes(&f.to_le_bytes());
    }

    pub fn put_string(&mut self, s: &CubeStr) {
        self.put_bytes(s.as_bytes());
        self.put_byte(0); // Null terminator
    }
}
