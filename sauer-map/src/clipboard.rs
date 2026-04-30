use crate::{Cube, Node, Selection};
use flate2::{Compression, read::ZlibDecoder, write::ZlibEncoder};
use ogz_rs::data::VSlot;
use std::io::{Read, Write};

pub fn pack_cube(node: &Node, out: &mut Vec<u8>) {
    match node {
        Node::Branch(children) => {
            out.push(0xFF);
            for child in children.as_ref() {
                pack_cube(child, out);
            }
        }
        Node::Leaf(cube) => {
            let mat_bits = cube.material.to_bits();
            out.push((mat_bits & 0xFF) as u8);
            out.push((mat_bits >> 8) as u8);
            out.extend_from_slice(&cube.edges);
            for &tex in &cube.textures {
                out.extend_from_slice(&tex.to_le_bytes());
            }
        }
    }
}

fn write_putint(out: &mut Vec<u8>, n: i32) {
    if n < 128 && n > -127 {
        out.push(n as u8);
    } else if n < 0x8000 && n >= -0x8000 {
        out.push(0x80);
        out.push(n as u8);
        out.push((n >> 8) as u8);
    } else {
        out.push(0x81);
        out.push(n as u8);
        out.push((n >> 8) as u8);
        out.push((n >> 16) as u8);
        out.push((n >> 24) as u8);
    }
}

fn write_putuint(out: &mut Vec<u8>, n: i32) {
    if n < 0 || n >= (1 << 21) {
        out.push(0x80 | (n as u8 & 0x7F));
        out.push(0x80 | ((n >> 7) as u8 & 0x7F));
        out.push(0x80 | ((n >> 14) as u8 & 0x7F));
        out.push((n >> 21) as u8);
    } else if n < (1 << 7) {
        out.push(n as u8);
    } else if n < (1 << 14) {
        out.push(0x80 | (n as u8 & 0x7F));
        out.push((n >> 7) as u8);
    } else {
        out.push(0x80 | (n as u8 & 0x7F));
        out.push(0x80 | ((n >> 7) as u8 & 0x7F));
        out.push((n >> 14) as u8);
    }
}

fn write_sendstring(out: &mut Vec<u8>, s: &cube_string::CubeString) {
    for &c in s.as_bytes() {
        write_putint(out, c as i32);
    }
    write_putint(out, 0);
}

fn pack_vslot(out: &mut Vec<u8>, vs: &VSlot) {
    if !vs.params.is_empty() {
        for p in &vs.params {
            out.push(0);
            write_sendstring(out, &p.name);
            for i in 0..4 {
                out.extend_from_slice(&p.values[i].to_le_bytes());
            }
        }
    }
    if let Some(scale) = vs.scale {
        out.push(1);
        out.extend_from_slice(&scale.to_le_bytes());
    }
    if let Some(rotation) = vs.rotation {
        out.push(2);
        write_putint(out, rotation);
    }
    if let Some(offset) = vs.offset {
        out.push(3);
        write_putint(out, offset.0);
        write_putint(out, offset.1);
    }
    if let Some(scroll) = vs.scroll {
        out.push(4);
        out.extend_from_slice(&scroll.0.to_le_bytes());
        out.extend_from_slice(&scroll.1.to_le_bytes());
    }
    if let Some(layer) = vs.layer {
        out.push(5);
        write_putuint(out, layer);
    }
    if let Some(alpha) = vs.alpha {
        out.push(6);
        out.extend_from_slice(&alpha.0.to_le_bytes());
        out.extend_from_slice(&alpha.1.to_le_bytes());
    }
    if let Some(color) = vs.color {
        out.push(7);
        out.extend_from_slice(&color.0.to_le_bytes());
        out.extend_from_slice(&color.1.to_le_bytes());
        out.extend_from_slice(&color.2.to_le_bytes());
    }
    out.push(0xFF);
}

pub fn pack_vslots(nodes: &[Node], vslots: &[Option<VSlot>], out: &mut Vec<u8>) {
    let mut used = std::collections::HashSet::new();
    fn collect(node: &Node, used: &mut std::collections::HashSet<u16>) {
        match node {
            Node::Leaf(cube) => {
                for &tex in &cube.textures {
                    used.insert(tex);
                }
            }
            Node::Branch(children) => {
                for child in children.as_ref() {
                    collect(child, used);
                }
            }
        }
    }
    for node in nodes {
        collect(node, &mut used);
    }
    let mut used_vec: Vec<u16> = used.into_iter().collect();
    used_vec.sort_unstable();

    for idx in used_vec {
        if let Some(Some(vs)) = vslots.get(idx as usize) {
            let slot_index = crate::vslot::root_base_of(vslots, idx) as u16;
            out.extend_from_slice(&idx.to_le_bytes());
            out.extend_from_slice(&slot_index.to_le_bytes());
            pack_vslot(out, vs);
        }
    }
    out.extend_from_slice(&0u32.to_le_bytes());
}

pub fn generate_clipboard(node: &Node, size: i32, vslots: &[Option<VSlot>]) -> (i32, Vec<u8>) {
    let mut uncompressed = Vec::new();
    for val in [0i32, 0, 0] {
        uncompressed.extend_from_slice(&val.to_le_bytes());
    }
    for val in [1i32, 1, 1] {
        uncompressed.extend_from_slice(&val.to_le_bytes());
    }
    uncompressed.extend_from_slice(&size.to_le_bytes());
    uncompressed.extend_from_slice(&0i32.to_le_bytes());
    pack_cube(node, &mut uncompressed);
    pack_vslots(&[node.clone()], vslots, &mut uncompressed);

    let unpack_len = uncompressed.len() as i32;
    let mut encoder = ZlibEncoder::new(Vec::new(), Compression::best());
    encoder.write_all(&uncompressed).unwrap();
    (unpack_len, encoder.finish().unwrap())
}

pub fn generate_clipboard_for_region(
    root: &Node,
    world_size: i32,
    padded_min: [i32; 3],
    s: [i32; 3],
    grid: i32,
    vslots: &[Option<VSlot>],
) -> (i32, Vec<u8>) {
    let [sx, sy, sz] = s;
    let mut uncompressed = Vec::new();
    for &v in &padded_min {
        uncompressed.extend_from_slice(&v.to_le_bytes());
    }
    for &v in &[sx, sy, sz] {
        uncompressed.extend_from_slice(&v.to_le_bytes());
    }
    uncompressed.extend_from_slice(&grid.to_le_bytes());
    uncompressed.extend_from_slice(&0i32.to_le_bytes());

    let mut nodes = Vec::with_capacity((sx * sy * sz) as usize);
    for xi in 0..sx {
        for zi in 0..sz {
            for yi in 0..sy {
                let cell_pos = [
                    padded_min[0] + xi * grid,
                    padded_min[1] + yi * grid,
                    padded_min[2] + zi * grid,
                ];
                let node =
                    crate::octree::extract_node_at(root, [0, 0, 0], world_size, cell_pos, grid);
                pack_cube(&node, &mut uncompressed);
                nodes.push(node);
            }
        }
    }
    pack_vslots(&nodes, vslots, &mut uncompressed);
    let unpack_len = uncompressed.len() as i32;
    let mut encoder = ZlibEncoder::new(Vec::new(), Compression::best());
    encoder.write_all(&uncompressed).unwrap();
    (unpack_len, encoder.finish().unwrap())
}

pub fn generate_region_clipboard(
    root: &Node,
    world_size: i32,
    min: [i32; 3],
    max: [i32; 3],
    vslots: &[Option<VSlot>],
) -> ([i32; 3], [i32; 3], i32, i32, Vec<u8>) {
    let spans = [max[0] - min[0], max[1] - min[1], max[2] - min[2]];
    let max_span = spans.iter().copied().max().unwrap_or(1).max(1);
    let mut grid = 1i32;
    while (max_span + grid - 1) / grid > 8 {
        grid <<= 1;
    }

    let padded_min: [i32; 3] = std::array::from_fn(|i| min[i].div_euclid(grid) * grid);
    let padded_max: [i32; 3] =
        std::array::from_fn(|i| (((max[i] + grid - 1).div_euclid(grid)) * grid).min(world_size));
    let sx = (padded_max[0] - padded_min[0]) / grid;
    let sy = (padded_max[1] - padded_min[1]) / grid;
    let sz = (padded_max[2] - padded_min[2]) / grid;
    let s = [sx, sy, sz];

    let (unpack_len, compressed) =
        generate_clipboard_for_region(root, world_size, padded_min, s, grid, vslots);
    (padded_min, s, grid, unpack_len, compressed)
}

pub fn unpack_cube(bytes: &[u8], pos: &mut usize) -> Option<Node> {
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
        Some(Node::Branch(Box::new(children.try_into().unwrap())))
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
        Some(Node::Leaf(Cube {
            edges,
            textures,
            material: ogz_rs::data::Material::from_bits(material),
        }))
    }
}

pub fn unpack_block(bytes: &[u8], pos: &mut usize) -> Option<(Selection, Vec<Node>)> {
    if *pos + 32 > bytes.len() {
        return None;
    }
    let get_i32 = |offset: usize| {
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

    let sel = Selection {
        o: sauer_core::Vec3 {
            x: o_x,
            y: o_y,
            z: o_z,
        },
        s: sauer_core::Vec3 {
            x: s_x,
            y: s_y,
            z: s_z,
        },
        grid,
        orient: sauer_core::Orient::try_from(orient).unwrap_or(sauer_core::Orient::Left),
        cx: 0,
        cxs: 0,
        cy: 0,
        cys: 0,
        corner: 0,
    };
    Some((sel, nodes))
}

pub fn decode_clipboard(data: &[u8]) -> Option<(Selection, Vec<Node>)> {
    let mut decoder = ZlibDecoder::new(data);
    let mut decompressed = Vec::new();
    let _ = decoder.read_to_end(&mut decompressed);
    if decompressed.is_empty() {
        return None;
    }
    let mut pos = 0;
    unpack_block(&decompressed, &mut pos)
}

pub fn decode_undo(data: &[u8]) -> Option<(Selection, Vec<Node>)> {
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
    }
    unpack_block(&decompressed, &mut pos)
}
