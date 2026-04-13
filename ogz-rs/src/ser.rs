use crate::data::*;
use crate::error::OgzError;
use flate2::write::GzEncoder;
use flate2::Compression;
use std::io::Write;

// Helper trait to write Little-Endian
trait WriteExt: Write {
    fn write_u8(&mut self, n: u8) -> std::io::Result<()> {
        self.write_all(&[n])
    }
    fn write_u16_le(&mut self, n: u16) -> std::io::Result<()> {
        self.write_all(&n.to_le_bytes())
    }
    fn write_i16_le(&mut self, n: i16) -> std::io::Result<()> {
        self.write_all(&n.to_le_bytes())
    }
    fn write_i32_le(&mut self, n: i32) -> std::io::Result<()> {
        self.write_all(&n.to_le_bytes())
    }
    fn write_f32_le(&mut self, n: f32) -> std::io::Result<()> {
        self.write_all(&n.to_le_bytes())
    }
}
impl<W: Write + ?Sized> WriteExt for W {}

pub fn serialize_map(map: &OgzMap) -> Result<Vec<u8>, OgzError> {
    let mut encoder = GzEncoder::new(Vec::new(), Compression::default());

    // Header
    encoder.write_all(b"OCTA")?;
    encoder.write_i32_le(33)?; // Hardcoded version 33
    encoder.write_i32_le(28)?; // headersize
    encoder.write_i32_le(map.world_size)?;
    encoder.write_i32_le(map.entities.len() as i32)?;
    encoder.write_i32_le(map.pvs.len() as i32)?;
    encoder.write_i32_le(map.lightmaps.len() as i32)?;

    match &map.config {
        MapConfig::Modern { variables } => {
            encoder.write_i32_le(map.blendmap_resolution)?;
            encoder.write_i32_le(variables.len() as i32)?;

            let vslots_count = map.vslots.len();
            encoder.write_i32_le(vslots_count as i32)?; // numvslots logically

            for var in variables {
                match var {
                    Variable::Int { name, value } => {
                        encoder.write_u8(0)?;
                        encoder.write_u16_le(name.0.len() as u16)?;
                        encoder.write_all(&name.0)?;
                        encoder.write_i32_le(*value)?;
                    }
                    Variable::Float { name, value } => {
                        encoder.write_u8(1)?;
                        encoder.write_u16_le(name.0.len() as u16)?;
                        encoder.write_all(&name.0)?;
                        encoder.write_f32_le(*value)?;
                    }
                    Variable::String { name, value } => {
                        encoder.write_u8(2)?;
                        encoder.write_u16_le(name.0.len() as u16)?;
                        encoder.write_all(&name.0)?;
                        encoder.write_u16_le(value.0.len() as u16)?;
                        encoder.write_all(&value.0)?;
                    }
                }
            }
        }
        MapConfig::Legacy { .. } => return Err(OgzError::CannotSerializeLegacy),
    }

    if let Some(ref gt) = map.game_type {
        encoder.write_u8(gt.0.len() as u8)?;
        encoder.write_all(&gt.0)?;
        encoder.write_u8(0)?; // Null terminator
        encoder.write_u16_le(map.eif)?;
        encoder.write_u16_le(map.extra_game_data.len() as u16)?;
        encoder.write_all(&map.extra_game_data)?;
    } else {
        encoder.write_u8(0)?; // fallback gametype logic or empty
        encoder.write_u8(0)?;
        encoder.write_u16_le(0)?; // eif
        encoder.write_u16_le(0)?; // extrasize
    }

    encoder.write_u16_le(map.mru_textures.len() as u16)?;
    for tex in &map.mru_textures {
        encoder.write_u16_le(*tex)?;
    }

    for ent in &map.entities {
        for p in &ent.position {
            encoder.write_f32_le(*p)?;
        }
        for a in &ent.attrs {
            encoder.write_i16_le(*a)?;
        }
        encoder.write_u8(ent.ent_type)?;
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
            encoder.write_i32_le(-skip)?;
        } else {
            let slot = map.vslots[i].as_ref().unwrap();
            let mut changed = 0i32;
            if !slot.params.is_empty() {
                changed |= 1 << 0;
            }
            if slot.scale.is_some() {
                changed |= 1 << 1;
            }
            if slot.rotation.is_some() {
                changed |= 1 << 2;
            }
            if slot.offset.is_some() {
                changed |= 1 << 3;
            }
            if slot.scroll.is_some() {
                changed |= 1 << 4;
            }
            if slot.layer.is_some() {
                changed |= 1 << 5;
            }
            if slot.alpha.is_some() {
                changed |= 1 << 6;
            }
            if slot.color.is_some() {
                changed |= 1 << 7;
            }

            encoder.write_i32_le(changed)?;
            encoder.write_i32_le(slot.prev)?;

            if !slot.params.is_empty() {
                encoder.write_u16_le(slot.params.len() as u16)?;
                for p in &slot.params {
                    encoder.write_u16_le(p.name.0.len() as u16)?;
                    encoder.write_all(&p.name.0)?;
                    for f in &p.values {
                        encoder.write_f32_le(*f)?;
                    }
                }
            }
            if let Some(s) = slot.scale {
                encoder.write_f32_le(s)?;
            }
            if let Some(r) = slot.rotation {
                encoder.write_i32_le(r)?;
            }
            if let Some(o) = slot.offset {
                encoder.write_i32_le(o.0)?;
                encoder.write_i32_le(o.1)?;
            }
            if let Some(sc) = slot.scroll {
                encoder.write_f32_le(sc.0)?;
                encoder.write_f32_le(sc.1)?;
            }
            if let Some(l) = slot.layer {
                encoder.write_i32_le(l)?;
            }
            if let Some(a) = slot.alpha {
                encoder.write_f32_le(a.0)?;
                encoder.write_f32_le(a.1)?;
            }
            if let Some(c) = slot.color {
                encoder.write_f32_le(c.0)?;
                encoder.write_f32_le(c.1)?;
                encoder.write_f32_le(c.2)?;
            }
            i += 1;
        }
    }

    write_cube(&map.octree, &mut encoder)?;

    for lm in &map.lightmaps {
        encoder.write_u8(lm.map_type)?;
        if (lm.map_type & 0x80) != 0 {
            if let Some((ux, uy)) = lm.unlit {
                encoder.write_u16_le(ux)?;
                encoder.write_u16_le(uy)?;
            } else {
                encoder.write_u16_le(0)?;
                encoder.write_u16_le(0)?; // Fallback safe empty
            }
        }
        encoder.write_all(&lm.data)?;
    }

    if map.pvs.len() > 0 {
        encoder.write_all(&map.pvs)?;
    }
    if let Some(ref bdata) = map.blendmap_data {
        encoder.write_all(bdata)?;
    }

    Ok(encoder.finish()?)
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
        w.write_u16_le(*tex)?;
    }

    if let Some(mat) = mod_surf.material {
        w.write_u16_le(mat)?;
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
                    w.write_u16_le(*v)?;
                }
            }
            if let Some(uv) = &face.uv {
                for v in uv {
                    w.write_u16_le(*v)?;
                }
            }
            if let Some(dup) = &face.dup_uv {
                for v in dup {
                    w.write_u16_le(*v)?;
                }
            }
            if let Some(n) = face.norm {
                w.write_u16_le(n)?;
            }

            for vert in &face.verts {
                if let Some(vx) = &vert.xyz {
                    w.write_u16_le(vx[0])?;
                    w.write_u16_le(vx[1])?;
                }
                if let Some(vu) = &vert.uv {
                    w.write_u16_le(vu[0])?;
                    w.write_u16_le(vu[1])?;
                }
                if let Some(vn) = vert.norm {
                    w.write_u16_le(vn)?;
                }
            }

            for dup in &face.dup_verts {
                if let Some(vu) = &dup.uv {
                    w.write_u16_le(vu[0])?;
                    w.write_u16_le(vu[1])?;
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
