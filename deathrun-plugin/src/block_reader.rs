use flate2::read::ZlibDecoder;
use std::io::Read;

fn getint(bytes: &[u8], pos: &mut usize) -> Option<i32> {
    if *pos >= bytes.len() {
        return None;
    }
    let c = bytes[*pos] as i8;
    *pos += 1;
    if c == -128 {
        if *pos + 1 >= bytes.len() {
            return None;
        }
        let b0 = bytes[*pos] as u8 as i32;
        let b1 = bytes[*pos + 1] as i8 as i32;
        *pos += 2;
        Some(b0 | (b1 << 8))
    } else if c == -127 {
        if *pos + 3 >= bytes.len() {
            return None;
        }
        let b0 = bytes[*pos] as u8 as i32;
        let b1 = bytes[*pos + 1] as u8 as i32;
        let b2 = bytes[*pos + 2] as u8 as i32;
        let b3 = bytes[*pos + 3] as i8 as i32;
        *pos += 4;
        Some(b0 | (b1 << 8) | (b2 << 16) | (b3 << 24))
    } else {
        Some(c as i32)
    }
}

fn skip_undo_ents(bytes: &[u8], pos: &mut usize) -> Option<()> {
    let numents = getint(bytes, pos)?;
    for _ in 0..numents {
        getint(bytes, pos)?; // id
        getint(bytes, pos)?; // type
        getint(bytes, pos)?; // x
        getint(bytes, pos)?; // y
        getint(bytes, pos)?; // z
        for _ in 0..5 {
            getint(bytes, pos)?; // attr 0-4
        }
    }
    Some(())
}

pub fn get_undo_redo_bounds(data: &[u8]) -> Option<([i32; 3], [i32; 3], i32)> {
    let mut decoder = ZlibDecoder::new(data);
    let mut decompressed = Vec::new();
    decoder
        .by_ref()
        .take(4096)
        .read_to_end(&mut decompressed)
        .ok()?;

    let mut pos = 0;
    skip_undo_ents(&decompressed, &mut pos)?;

    if pos >= decompressed.len() {
        return None;
    }

    let x = getint(&decompressed, &mut pos)?;
    let y = getint(&decompressed, &mut pos)?;
    let z = getint(&decompressed, &mut pos)?;
    let xs = getint(&decompressed, &mut pos)?;
    let ys = getint(&decompressed, &mut pos)?;
    let zs = getint(&decompressed, &mut pos)?;
    let grid = getint(&decompressed, &mut pos)?;

    Some(([x, y, z], [xs, ys, zs], grid))
}

pub fn get_clipboard_bounds(data: &[u8]) -> Option<([i32; 3], [i32; 3], i32)> {
    let mut decoder = ZlibDecoder::new(data);
    let mut decompressed = Vec::new();
    decoder
        .by_ref()
        .take(256)
        .read_to_end(&mut decompressed)
        .ok()?;

    let mut pos = 0;
    let x = getint(&decompressed, &mut pos)?;
    let y = getint(&decompressed, &mut pos)?;
    let z = getint(&decompressed, &mut pos)?;
    let xs = getint(&decompressed, &mut pos)?;
    let ys = getint(&decompressed, &mut pos)?;
    let zs = getint(&decompressed, &mut pos)?;
    let grid = getint(&decompressed, &mut pos)?;

    Some(([x, y, z], [xs, ys, zs], grid))
}
