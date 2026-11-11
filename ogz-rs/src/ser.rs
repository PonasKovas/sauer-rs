use crate::error::OgzError;
use crate::{OgzMap, data::*};
use byteorder::{LittleEndian, WriteBytesExt};
use flate2::Compression;
use flate2::write::GzEncoder;
use std::io::{BufWriter, Write};

/// A length that must fit the field the engine stores it in. A wrapped
/// length would make the engine misread the rest of the file.
fn checked<T: TryFrom<usize>>(len: usize, what: &'static str) -> Result<T, OgzError> {
    T::try_from(len).map_err(|_| OgzError::LimitExceeded(what))
}

pub fn serialize_map(map: &OgzMap, compression: Compression) -> Result<Vec<u8>, OgzError> {
    let mut encoder = BufWriter::new(GzEncoder::new(Vec::new(), compression));

    // Header
    encoder.write_all(b"OCTA")?;
    encoder.write_i32::<LittleEndian>(33)?; // Hardcoded version 33
    encoder.write_i32::<LittleEndian>(28)?; // headersize
    encoder.write_i32::<LittleEndian>(map.world_size)?;
    encoder.write_i32::<LittleEndian>(checked(map.entities.len(), "numents")?)?;
    let num_pvs = map.pvs.as_ref().map_or(0, |p| p.cells.len());
    encoder.write_i32::<LittleEndian>(checked(num_pvs, "numpvs")?)?;
    encoder.write_i32::<LittleEndian>(checked(map.lightmaps.len(), "lightmaps")?)?;

    // The engine only checks whether the blendmap field is set.
    encoder.write_i32::<LittleEndian>(map.blendmap.is_some() as i32)?;
    encoder.write_i32::<LittleEndian>(checked(map.variables.len(), "numvars")?)?;
    encoder.write_i32::<LittleEndian>(checked(map.vslots.len(), "numvslots")?)?;

    for var in &map.variables {
        let (ty, name) = match var {
            Variable::Int { name, .. } => (0, name),
            Variable::Float { name, .. } => (1, name),
            Variable::String { name, .. } => (2, name),
        };
        encoder.write_u8(ty)?;
        encoder.write_u16::<LittleEndian>(checked(name.len(), "var name")?)?;
        encoder.write_all(name.as_bytes())?;
        match var {
            Variable::Int { value, .. } => encoder.write_i32::<LittleEndian>(*value)?,
            Variable::Float { value, .. } => encoder.write_f32::<LittleEndian>(*value)?,
            Variable::String { value, .. } => {
                encoder.write_u16::<LittleEndian>(checked(value.len(), "var value")?)?;
                encoder.write_all(value.as_bytes())?;
            }
        }
    }

    if let Some(ref gt) = map.game_type {
        encoder.write_u8(checked(gt.len(), "gametype")?)?;
        encoder.write_all(gt.as_bytes())?;
        encoder.write_u8(0)?; // Null terminator
        encoder.write_u16::<LittleEndian>(map.eif)?;
        encoder
            .write_u16::<LittleEndian>(checked(map.extra_game_data.len(), "extra game data")?)?;
        encoder.write_all(&map.extra_game_data)?;
    } else {
        encoder.write_u8(0)?; // fallback gametype logic or empty
        encoder.write_u8(0)?;
        encoder.write_u16::<LittleEndian>(0)?; // eif
        encoder.write_u16::<LittleEndian>(0)?; // extrasize
    }

    encoder.write_u16::<LittleEndian>(checked(map.mru_textures.len(), "texture mru")?)?;
    for tex in &map.mru_textures {
        encoder.write_u16::<LittleEndian>(*tex)?;
    }

    for ent in &map.entities {
        for p in &ent.position {
            encoder.write_f32::<LittleEndian>(*p)?;
        }
        for a in &ent.attrs {
            encoder.write_i16::<LittleEndian>(*a)?;
        }
        encoder.write_u8(checked(i32::from(ent.ent_type) as usize, "entity type")?)?;
        encoder.write_u8(0)?; // reserved
        if map.eif > 0 {
            encoder.write_all(&ent.extra_data)?;
        }
    }

    let mut i = 0;
    while i < map.vslots.len() {
        if map.vslots[i].is_none() {
            let mut skip = 0;
            while i < map.vslots.len() && map.vslots[i].is_none() {
                skip += 1;
                i += 1;
            }
            encoder.write_i32::<LittleEndian>(-skip)?;
        } else {
            let slot = map.vslots[i].as_ref().unwrap();
            let mut changed = 0i32;
            if !slot.props.params.is_empty() {
                changed |= 1 << 0;
            }
            if slot.props.scale.is_some() {
                changed |= 1 << 1;
            }
            if slot.props.rotation.is_some() {
                changed |= 1 << 2;
            }
            if slot.props.offset.is_some() {
                changed |= 1 << 3;
            }
            if slot.props.scroll.is_some() {
                changed |= 1 << 4;
            }
            if slot.props.layer.is_some() {
                changed |= 1 << 5;
            }
            if slot.props.alpha.is_some() {
                changed |= 1 << 6;
            }
            if slot.props.color.is_some() {
                changed |= 1 << 7;
            }

            encoder.write_i32::<LittleEndian>(changed)?;
            encoder.write_i32::<LittleEndian>(slot.prev)?;

            if !slot.props.params.is_empty() {
                encoder
                    .write_u16::<LittleEndian>(checked(slot.props.params.len(), "vslot params")?)?;
                for p in &slot.props.params {
                    encoder
                        .write_u16::<LittleEndian>(checked(p.name.len(), "vslot param name")?)?;
                    encoder.write_all(p.name.as_bytes())?;
                    for f in &p.values {
                        encoder.write_f32::<LittleEndian>(*f)?;
                    }
                }
            }
            if let Some(s) = slot.props.scale {
                encoder.write_f32::<LittleEndian>(s)?;
            }
            if let Some(r) = slot.props.rotation {
                encoder.write_i32::<LittleEndian>(r)?;
            }
            if let Some(o) = slot.props.offset {
                encoder.write_i32::<LittleEndian>(o.0)?;
                encoder.write_i32::<LittleEndian>(o.1)?;
            }
            if let Some(sc) = slot.props.scroll {
                encoder.write_f32::<LittleEndian>(sc.0)?;
                encoder.write_f32::<LittleEndian>(sc.1)?;
            }
            if let Some(l) = slot.props.layer {
                encoder.write_i32::<LittleEndian>(l)?;
            }
            if let Some(a) = slot.props.alpha {
                encoder.write_f32::<LittleEndian>(a.0)?;
                encoder.write_f32::<LittleEndian>(a.1)?;
            }
            if let Some(c) = slot.props.color {
                encoder.write_f32::<LittleEndian>(c.0)?;
                encoder.write_f32::<LittleEndian>(c.1)?;
                encoder.write_f32::<LittleEndian>(c.2)?;
            }
            i += 1;
        }
    }

    // C++ writes the 8 worldroot cubes directly (no wrapper byte).
    // Our OctreeNode wraps them in a Children node, so unwrap it here.
    match &map.octree {
        OctreeNode::Children(children) => {
            for child in children.as_ref() {
                write_cube(child, &mut encoder)?;
            }
        }
        _ => write_cube(&map.octree, &mut encoder)?,
    }

    for lm in &map.lightmaps {
        encoder.write_u8(lm.map_type)?;
        if (lm.map_type & 0x80) != 0 {
            if let Some((ux, uy)) = lm.unlit {
                encoder.write_u16::<LittleEndian>(ux)?;
                encoder.write_u16::<LittleEndian>(uy)?;
            } else {
                encoder.write_u16::<LittleEndian>(0)?;
                encoder.write_u16::<LittleEndian>(0)?; // Fallback safe empty
            }
        }
        encoder.write_all(&lm.data)?;
    }

    // The engine only reads it when the header counts view cells.
    if let Some(pvs) = map.pvs.as_ref().filter(|_| num_pvs > 0) {
        crate::pvs::write(&mut encoder, pvs)?;
    }
    if let Some(ref bdata) = map.blendmap {
        encoder.write_all(bdata)?;
    }

    let gz_encoder = encoder.into_inner().map_err(|e| e.into_error())?;
    Ok(gz_encoder.finish()?)
}

fn write_cube<W: Write>(node: &OctreeNode, w: &mut W) -> Result<(), OgzError> {
    let (ty, info, edges) = match node {
        OctreeNode::Children(c) => {
            w.write_u8(0)?;
            for child in c.as_ref() {
                write_cube(child, w)?;
            }
            return Ok(());
        }
        OctreeNode::Empty(i) => (1, i, None),
        OctreeNode::Solid(i) => (2, i, None),
        OctreeNode::Normal { info, edges } => (3, info, Some(edges)),
        OctreeNode::LodCube { info, .. } => (4, info, None),
    };

    let mut octsav = ty;
    let mod_surf = match &info.surfaces {
        Surfaces::Modern(m) => m,
        Surfaces::None => &ModernSurfaceData {
            material: None,
            merged: None,
            surfaces: None,
        },
        Surfaces::Legacy(_) => return Err(OgzError::CannotSerializeLegacy),
    };

    if mod_surf.material.is_some() {
        octsav |= 0x40;
    }
    if mod_surf.merged.is_some() {
        octsav |= 0x80;
    }
    if mod_surf.surfaces.is_some() {
        octsav |= 0x20;
    }

    w.write_u8(octsav)?;

    if ty == 3 {
        w.write_all(edges.unwrap())?;
    }
    for tex in &info.textures {
        w.write_u16::<LittleEndian>(*tex)?;
    }

    if let Some(mat) = mod_surf.material {
        w.write_u16::<LittleEndian>(mat.to_bits())?;
    }
    if let Some(m) = mod_surf.merged {
        w.write_u8(m)?;
    }

    if let Some(s) = &mod_surf.surfaces {
        w.write_u8(s.surfmask)?;
        w.write_u8(s.totalverts)?;
        for face in &s.faces {
            w.write_all(&face.lmid)?;
            w.write_u8(face.vertmask)?;
            w.write_u8(face.numverts)?;

            if let Some(xyz) = &face.xyz {
                for v in xyz {
                    w.write_u16::<LittleEndian>(*v)?;
                }
            }
            if let Some(uv) = &face.uv {
                for v in uv {
                    w.write_u16::<LittleEndian>(*v)?;
                }
            }
            if let Some(dup) = &face.dup_uv {
                for v in dup {
                    w.write_u16::<LittleEndian>(*v)?;
                }
            }
            if let Some(n) = face.norm {
                w.write_u16::<LittleEndian>(n)?;
            }

            for vert in &face.verts {
                if let Some(vx) = &vert.xyz {
                    w.write_u16::<LittleEndian>(vx[0])?;
                    w.write_u16::<LittleEndian>(vx[1])?;
                }
                if let Some(vu) = &vert.uv {
                    w.write_u16::<LittleEndian>(vu[0])?;
                    w.write_u16::<LittleEndian>(vu[1])?;
                }
                if let Some(vn) = vert.norm {
                    w.write_u16::<LittleEndian>(vn)?;
                }
            }

            for dup in &face.dup_verts {
                if let Some(vu) = &dup.uv {
                    w.write_u16::<LittleEndian>(vu[0])?;
                    w.write_u16::<LittleEndian>(vu[1])?;
                }
            }
        }
    }

    if let OctreeNode::LodCube { children, .. } = node {
        for child in children.as_ref() {
            write_cube(child, w)?;
        }
    }

    Ok(())
}
