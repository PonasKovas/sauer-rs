use cube_string::CubeString;

pub fn putint(out: &mut Vec<u8>, n: i32) {
    if (-126..=127).contains(&n) {
        out.push(n as u8);
    } else if (-32768..=32767).contains(&n) {
        out.push(0x80);
        out.extend_from_slice(&(n as i16).to_le_bytes());
    } else {
        out.push(0x81);
        out.extend_from_slice(&n.to_le_bytes());
    }
}

pub fn getint(reader: &mut &[u8]) -> Option<i32> {
    if reader.is_empty() {
        return None;
    }
    let c = reader[0] as i8;
    *reader = &reader[1..];
    if c == -128 {
        if reader.len() < 2 {
            return None;
        }
        let s = [reader[0], reader[1]];
        *reader = &reader[2..];
        Some(i16::from_le_bytes(s) as i32)
    } else if c == -127 {
        if reader.len() < 4 {
            return None;
        }
        let s = [reader[0], reader[1], reader[2], reader[3]];
        *reader = &reader[4..];
        Some(i32::from_le_bytes(s))
    } else {
        Some(c as i32)
    }
}

pub fn putuint(out: &mut Vec<u8>, n: i32) {
    if n < 0 || n >= (1 << 21) {
        out.push((0x80 | (n & 0x7F)) as u8);
        out.push((0x80 | ((n >> 7) & 0x7F)) as u8);
        out.push((0x80 | ((n >> 14) & 0x7F)) as u8);
        out.push((n >> 21) as u8);
    } else if n < (1 << 7) {
        out.push(n as u8);
    } else if n < (1 << 14) {
        out.push((0x80 | (n & 0x7F)) as u8);
        out.push((n >> 7) as u8);
    } else {
        out.push((0x80 | (n & 0x7F)) as u8);
        out.push((0x80 | ((n >> 7) & 0x7F)) as u8);
        out.push((n >> 14) as u8);
    }
}

pub fn getuint(reader: &mut &[u8]) -> Option<i32> {
    if reader.is_empty() {
        return None;
    }
    let mut n = reader[0] as i32;
    *reader = &reader[1..];
    if n & 0x80 != 0 {
        if reader.is_empty() {
            return None;
        }
        n += ((reader[0] as i32) << 7) - 0x80;
        *reader = &reader[1..];
        if n & (1 << 14) != 0 {
            if reader.is_empty() {
                return None;
            }
            n += ((reader[0] as i32) << 14) - (1 << 14);
            *reader = &reader[1..];
            if n & (1 << 21) != 0 {
                if reader.is_empty() {
                    return None;
                }
                n += ((reader[0] as i32) << 21) - (1 << 21);
                *reader = &reader[1..];
                if n & (1 << 28) != 0 {
                    n |= !0i32 << 28;
                }
            }
        }
    }
    Some(n)
}

pub fn putfloat(out: &mut Vec<u8>, f: f32) {
    out.extend_from_slice(&f.to_le_bytes());
}

pub fn getfloat(reader: &mut &[u8]) -> Option<f32> {
    if reader.len() < 4 {
        return None;
    }
    let s = [reader[0], reader[1], reader[2], reader[3]];
    *reader = &reader[4..];
    Some(f32::from_le_bytes(s))
}

pub fn getstring(reader: &mut &[u8]) -> Option<CubeString> {
    let mut string_bytes = Vec::new();
    loop {
        if reader.is_empty() {
            return None;
        }
        let b = reader[0];
        *reader = &reader[1..];
        if b == 0 {
            break;
        }
        string_bytes.push(b);
    }
    Some(CubeString::from_bytes(string_bytes))
}
