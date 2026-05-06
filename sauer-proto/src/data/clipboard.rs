use crate::internal::{SauerRead, SauerWrite};
use crate::ParseError;
use sauer_core::{Node, Cube, Selection, VSlot, VSlotParam, Entity};
use flate2::{Compression, read::ZlibDecoder, write::ZlibEncoder};
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

pub fn unpack_cube(bytes: &[u8], pos: &mut usize) -> Option<Node> {
    if *pos >= bytes.len() { return None; }
    let mat = bytes[*pos];
    *pos += 1;
    if mat == 0xFF {
        let mut children = Vec::with_capacity(8);
        for _ in 0..8 {
            children.push(unpack_cube(bytes, pos)?);
        }
        Some(Node::Branch(Box::new(children.try_into().unwrap())))
    } else {
        if *pos >= bytes.len() { return None; }
        let mat_hi = bytes[*pos];
        *pos += 1;
        let material = (mat as u16) | ((mat_hi as u16) << 8);

        let mut edges = [0u8; 12];
        if *pos + 12 > bytes.len() { return None; }
        edges.copy_from_slice(&bytes[*pos..*pos + 12]);
        *pos += 12;

        let mut textures = [0u16; 6];
        for i in 0..6 {
            if *pos + 2 > bytes.len() { return None; }
            textures[i] = u16::from_le_bytes([bytes[*pos], bytes[*pos + 1]]);
            *pos += 2;
        }
        Some(Node::Leaf(Cube {
            edges,
            textures,
            material: sauer_core::Material::from_bits(material),
        }))
    }
}

pub fn pack_vslot(out: &mut Vec<u8>, vs: &VSlot) {
    use sauer_core::io::{putint, putuint, putfloat, putstring};
    if !vs.params.is_empty() {
        for p in &vs.params {
            out.push(0);
            putstring(out, p.name.as_bytes());
            for i in 0..4 {
                putfloat(out, p.values[i]);
            }
        }
    }
    if let Some(scale) = vs.scale {
        out.push(1);
        putfloat(out, scale);
    }
    if let Some(rotation) = vs.rotation {
        out.push(2);
        putint(out, rotation);
    }
    if let Some(offset) = vs.offset {
        out.push(3);
        putint(out, offset.0);
        putint(out, offset.1);
    }
    if let Some(scroll) = vs.scroll {
        out.push(4);
        putfloat(out, scroll.0);
        putfloat(out, scroll.1);
    }
    if let Some(layer) = vs.layer {
        out.push(5);
        putuint(out, layer);
    }
    if let Some(alpha) = vs.alpha {
        out.push(6);
        putfloat(out, alpha.0);
        putfloat(out, alpha.1);
    }
    if let Some(color) = vs.color {
        out.push(7);
        putfloat(out, color.0);
        putfloat(out, color.1);
        putfloat(out, color.2);
    }
    out.push(0xFF);
}

pub fn unpack_vslot(bytes: &[u8], pos: &mut usize) -> Option<VSlot> {
    use sauer_core::io::{getint, getuint, getfloat, getstring};
    let mut vs = VSlot::default();
    loop {
        if *pos >= bytes.len() { break; }
        let type_byte = bytes[*pos];
        *pos += 1;
        if type_byte == 0xFF { break; }
        
        let mut slice = &bytes[*pos..];
        match type_byte {
            0 => {
                let name = getstring(&mut slice)?;
                let mut values = [0.0; 4];
                for i in 0..4 { values[i] = getfloat(&mut slice)?; }
                vs.params.push(VSlotParam { name, values });
            }
            1 => vs.scale = Some(getfloat(&mut slice)?),
            2 => vs.rotation = Some(getint(&mut slice)?),
            3 => {
                let x = getint(&mut slice)?;
                let y = getint(&mut slice)?;
                vs.offset = Some((x, y));
            }
            4 => {
                let s = getfloat(&mut slice)?;
                let t = getfloat(&mut slice)?;
                vs.scroll = Some((s, t));
            }
            5 => vs.layer = Some(getuint(&mut slice)?),
            6 => {
                let f = getfloat(&mut slice)?;
                let b = getfloat(&mut slice)?;
                vs.alpha = Some((f, b));
            }
            7 => {
                let r = getfloat(&mut slice)?;
                let g = getfloat(&mut slice)?;
                let b = getfloat(&mut slice)?;
                vs.color = Some((r, g, b));
            }
            _ => return None,
        }
        *pos = bytes.len() - slice.len();
    }
    Some(vs)
}

pub fn pack_entity(ent: &Entity, out: &mut Vec<u8>) {
    out.extend_from_slice(&ent.position[0].to_le_bytes());
    out.extend_from_slice(&ent.position[1].to_le_bytes());
    out.extend_from_slice(&ent.position[2].to_le_bytes());
    for attr in &ent.attrs {
        out.extend_from_slice(&attr.to_le_bytes());
    }
    out.push(i32::from(ent.ent_type) as u8);
    out.push(0);
}

pub fn unpack_entity(bytes: &[u8], pos: &mut usize) -> Option<Entity> {
    let get_f32 = |bytes: &[u8], pos: &mut usize| -> Option<f32> {
        if *pos + 4 > bytes.len() { return None; }
        let v = f32::from_le_bytes(bytes[*pos..*pos+4].try_into().unwrap());
        *pos += 4;
        Some(v)
    };
    let get_i16 = |bytes: &[u8], pos: &mut usize| -> Option<i16> {
        if *pos + 2 > bytes.len() { return None; }
        let v = i16::from_le_bytes(bytes[*pos..*pos+2].try_into().unwrap());
        *pos += 2;
        Some(v)
    };
    let x = get_f32(bytes, pos)?;
    let y = get_f32(bytes, pos)?;
    let z = get_f32(bytes, pos)?;
    let mut attrs = [0; 5];
    for i in 0..5 {
        attrs[i] = get_i16(bytes, pos)?;
    }
    if *pos + 2 > bytes.len() { return None; }
    let ent_type = bytes[*pos];
    *pos += 2;
    Some(Entity {
        position: [x, y, z],
        attrs,
        ent_type: sauer_core::EntityType::try_from(ent_type as i32).unwrap_or(sauer_core::EntityType::Empty),
        extra_data: Vec::new(),
    })
}

#[derive(Debug, Clone, PartialEq)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
pub struct ClipboardPayload {
    pub unpack_len: i32,
    pub sel: Selection,
    pub nodes: Vec<Node>,
    pub vslots: Vec<(i32, VSlot)>,
}

impl SauerWrite for ClipboardPayload {
    fn sauer_write(&self, out: &mut Vec<u8>) {
        let mut uncompressed = Vec::new();
        
        uncompressed.extend_from_slice(&self.sel.o.x.to_le_bytes());
        uncompressed.extend_from_slice(&self.sel.o.y.to_le_bytes());
        uncompressed.extend_from_slice(&self.sel.o.z.to_le_bytes());
        uncompressed.extend_from_slice(&self.sel.s.x.to_le_bytes());
        uncompressed.extend_from_slice(&self.sel.s.y.to_le_bytes());
        uncompressed.extend_from_slice(&self.sel.s.z.to_le_bytes());
        uncompressed.extend_from_slice(&self.sel.grid.to_le_bytes());
        uncompressed.extend_from_slice(&(self.sel.orient as i32).to_le_bytes());
        
        for node in &self.nodes {
            pack_cube(node, &mut uncompressed);
        }
        
        for (idx, vs) in &self.vslots {
            uncompressed.extend_from_slice(&idx.to_le_bytes());
            pack_vslot(&mut uncompressed, vs);
        }
        if !self.vslots.is_empty() {
            uncompressed.extend_from_slice(&(-1i32).to_le_bytes());
        }

        let mut encoder = ZlibEncoder::new(Vec::new(), Compression::best());
        encoder.write_all(&uncompressed).unwrap();
        let compressed = encoder.finish().unwrap();

        (uncompressed.len() as i32).sauer_write(out);
        (compressed.len() as i32).sauer_write(out);
        out.extend_from_slice(&compressed);
    }
}

impl SauerRead for ClipboardPayload {
    fn sauer_read(reader: &mut &[u8]) -> Result<Self, ParseError> {
        let unpack_len = i32::sauer_read(reader)?;
        let comp_len = i32::sauer_read(reader)?;
        if comp_len < 0 || comp_len > 16 * 1024 * 1024 {
            return Err(ParseError::ValueOutOfRange { value: comp_len, max: 16*1024*1024 });
        }
        let compressed = crate::internal::read_slice(reader, comp_len as usize)?;
        
        let mut decoder = ZlibDecoder::new(compressed);
        let mut decompressed = Vec::new();
        decoder.read_to_end(&mut decompressed).map_err(|_| ParseError::InvalidExtInfo)?;
        
        let mut pos = 0;
        let get_i32 = |bytes: &[u8], pos: &mut usize| -> Option<i32> {
            if *pos + 4 > bytes.len() { return None; }
            let v = i32::from_le_bytes(bytes[*pos..*pos+4].try_into().unwrap());
            *pos += 4;
            Some(v)
        };
        
        let o_x = get_i32(&decompressed, &mut pos).ok_or(ParseError::UnexpectedEof { expected: 4 })?;
        let o_y = get_i32(&decompressed, &mut pos).ok_or(ParseError::UnexpectedEof { expected: 4 })?;
        let o_z = get_i32(&decompressed, &mut pos).ok_or(ParseError::UnexpectedEof { expected: 4 })?;
        let s_x = get_i32(&decompressed, &mut pos).ok_or(ParseError::UnexpectedEof { expected: 4 })?;
        let s_y = get_i32(&decompressed, &mut pos).ok_or(ParseError::UnexpectedEof { expected: 4 })?;
        let s_z = get_i32(&decompressed, &mut pos).ok_or(ParseError::UnexpectedEof { expected: 4 })?;
        let grid = get_i32(&decompressed, &mut pos).ok_or(ParseError::UnexpectedEof { expected: 4 })?;
        let orient = get_i32(&decompressed, &mut pos).ok_or(ParseError::UnexpectedEof { expected: 4 })?;
        
        let sel = sauer_core::Selection {
            o: sauer_core::Vec3 { x: o_x, y: o_y, z: o_z },
            s: sauer_core::Vec3 { x: s_x, y: s_y, z: s_z },
            grid,
            orient: sauer_core::Orient::try_from(orient).unwrap_or(sauer_core::Orient::Left),
            cx: 0, cxs: 0, cy: 0, cys: 0, corner: 0,
        };
        
        let total_nodes = (s_x * s_y * s_z).max(0) as usize;
        let mut nodes = Vec::with_capacity(total_nodes);
        for _ in 0..total_nodes {
            nodes.push(unpack_cube(&decompressed, &mut pos).ok_or(ParseError::UnexpectedEof { expected: 1 })?);
        }
        
        let mut vslots = Vec::new();
        while pos + 4 <= decompressed.len() {
            let idx = get_i32(&decompressed, &mut pos).unwrap();
            if idx < 0 {
                break;
            }
            let vs = unpack_vslot(&decompressed, &mut pos).ok_or(ParseError::UnexpectedEof { expected: 1 })?;
            vslots.push((idx, vs));
        }
        
        Ok(ClipboardPayload { unpack_len, sel, nodes, vslots })
    }
}

#[derive(Debug, Clone, PartialEq)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
pub struct UndoPayload {
    pub unpack_len: i32,
    pub ents: Vec<Entity>,
    pub sel: Selection,
    pub nodes: Vec<Node>,
    pub vslots: Vec<(i32, VSlot)>,
}

impl SauerWrite for UndoPayload {
    fn sauer_write(&self, out: &mut Vec<u8>) {
        let mut uncompressed = Vec::new();
        
        let numents = self.ents.len() as u16;
        uncompressed.extend_from_slice(&numents.to_le_bytes());
        for ent in &self.ents {
            pack_entity(ent, &mut uncompressed);
        }

        uncompressed.extend_from_slice(&self.sel.o.x.to_le_bytes());
        uncompressed.extend_from_slice(&self.sel.o.y.to_le_bytes());
        uncompressed.extend_from_slice(&self.sel.o.z.to_le_bytes());
        uncompressed.extend_from_slice(&self.sel.s.x.to_le_bytes());
        uncompressed.extend_from_slice(&self.sel.s.y.to_le_bytes());
        uncompressed.extend_from_slice(&self.sel.s.z.to_le_bytes());
        uncompressed.extend_from_slice(&self.sel.grid.to_le_bytes());
        uncompressed.extend_from_slice(&(self.sel.orient as i32).to_le_bytes());
        
        for node in &self.nodes {
            pack_cube(node, &mut uncompressed);
        }
        
        for (idx, vs) in &self.vslots {
            uncompressed.extend_from_slice(&idx.to_le_bytes());
            pack_vslot(&mut uncompressed, vs);
        }
        if !self.vslots.is_empty() {
            uncompressed.extend_from_slice(&(-1i32).to_le_bytes());
        }

        let mut encoder = ZlibEncoder::new(Vec::new(), Compression::best());
        encoder.write_all(&uncompressed).unwrap();
        let compressed = encoder.finish().unwrap();

        (uncompressed.len() as i32).sauer_write(out);
        (compressed.len() as i32).sauer_write(out);
        out.extend_from_slice(&compressed);
    }
}

impl SauerRead for UndoPayload {
    fn sauer_read(reader: &mut &[u8]) -> Result<Self, ParseError> {
        let unpack_len = i32::sauer_read(reader)?;
        let comp_len = i32::sauer_read(reader)?;
        if comp_len < 0 || comp_len > 16 * 1024 * 1024 {
            return Err(ParseError::ValueOutOfRange { value: comp_len, max: 16*1024*1024 });
        }
        let compressed = crate::internal::read_slice(reader, comp_len as usize)?;
        
        let mut decoder = ZlibDecoder::new(compressed);
        let mut decompressed = Vec::new();
        decoder.read_to_end(&mut decompressed).map_err(|_| ParseError::InvalidExtInfo)?;
        
        let mut pos = 0;
        if pos + 2 > decompressed.len() { return Err(ParseError::UnexpectedEof { expected: 2 }); }
        let numents = u16::from_le_bytes(decompressed[pos..pos+2].try_into().unwrap());
        pos += 2;
        
        let mut ents = Vec::with_capacity(numents as usize);
        for _ in 0..numents {
            ents.push(unpack_entity(&decompressed, &mut pos).ok_or(ParseError::UnexpectedEof { expected: 24 })?);
        }

        let get_i32 = |bytes: &[u8], pos: &mut usize| -> Option<i32> {
            if *pos + 4 > bytes.len() { return None; }
            let v = i32::from_le_bytes(bytes[*pos..*pos+4].try_into().unwrap());
            *pos += 4;
            Some(v)
        };
        
        let o_x = get_i32(&decompressed, &mut pos).ok_or(ParseError::UnexpectedEof { expected: 4 })?;
        let o_y = get_i32(&decompressed, &mut pos).ok_or(ParseError::UnexpectedEof { expected: 4 })?;
        let o_z = get_i32(&decompressed, &mut pos).ok_or(ParseError::UnexpectedEof { expected: 4 })?;
        let s_x = get_i32(&decompressed, &mut pos).ok_or(ParseError::UnexpectedEof { expected: 4 })?;
        let s_y = get_i32(&decompressed, &mut pos).ok_or(ParseError::UnexpectedEof { expected: 4 })?;
        let s_z = get_i32(&decompressed, &mut pos).ok_or(ParseError::UnexpectedEof { expected: 4 })?;
        let grid = get_i32(&decompressed, &mut pos).ok_or(ParseError::UnexpectedEof { expected: 4 })?;
        let orient = get_i32(&decompressed, &mut pos).ok_or(ParseError::UnexpectedEof { expected: 4 })?;
        
        let sel = sauer_core::Selection {
            o: sauer_core::Vec3 { x: o_x, y: o_y, z: o_z },
            s: sauer_core::Vec3 { x: s_x, y: s_y, z: s_z },
            grid,
            orient: sauer_core::Orient::try_from(orient).unwrap_or(sauer_core::Orient::Left),
            cx: 0, cxs: 0, cy: 0, cys: 0, corner: 0,
        };
        
        let total_nodes = (s_x * s_y * s_z).max(0) as usize;
        let mut nodes = Vec::with_capacity(total_nodes);
        for _ in 0..total_nodes {
            nodes.push(unpack_cube(&decompressed, &mut pos).ok_or(ParseError::UnexpectedEof { expected: 1 })?);
        }
        
        let mut vslots = Vec::new();
        while pos + 4 <= decompressed.len() {
            let idx = get_i32(&decompressed, &mut pos).unwrap();
            if idx < 0 {
                break;
            }
            let vs = unpack_vslot(&decompressed, &mut pos).ok_or(ParseError::UnexpectedEof { expected: 1 })?;
            vslots.push((idx, vs));
        }
        
        Ok(UndoPayload { unpack_len, ents, sel, nodes, vslots })
    }
}

#[cfg(feature = "lua")]
const _: () = {
    use mlua::{FromLua, UserData, UserDataFields};

    impl UserData for ClipboardPayload {
        fn add_fields<F: UserDataFields<Self>>(_fields: &mut F) {}
    }

    impl FromLua for ClipboardPayload {
        fn from_lua(_value: mlua::Value, _lua: &mlua::Lua) -> mlua::Result<Self> {
            Err(mlua::Error::FromLuaConversionError {
                from: "Value",
                to: "ClipboardPayload".to_owned(),
                message: Some("not supported in lua".to_owned()),
            })
        }
    }

    impl UserData for UndoPayload {
        fn add_fields<F: UserDataFields<Self>>(_fields: &mut F) {}
    }

    impl FromLua for UndoPayload {
        fn from_lua(_value: mlua::Value, _lua: &mlua::Lua) -> mlua::Result<Self> {
            Err(mlua::Error::FromLuaConversionError {
                from: "Value",
                to: "UndoPayload".to_owned(),
                message: Some("not supported in lua".to_owned()),
            })
        }
    }
};