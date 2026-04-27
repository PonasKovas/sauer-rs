use flate2::read::ZlibDecoder;
use sauer_proto::data::{Orient, SelInfo, Vec3};
use std::io::Read;

pub fn unpack_cube(bytes: &[u8], pos: &mut usize) -> Option<sauer_map::Node> {
    if *pos >= bytes.len() {
        return None;
    }
    let mat = bytes[*pos];
    *pos += 1;
    if mat == 0xFF {
        let mut children = Vec::with_capacity(8);
        for _ in 0..8 {
            children.push(unpack_cube(bytes, pos)?);
        }
        Some(sauer_map::Node::Branch(Box::new(
            children.try_into().unwrap(),
        )))
    } else {
        if *pos >= bytes.len() {
            return None;
        }
        let mat_hi = bytes[*pos];
        *pos += 1;
        let material = (mat as u16) | ((mat_hi as u16) << 8);

        let mut edges = [0u8; 12];
        if *pos + 12 > bytes.len() {
            return None;
        }
        edges.copy_from_slice(&bytes[*pos..*pos + 12]);
        *pos += 12;

        let mut textures = [0u16; 6];
        for i in 0..6 {
            if *pos + 2 > bytes.len() {
                return None;
            }
            textures[i] = u16::from_le_bytes([bytes[*pos], bytes[*pos + 1]]);
            *pos += 2;
        }

        Some(sauer_map::Node::Leaf(sauer_map::Cube {
            edges,
            textures,
            material: ogz_rs::data::Material::from_bits(material),
        }))
    }
}

pub fn unpack_block(bytes: &[u8], pos: &mut usize) -> Option<(SelInfo, Vec<sauer_map::Node>)> {
    if *pos + 32 > bytes.len() {
        return None;
    }
    let mut get_i32 = |offset: usize| {
        let start = *pos + offset * 4;
        i32::from_le_bytes(bytes[start..start + 4].try_into().unwrap())
    };

    let o_x = get_i32(0);
    let o_y = get_i32(1);
    let o_z = get_i32(2);
    let s_x = get_i32(3);
    let s_y = get_i32(4);
    let s_z = get_i32(5);
    let grid = get_i32(6);
    let orient = get_i32(7);
    *pos += 32;

    let total = (s_x * s_y * s_z) as usize;
    let mut nodes = Vec::with_capacity(total);
    for _ in 0..total {
        nodes.push(unpack_cube(bytes, pos)?);
    }

    let sel = SelInfo {
        o: Vec3 {
            x: o_x,
            y: o_y,
            z: o_z,
        },
        s: Vec3 {
            x: s_x,
            y: s_y,
            z: s_z,
        },
        grid,
        orient: Orient::try_from(orient).unwrap_or(Orient::Left),
        cx: 0,
        cxs: 0,
        cy: 0,
        cys: 0,
        corner: 0,
    };

    Some((sel, nodes))
}

pub fn decode_clipboard(data: &[u8]) -> Option<(SelInfo, Vec<sauer_map::Node>)> {
    let mut decoder = ZlibDecoder::new(data);
    let mut decompressed = Vec::new();

    // CRITICAL FIX: Ignore broken Sauerbraten footers. As long as decompressed isn't empty, it worked.
    let _ = decoder.read_to_end(&mut decompressed);
    if decompressed.is_empty() {
        return None;
    }

    let mut pos = 0;
    unpack_block(&decompressed, &mut pos)
}

pub fn decode_undo(data: &[u8]) -> Option<(SelInfo, Vec<sauer_map::Node>)> {
    let mut decoder = ZlibDecoder::new(data);
    let mut decompressed = Vec::new();

    let _ = decoder.read_to_end(&mut decompressed);
    if decompressed.is_empty() {
        return None;
    }

    let mut pos = 0;
    if pos + 2 > decompressed.len() {
        return None;
    }
    let numents = u16::from_le_bytes([decompressed[0], decompressed[1]]);
    pos += 2;

    if numents > 0 {
        return None;
    } // We don't care about entity-only undos
    unpack_block(&decompressed, &mut pos)
}
