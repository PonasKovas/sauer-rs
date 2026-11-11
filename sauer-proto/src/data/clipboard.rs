//! The zlib compressed edit blocks of `N_CLIPBOARD`, `N_UNDO` and `N_REDO`
//! (`packeditinfo` and `packundo` in octaedit.cpp).

use crate::ParseError;
use crate::error::EnumConvertResultExt;
use crate::internal::{SauerRead, SauerWrite, read_slice};
use flate2::{Compression, read::ZlibDecoder, write::ZlibEncoder};
use sauer_core::{Cube, Entity, EntityType, Material, Node, Orient, Selection, VSlotProps, Vec3};
use std::io::{Read, Write};

/// Clients drop a block when zlib's `compressBound` of its unpacked length is
/// over 1 MB (`uncompresseditinfo` in octaedit.cpp).
pub const MAX_CLIENT_UNPACK_LEN: usize = {
    let mut n = 1 << 20;
    while n + (n >> 12) + (n >> 14) + (n >> 25) + 13 > 1 << 20 {
        n -= 1;
    }
    n
};

/// Most cubes a block may have (`unpackblock`).
const MAX_BLOCK_CUBES: i64 = 1 << 20;
/// Deeper than any real octree, to bound the recursion on bad data.
const MAX_CUBE_DEPTH: usize = 32;

/// A copied block of cubes, as `N_CLIPBOARD` sends it.
#[derive(Debug, Clone, PartialEq)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
pub struct ClipboardPayload {
    /// Only `o`, `s`, `grid` and `orient` are sent.
    pub sel: Selection,
    /// `s.x * s.y * s.z` cubes in the engine's block order.
    pub nodes: Vec<Node>,
    /// The changed vslots the cubes use, in the sender's numbering.
    pub vslots: Vec<VSlotData>,
}

/// A changed vslot an edit block uses.
#[derive(Debug, Clone, PartialEq)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
pub struct VSlotData {
    /// The vslot index the cubes use.
    pub index: u16,
    /// The texture slot it is a variant of.
    pub slot: u16,
    pub vslot: VSlotProps,
}

/// The block of an `N_UNDO` or `N_REDO`: either entities or cubes.
#[derive(Debug, Clone, PartialEq)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
pub enum UndoPayload {
    Entities(Vec<UndoEntity>),
    Cubes {
        /// Only `o`, `s`, `grid` and `orient` are sent.
        sel: Selection,
        nodes: Vec<Node>,
        /// The grid size each cube had, one per node.
        gridmap: Vec<u8>,
        vslots: Vec<VSlotData>,
    },
}

#[derive(Debug, Clone, PartialEq)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
pub struct UndoEntity {
    pub index: u16,
    pub ent: Entity,
}

impl ClipboardPayload {
    /// The bytes before zlib, whose length clients check against
    /// [`MAX_CLIENT_UNPACK_LEN`].
    pub fn uncompressed(&self) -> Vec<u8> {
        let mut out = Vec::new();
        write_block(&self.sel, &self.nodes, &mut out);
        write_vslots(&self.vslots, &mut out);
        out
    }

    /// Parses the bytes [`uncompressed`](Self::uncompressed) makes.
    pub fn from_uncompressed(mut data: &[u8]) -> Result<Self, ParseError> {
        let reader = &mut data;
        let (sel, nodes) = read_block(reader)?;
        let vslots = read_vslots(reader)?;
        Ok(Self { sel, nodes, vslots })
    }
}

impl SauerWrite for ClipboardPayload {
    fn sauer_write(&self, out: &mut Vec<u8>) {
        write_compressed(&self.uncompressed(), out);
    }
}

impl SauerRead for ClipboardPayload {
    fn sauer_read(reader: &mut &[u8]) -> Result<Self, ParseError> {
        Self::from_uncompressed(&read_compressed(reader)?)
    }
}

/// `N_CLIPBOARD` contents. `None` is an empty clipboard: clients send that
/// when their copy is too big to pack, and the server passes it on.
impl SauerWrite for Option<ClipboardPayload> {
    fn sauer_write(&self, out: &mut Vec<u8>) {
        match self {
            Some(payload) => payload.sauer_write(out),
            None => {
                0i32.sauer_write(out);
                0i32.sauer_write(out);
            }
        }
    }
}

impl SauerRead for Option<ClipboardPayload> {
    fn sauer_read(reader: &mut &[u8]) -> Result<Self, ParseError> {
        let mut peek = *reader;
        let unpack_len = i32::sauer_read(&mut peek)?;
        let pack_len = i32::sauer_read(&mut peek)?;
        // Like the server's N_CLIPBOARD handling.
        if pack_len <= 0 || pack_len > 1 << 16 || unpack_len <= 0 {
            read_slice(&mut peek, pack_len.max(0) as usize)?;
            *reader = peek;
            return Ok(None);
        }
        ClipboardPayload::sauer_read(reader).map(Some)
    }
}

impl SauerWrite for UndoPayload {
    fn sauer_write(&self, out: &mut Vec<u8>) {
        let mut data = Vec::new();
        match self {
            UndoPayload::Entities(ents) => {
                let count = u16::try_from(ents.len()).expect("undo of over 65535 entities");
                data.extend_from_slice(&count.to_le_bytes());
                for ue in ents {
                    data.extend_from_slice(&ue.index.to_le_bytes());
                    write_entity(&ue.ent, &mut data);
                }
            }
            UndoPayload::Cubes {
                sel,
                nodes,
                gridmap,
                vslots,
            } => {
                data.extend_from_slice(&0u16.to_le_bytes());
                write_block(sel, nodes, &mut data);
                data.extend_from_slice(gridmap);
                write_vslots(vslots, &mut data);
            }
        }
        write_compressed(&data, out);
    }
}

impl SauerRead for UndoPayload {
    fn sauer_read(reader: &mut &[u8]) -> Result<Self, ParseError> {
        let data = read_compressed(reader)?;
        let reader = &mut &data[..];
        let numents = u16::from_le_bytes(le(reader)?);
        if numents > 0 {
            let ents = (0..numents)
                .map(|_| {
                    Ok(UndoEntity {
                        index: u16::from_le_bytes(le(reader)?),
                        ent: read_entity(reader)?,
                    })
                })
                .collect::<Result<_, ParseError>>()?;
            return Ok(UndoPayload::Entities(ents));
        }
        let (sel, nodes) = read_block(reader)?;
        let gridmap = read_slice(reader, nodes.len())?.to_vec();
        let vslots = read_vslots(reader)?;
        Ok(UndoPayload::Cubes {
            sel,
            nodes,
            gridmap,
            vslots,
        })
    }
}

/// Reads `N` raw bytes, for the little endian fields of edit blocks.
fn le<const N: usize>(reader: &mut &[u8]) -> Result<[u8; N], ParseError> {
    Ok(read_slice(reader, N)?.try_into().unwrap())
}

/// Writes the unpacked length, the packed length and the zlib data.
fn write_compressed(data: &[u8], out: &mut Vec<u8>) {
    let mut encoder = ZlibEncoder::new(Vec::new(), Compression::best());
    encoder.write_all(data).unwrap();
    let compressed = encoder.finish().unwrap();
    (data.len() as i32).sauer_write(out);
    (compressed.len() as i32).sauer_write(out);
    out.extend_from_slice(&compressed);
}

/// Reads what [`write_compressed`] writes. Data over the unpacked length,
/// which clients would not take either, is an error.
fn read_compressed(reader: &mut &[u8]) -> Result<Vec<u8>, ParseError> {
    let unpack_len = i32::sauer_read(reader)?;
    let pack_len = i32::sauer_read(reader)?;
    for (value, max) in [
        (unpack_len, MAX_CLIENT_UNPACK_LEN as i32),
        (pack_len, 1 << 16),
    ] {
        if !(0..=max).contains(&value) {
            return Err(ParseError::ValueOutOfRange { value, max });
        }
    }
    let compressed = read_slice(reader, pack_len as usize)?;
    let mut data = Vec::with_capacity(unpack_len as usize);
    ZlibDecoder::new(compressed)
        .take(unpack_len as u64 + 1)
        .read_to_end(&mut data)
        .map_err(|_| ParseError::InvalidZlib)?;
    if data.len() > unpack_len as usize {
        return Err(ParseError::InvalidZlib);
    }
    Ok(data)
}

/// The engine's `block3` header followed by its cubes.
fn write_block(sel: &Selection, nodes: &[Node], out: &mut Vec<u8>) {
    for v in [
        sel.o.x, sel.o.y, sel.o.z, sel.s.x, sel.s.y, sel.s.z, sel.grid,
    ] {
        out.extend_from_slice(&v.to_le_bytes());
    }
    out.extend_from_slice(&(sel.orient as i32).to_le_bytes());
    for node in nodes {
        write_node(node, out);
    }
}

fn read_block(reader: &mut &[u8]) -> Result<(Selection, Vec<Node>), ParseError> {
    let mut int = || le(reader).map(i32::from_le_bytes);
    let o = Vec3 {
        x: int()?,
        y: int()?,
        z: int()?,
    };
    let s = Vec3 {
        x: int()?,
        y: int()?,
        z: int()?,
    };
    let grid = int()?;
    if !(1..=1 << 12).contains(&grid) {
        return Err(ParseError::ValueOutOfRange {
            value: grid,
            max: 1 << 12,
        });
    }
    let orient = Orient::try_from(int()?).in_enum("Orient")?;

    // A client only copies a real box. The engine checks just the product,
    // which two negative sizes would pass.
    if let Some(&bad) = [s.x, s.y, s.z].iter().find(|&&v| v <= 0) {
        return Err(ParseError::ValueOutOfRange {
            value: bad,
            max: i32::MAX,
        });
    }
    let count = s.x as i64 * s.y as i64 * s.z as i64;
    // Every cube takes at least one byte.
    let max = MAX_BLOCK_CUBES.min(reader.len() as i64);
    if !(0..=max).contains(&count) {
        return Err(ParseError::ValueOutOfRange {
            value: count.clamp(i32::MIN as i64, i32::MAX as i64) as i32,
            max: max as i32,
        });
    }
    let nodes = (0..count)
        .map(|_| read_node(reader, 0))
        .collect::<Result<_, _>>()?;
    let sel = Selection {
        o,
        s,
        grid,
        orient,
        cx: 0,
        cxs: 0,
        cy: 0,
        cys: 0,
        corner: 0,
    };
    Ok((sel, nodes))
}

fn write_node(node: &Node, out: &mut Vec<u8>) {
    match node {
        Node::Branch(children) => {
            out.push(0xFF);
            for child in children.iter() {
                write_node(child, out);
            }
        }
        Node::Leaf(cube) => {
            out.extend_from_slice(&cube.material.to_bits().to_le_bytes());
            out.extend_from_slice(&cube.edges);
            for tex in cube.textures {
                out.extend_from_slice(&tex.to_le_bytes());
            }
        }
    }
}

fn read_node(reader: &mut &[u8], depth: usize) -> Result<Node, ParseError> {
    let [mat_lo] = le(reader)?;
    if mat_lo == 0xFF {
        if depth >= MAX_CUBE_DEPTH {
            return Err(ParseError::ValueOutOfRange {
                value: depth as i32,
                max: MAX_CUBE_DEPTH as i32,
            });
        }
        let mut children = Vec::with_capacity(8);
        for _ in 0..8 {
            children.push(read_node(reader, depth + 1)?);
        }
        return Ok(Node::Branch(Box::new(children.try_into().unwrap())));
    }
    let [mat_hi] = le(reader)?;
    let edges = le(reader)?;
    let mut textures = [0; 6];
    for tex in &mut textures {
        *tex = u16::from_le_bytes(le(reader)?);
    }
    Ok(Node::Leaf(Cube {
        edges,
        textures,
        material: Material::from_bits(u16::from_le_bytes([mat_lo, mat_hi])),
    }))
}

/// `vslothdr`s with packed vslots, ended by an all zero header.
fn write_vslots(vslots: &[VSlotData], out: &mut Vec<u8>) {
    for v in vslots {
        out.extend_from_slice(&v.index.to_le_bytes());
        out.extend_from_slice(&v.slot.to_le_bytes());
        v.vslot.sauer_write(out);
    }
    out.extend_from_slice(&[0; 4]);
}

/// Reads what [`write_vslots`] writes. Like the engine, a missing end
/// header is fine.
fn read_vslots(reader: &mut &[u8]) -> Result<Vec<VSlotData>, ParseError> {
    let mut vslots = Vec::new();
    while reader.len() >= 4 {
        let index = u16::from_le_bytes(le(reader)?);
        let slot = u16::from_le_bytes(le(reader)?);
        if index == 0 {
            break;
        }
        let vslot = VSlotProps::sauer_read(reader)?;
        vslots.push(VSlotData { index, slot, vslot });
    }
    Ok(vslots)
}

/// The engine's `entity` struct.
fn write_entity(ent: &Entity, out: &mut Vec<u8>) {
    for v in ent.position {
        out.extend_from_slice(&v.to_le_bytes());
    }
    for attr in ent.attrs {
        out.extend_from_slice(&attr.to_le_bytes());
    }
    // The engine stores the type in a byte.
    out.push(u8::try_from(i32::from(ent.ent_type)).expect("entity type over 255"));
    out.push(0); // reserved
}

fn read_entity(reader: &mut &[u8]) -> Result<Entity, ParseError> {
    let mut position = [0.0; 3];
    for v in &mut position {
        *v = f32::from_le_bytes(le(reader)?);
    }
    let mut attrs = [0; 5];
    for attr in &mut attrs {
        *attr = i16::from_le_bytes(le(reader)?);
    }
    let [ent_type, _reserved] = le(reader)?;
    Ok(Entity {
        position,
        attrs,
        ent_type: EntityType::from(i32::from(ent_type)),
        extra_data: Vec::new(),
    })
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
