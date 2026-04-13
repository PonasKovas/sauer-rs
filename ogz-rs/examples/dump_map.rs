//! Reads a `.ogz` map file (gzip-compressed), parses it, and dumps structured
//! JSON to `mapdumps/<name>.json` in the same format as the reference C++ tool.
//!
//! Usage: `cargo run --example dump_map -- <path/to/map.ogz>`

use std::{fs, io::{self, Read}, path::Path};

use flate2::read::GzDecoder;
use serde::{Serialize, Serializer, ser::{SerializeMap, SerializeSeq}};
use ogz_rs::{
    Map,
    types::{MapVariableValue, OctreeNode, SurfaceInfo, VSlot, VSlotData},
};

fn main() -> io::Result<()> {
    let args: Vec<String> = std::env::args().collect();
    if args.len() != 2 {
        eprintln!("Usage: {} <path/to/map.ogz>", args[0]);
        std::process::exit(1);
    }
    let path = Path::new(&args[1]);
    let stem = path.file_stem().unwrap_or_default().to_string_lossy();

    let compressed = fs::read(path)?;
    let mut decompressed = Vec::new();
    let mut decoder = GzDecoder::new(compressed.as_slice());
    if let Err(e) = decoder.read_to_end(&mut decompressed) {
        let msg = e.to_string();
        if !msg.contains("checksum") && !msg.contains("corrupt gzip") {
            decompressed.clear();
            let start = gzip_deflate_offset(&compressed)
                .ok_or_else(|| io::Error::new(io::ErrorKind::InvalidData, "not a gzip file"))?;
            flate2::read::DeflateDecoder::new(&compressed[start..])
                .read_to_end(&mut decompressed)
                .map_err(|e| io::Error::new(io::ErrorKind::InvalidData, e.to_string()))?;
        }
    }

    let map = ogz_rs::parse(&decompressed)
        .map_err(|e| io::Error::new(io::ErrorKind::InvalidData, e.to_string()))?;

    let out_path = format!("mapdumps/{}.json", stem);
    let file = fs::File::create(&out_path)?;
    let writer = io::BufWriter::new(file);
    serde_json::to_writer(writer, &MapDump(&map))
        .map_err(|e| io::Error::new(io::ErrorKind::Other, e.to_string()))?;
    eprintln!("Wrote {}", out_path);
    Ok(())
}

// ─── helpers ──────────────────────────────────────────────────────────────────

/// Serializes a f32, replacing non-finite values with 0.0 (matching C++ finitef).
/// Uses 6 decimal places to match C++ `printf("%f", ...)` output.
struct FiniteF32(f32);
impl Serialize for FiniteF32 {
    fn serialize<S: Serializer>(&self, s: S) -> Result<S::Ok, S::Error> {
        let v = if self.0.is_finite() { self.0 as f64 } else { 0.0_f64 };
        // C++ dumps floats with %f (6 decimal places). Parse back to f64 to get the
        // same numeric value so JSON comparison works correctly.
        let n: f64 = format!("{:.6}", v).parse().unwrap();
        s.serialize_f64(n)
    }
}

/// Serializes a byte slice as a JSON array of unsigned integers.
struct ByteArray<'a>(&'a [u8]);
impl Serialize for ByteArray<'_> {
    fn serialize<S: Serializer>(&self, s: S) -> Result<S::Ok, S::Error> {
        let mut seq = s.serialize_seq(Some(self.0.len()))?;
        for &b in self.0 { seq.serialize_element(&b)?; }
        seq.end()
    }
}

fn lightmap_bpp(lm_type: u8) -> usize {
    if (lm_type & 0x10) != 0 && (lm_type & 0x0F) != 2 { 4 } else { 3 }
}

fn fnv1a_hash(data: &[u8]) -> u32 {
    let mut h = 2166136261u32;
    for &b in data { h ^= b as u32; h = h.wrapping_mul(16777619); }
    h
}

fn vslot_changed_mask(d: &VSlotData) -> i32 {
    let mut m = 0i32;
    if !d.shader_params.is_empty() { m |= 1 << 0; }
    if d.scale.is_some()           { m |= 1 << 1; }
    if d.rotation.is_some()        { m |= 1 << 2; }
    if d.offset.is_some()          { m |= 1 << 3; }
    if d.scroll.is_some()          { m |= 1 << 4; }
    if d.layer.is_some()           { m |= 1 << 5; }
    if d.alpha.is_some()           { m |= 1 << 6; }
    if d.color.is_some()           { m |= 1 << 7; }
    m
}

fn gzip_deflate_offset(data: &[u8]) -> Option<usize> {
    if data.len() < 10 || data[0] != 0x1f || data[1] != 0x8b { return None; }
    let flags = data[3];
    let mut pos = 10usize;
    if flags & 0x04 != 0 {
        if pos + 2 > data.len() { return None; }
        pos += 2 + u16::from_le_bytes([data[pos], data[pos+1]]) as usize;
    }
    if flags & 0x08 != 0 { while pos < data.len() && data[pos] != 0 { pos += 1; } pos += 1; }
    if flags & 0x10 != 0 { while pos < data.len() && data[pos] != 0 { pos += 1; } pos += 1; }
    if flags & 0x02 != 0 { pos += 2; }
    if pos > data.len() { return None; }
    Some(pos)
}

// ─── top-level ────────────────────────────────────────────────────────────────

struct MapDump<'a>(&'a Map);

impl Serialize for MapDump<'_> {
    fn serialize<S: Serializer>(&self, s: S) -> Result<S::Ok, S::Error> {
        let map = self.0;
        let mut m = s.serialize_map(Some(6))?;
        m.serialize_entry("worldsize", &map.world_size)?;
        m.serialize_entry("vars",      &VarsDump(&map.variables))?;
        m.serialize_entry("entities",  &EntitiesDump(&map.entities))?;
        m.serialize_entry("vslots",    &VSlotsDump(&map.vslots))?;
        m.serialize_entry("lightmaps", &LightmapsDump(&map.lightmaps))?;
        m.serialize_entry("octree",    &OctreeRootDump(map))?;
        m.end()
    }
}

// ─── vars ─────────────────────────────────────────────────────────────────────

struct VarsDump<'a>(&'a [ogz_rs::types::MapVariable]);

impl Serialize for VarsDump<'_> {
    fn serialize<S: Serializer>(&self, s: S) -> Result<S::Ok, S::Error> {
        let mut m = s.serialize_map(Some(self.0.len()))?;
        for var in self.0 {
            let key = var.name.to_string();
            match &var.value {
                MapVariableValue::Int(v)   => m.serialize_entry(&key, v)?,
                MapVariableValue::Float(v) => m.serialize_entry(&key, &FiniteF32(*v))?,
                MapVariableValue::String(v)=> m.serialize_entry(&key, &ByteArray(v.as_bytes()))?,
            }
        }
        m.end()
    }
}

// ─── entities ─────────────────────────────────────────────────────────────────

struct EntitiesDump<'a>(&'a [ogz_rs::types::Entity]);

impl Serialize for EntitiesDump<'_> {
    fn serialize<S: Serializer>(&self, s: S) -> Result<S::Ok, S::Error> {
        let mut seq = s.serialize_seq(Some(self.0.len()))?;
        for e in self.0 { seq.serialize_element(&EntityDump(e))?; }
        seq.end()
    }
}

struct EntityDump<'a>(&'a ogz_rs::types::Entity);

impl Serialize for EntityDump<'_> {
    fn serialize<S: Serializer>(&self, s: S) -> Result<S::Ok, S::Error> {
        let e = self.0;
        let mut m = s.serialize_map(Some(9))?;
        m.serialize_entry("x",     &FiniteF32(e.position.x))?;
        m.serialize_entry("y",     &FiniteF32(e.position.y))?;
        m.serialize_entry("z",     &FiniteF32(e.position.z))?;
        m.serialize_entry("attr1", &e.attrs[0])?;
        m.serialize_entry("attr2", &e.attrs[1])?;
        m.serialize_entry("attr3", &e.attrs[2])?;
        m.serialize_entry("attr4", &e.attrs[3])?;
        m.serialize_entry("attr5", &e.attrs[4])?;
        m.serialize_entry("type",  &e.entity_type.to_u8())?;
        m.end()
    }
}

// ─── vslots ───────────────────────────────────────────────────────────────────

struct VSlotsDump<'a>(&'a [VSlot]);

impl Serialize for VSlotsDump<'_> {
    fn serialize<S: Serializer>(&self, s: S) -> Result<S::Ok, S::Error> {
        let mut seq = s.serialize_seq(Some(self.0.len()))?;
        for (i, vs) in self.0.iter().enumerate() {
            seq.serialize_element(&VSlotDump(i as i32, vs))?;
        }
        seq.end()
    }
}

struct VSlotDump<'a>(i32, &'a VSlot);

impl Serialize for VSlotDump<'_> {
    fn serialize<S: Serializer>(&self, s: S) -> Result<S::Ok, S::Error> {
        let index = self.0;
        let (changed, layer, rotation, scale, offset, scroll, alpha, colorscale) = match self.1 {
            VSlot::Empty => (0i32, 0i32, 0i32, 1.0f32, [0i32,0], [0.0f32,0.0], [0.5f32,0.0], [1.0f32,1.0,1.0]),
            VSlot::Data(d) => (
                vslot_changed_mask(d),
                d.layer.unwrap_or(0),
                d.rotation.unwrap_or(0),
                d.scale.unwrap_or(1.0),
                d.offset.unwrap_or([0,0]),
                d.scroll.unwrap_or([0.0,0.0]),
                d.alpha.unwrap_or([0.5,0.0]),
                d.color.unwrap_or([1.0,1.0,1.0]),
            ),
        };
        let mut m = s.serialize_map(Some(9))?;
        m.serialize_entry("index",      &index)?;
        m.serialize_entry("changed",    &changed)?;
        m.serialize_entry("layer",      &layer)?;
        m.serialize_entry("rotation",   &rotation)?;
        m.serialize_entry("scale",      &FiniteF32(scale))?;
        m.serialize_entry("offset",     &[offset[0], offset[1]])?;
        m.serialize_entry("scroll",     &[FiniteF32(scroll[0]),     FiniteF32(scroll[1])])?;
        m.serialize_entry("alpha",      &[FiniteF32(alpha[0]),      FiniteF32(alpha[1])])?;
        m.serialize_entry("colorscale", &[FiniteF32(colorscale[0]), FiniteF32(colorscale[1]), FiniteF32(colorscale[2])])?;
        m.end()
    }
}

// ─── lightmaps ────────────────────────────────────────────────────────────────

struct LightmapsDump<'a>(&'a [ogz_rs::types::Lightmap]);

impl Serialize for LightmapsDump<'_> {
    fn serialize<S: Serializer>(&self, s: S) -> Result<S::Ok, S::Error> {
        let mut seq = s.serialize_seq(Some(self.0.len()))?;
        for lm in self.0 { seq.serialize_element(&LightmapDump(lm))?; }
        seq.end()
    }
}

struct LightmapDump<'a>(&'a ogz_rs::types::Lightmap);

impl Serialize for LightmapDump<'_> {
    fn serialize<S: Serializer>(&self, s: S) -> Result<S::Ok, S::Error> {
        let lm = self.0;
        let bpp = lightmap_bpp(lm.lm_type);
        let datasize = bpp * 512 * 512;
        let hash = fnv1a_hash(&lm.data[..datasize.min(lm.data.len())]);
        let (unlitx, unlity): (i32, i32) = match lm.unlit_coords {
            Some([x, y]) => (x as i32, y as i32),
            None         => (-1, -1),
        };
        let mut m = s.serialize_map(Some(6))?;
        m.serialize_entry("type",      &lm.lm_type)?;
        m.serialize_entry("bpp",       &bpp)?;
        m.serialize_entry("unlitx",    &unlitx)?;
        m.serialize_entry("unlity",    &unlity)?;
        m.serialize_entry("datasize",  &datasize)?;
        m.serialize_entry("data_hash", &hash)?;
        m.end()
    }
}

// ─── octree ───────────────────────────────────────────────────────────────────

const R: [usize; 3] = [1, 2, 0];
const C: [usize; 3] = [2, 0, 1];
const SOLID_EDGES: [u8; 12] = [0x80; 12];
const EMPTY_EDGES: [u8; 12] = [0x00; 12];

struct OctreeRootDump<'a>(&'a Map);

impl Serialize for OctreeRootDump<'_> {
    fn serialize<S: Serializer>(&self, s: S) -> Result<S::Ok, S::Error> {
        let map = self.0;
        match &map.octree_root {
            OctreeNode::Children { children, .. } =>
                ChildrenDump { children, co: [0,0,0], size: map.world_size }.serialize(s),
            other =>
                NodeDump { node: other, co: [0,0,0], size: map.world_size }.serialize(s),
        }
    }
}

struct ChildrenDump<'a> {
    children: &'a [OctreeNode; 8],
    co:       [i32; 3],
    size:     i32,
}

impl Serialize for ChildrenDump<'_> {
    fn serialize<S: Serializer>(&self, s: S) -> Result<S::Ok, S::Error> {
        let half = self.size / 2;
        let mut seq = s.serialize_seq(Some(8))?;
        for (i, child) in self.children.iter().enumerate() {
            let child_co = [
                self.co[0] + ((i & 1)      as i32) * half,
                self.co[1] + (((i>>1) & 1) as i32) * half,
                self.co[2] + (((i>>2) & 1) as i32) * half,
            ];
            seq.serialize_element(&NodeDump { node: child, co: child_co, size: half })?;
        }
        seq.end()
    }
}

struct NodeDump<'a> {
    node: &'a OctreeNode,
    co:   [i32; 3],
    size: i32,
}

impl Serialize for NodeDump<'_> {
    fn serialize<S: Serializer>(&self, s: S) -> Result<S::Ok, S::Error> {
        let (faces, textures, material, merged, surfaces, edges, children) = match self.node {
            OctreeNode::Children { children, material, merged } =>
                ([0u32,0,0], [1u16,1,1,1,1,1], *material, *merged, None, None, Some(children.as_ref())),
            OctreeNode::Empty { textures, material, merged, surfaces } =>
                ([0u32,0,0], *textures, *material, *merged, surfaces.as_deref(), Some(&EMPTY_EDGES as &[u8;12]), None),
            OctreeNode::Solid { textures, material, merged, surfaces } =>
                ([0x80808080u32,0x80808080,0x80808080], *textures, *material, *merged, surfaces.as_deref(), Some(&SOLID_EDGES as &[u8;12]), None),
            OctreeNode::Normal { textures, edges, material, merged, surfaces } => {
                let faces = [
                    u32::from_le_bytes([edges[0],edges[1],edges[2],edges[3]]),
                    u32::from_le_bytes([edges[4],edges[5],edges[6],edges[7]]),
                    u32::from_le_bytes([edges[8],edges[9],edges[10],edges[11]]),
                ];
                (faces, *textures, *material, *merged, surfaces.as_deref(), Some(edges as &[u8;12]), None)
            }
            OctreeNode::LodCube { textures, children, material, merged, surfaces } =>
                ([0u32,0,0], *textures, *material, *merged, surfaces.as_deref(), Some(&SOLID_EDGES as &[u8;12]), Some(children.as_ref())),
        };

        let field_count = 4 + usize::from(surfaces.is_some()) + usize::from(children.is_some());
        let mut m = s.serialize_map(Some(field_count))?;
        m.serialize_entry("faces",    &faces)?;
        m.serialize_entry("texture",  &textures)?;
        m.serialize_entry("material", &material.unwrap_or(0))?;
        m.serialize_entry("merged",   &merged.unwrap_or(0))?;

        if let Some(surfs) = surfaces {
            let edges = edges.expect("surfaces without edges");
            m.serialize_entry("ext", &ExtDump { surfs, edges, co: self.co, size: self.size })?;
        }
        if let Some(ch) = children {
            m.serialize_entry("children", &ChildrenDump { children: ch, co: self.co, size: self.size })?;
        }
        m.end()
    }
}

struct ExtDump<'a> {
    surfs: &'a ogz_rs::types::NodeSurfaces,
    edges: &'a [u8; 12],
    co:    [i32; 3],
    size:  i32,
}

impl Serialize for ExtDump<'_> {
    fn serialize<S: Serializer>(&self, s: S) -> Result<S::Ok, S::Error> {
        // Precompute all verts up front so surface numverts/offsets are exact
        // (dedup may reduce the count below verts.len() for merge surfaces).
        let face_verts: [Vec<[u16; 6]>; 6] = std::array::from_fn(|orient| {
            match self.surfs[orient].as_ref() {
                Some(surf) => compute_surf_verts(surf, self.edges, orient, self.co, self.size),
                None => vec![],
            }
        });
        let mut m = s.serialize_map(Some(2))?;
        m.serialize_entry("surfaces", &PrecomputedSurfacesDump { surfs: self.surfs, face_verts: &face_verts })?;
        m.serialize_entry("verts",    &PrecomputedVertsDump { face_verts: &face_verts })?;
        m.end()
    }
}

struct PrecomputedSurfacesDump<'a> {
    surfs:      &'a ogz_rs::types::NodeSurfaces,
    face_verts: &'a [Vec<[u16; 6]>; 6],
}

impl Serialize for PrecomputedSurfacesDump<'_> {
    fn serialize<S: Serializer>(&self, s: S) -> Result<S::Ok, S::Error> {
        // Compute offsets from actual (deduped) primary vert counts.
        let primary_counts: [usize; 6] = std::array::from_fn(|i| {
            let fv = &self.face_verts[i];
            if fv.is_empty() { return 0; }
            let has_dup = self.surfs[i].as_ref().map_or(false, |s| s.has_dup_layer());
            if has_dup { fv.len() / 2 } else { fv.len() }
        });
        let mut offset = 0u8;
        let mut offsets = [0u8; 6];
        for i in 0..6 { offsets[i] = offset; offset += primary_counts[i] as u8; }

        let mut seq = s.serialize_seq(Some(6))?;
        for i in 0..6 {
            seq.serialize_element(&SurfaceEntryDump {
                surf: self.surfs[i].as_ref(),
                verts_offset: offsets[i],
                actual_primary: primary_counts[i],
            })?;
        }
        seq.end()
    }
}

struct SurfaceEntryDump<'a> {
    surf:           Option<&'a SurfaceInfo>,
    verts_offset:   u8,
    actual_primary: usize,
}

impl Serialize for SurfaceEntryDump<'_> {
    fn serialize<S: Serializer>(&self, s: S) -> Result<S::Ok, S::Error> {
        let mut m = s.serialize_map(Some(3))?;
        match self.surf {
            None => {
                m.serialize_entry("lmid",     &[0u8, 0u8])?;
                m.serialize_entry("verts",    &0u8)?;
                m.serialize_entry("numverts", &0u8)?;
            }
            Some(surf) => {
                let numverts = (self.actual_primary as u8 & 0x0F)
                    | if surf.layer_top    { 0x20 } else { 0 }
                    | if surf.layer_bottom { 0x40 } else { 0 }
                    | if surf.has_dup_layer() { 0x80 } else { 0 };
                m.serialize_entry("lmid",     &surf.lmid)?;
                m.serialize_entry("verts",    &self.verts_offset)?;
                m.serialize_entry("numverts", &numverts)?;
            }
        }
        m.end()
    }
}

struct PrecomputedVertsDump<'a> {
    face_verts: &'a [Vec<[u16; 6]>; 6],
}

impl Serialize for PrecomputedVertsDump<'_> {
    fn serialize<S: Serializer>(&self, s: S) -> Result<S::Ok, S::Error> {
        let mut seq = s.serialize_seq(None)?;
        for verts in self.face_verts {
            for &vert in verts {
                seq.serialize_element(&VertDump(vert))?;
            }
        }
        seq.end()
    }
}

struct VertDump([u16; 6]);

impl Serialize for VertDump {
    fn serialize<S: Serializer>(&self, s: S) -> Result<S::Ok, S::Error> {
        let [x,y,z,u,v,norm] = self.0;
        let mut m = s.serialize_map(Some(6))?;
        m.serialize_entry("x",    &x)?;
        m.serialize_entry("y",    &y)?;
        m.serialize_entry("z",    &z)?;
        m.serialize_entry("u",    &u)?;
        m.serialize_entry("v",    &v)?;
        m.serialize_entry("norm", &norm)?;
        m.end()
    }
}

// ─── surface vertex computation ───────────────────────────────────────────────

#[inline]
fn edge_get(edge: u8, hi: bool) -> i32 {
    if hi { (edge >> 4) as i32 } else { (edge & 0xF) as i32 }
}

#[inline]
fn gen_face_vert(edges: &[u8;12], x: usize, y: usize, z: usize) -> [i32;3] {
    [
        edge_get(edges[(z<<1)|y],     x != 0),
        edge_get(edges[4|(x<<1)|z],   y != 0),
        edge_get(edges[8|(y<<1)|x],   z != 0),
    ]
}

fn gen_face_verts(edges: &[u8;12], orient: usize) -> [[i32;3];4] {
    match orient {
        0 => [gen_face_vert(edges,0,1,1), gen_face_vert(edges,0,1,0), gen_face_vert(edges,0,0,0), gen_face_vert(edges,0,0,1)],
        1 => [gen_face_vert(edges,1,1,1), gen_face_vert(edges,1,0,1), gen_face_vert(edges,1,0,0), gen_face_vert(edges,1,1,0)],
        2 => [gen_face_vert(edges,1,0,1), gen_face_vert(edges,0,0,1), gen_face_vert(edges,0,0,0), gen_face_vert(edges,1,0,0)],
        3 => [gen_face_vert(edges,0,1,0), gen_face_vert(edges,0,1,1), gen_face_vert(edges,1,1,1), gen_face_vert(edges,1,1,0)],
        4 => [gen_face_vert(edges,0,0,0), gen_face_vert(edges,0,1,0), gen_face_vert(edges,1,1,0), gen_face_vert(edges,1,0,0)],
        5 => [gen_face_vert(edges,0,0,1), gen_face_vert(edges,1,0,1), gen_face_vert(edges,1,1,1), gen_face_vert(edges,0,1,1)],
        _ => unreachable!(),
    }
}

#[inline] fn cross3(a:[i32;3],b:[i32;3])->[i32;3]{[a[1]*b[2]-a[2]*b[1],a[2]*b[0]-a[0]*b[2],a[0]*b[1]-a[1]*b[0]]}
#[inline] fn sub3(a:[i32;3],b:[i32;3])->[i32;3]{[a[0]-b[0],a[1]-b[1],a[2]-b[2]]}
#[inline] fn dot3(a:[i32;3],b:[i32;3])->i32{a[0]*b[0]+a[1]*b[1]+a[2]*b[2]}

fn compute_surf_verts(surf: &SurfaceInfo, edges: &[u8;12], orient: usize, co: [i32;3], size: i32) -> Vec<[u16;6]> {
    let dim = orient >> 1;
    let vc  = C[dim];
    let vr  = R[dim];
    let vo  = [(co[0]&0xFFF)<<3, (co[1]&0xFFF)<<3, (co[2]&0xFFF)<<3];

    let vm = surf.verts_mask;
    let lv = surf.verts.len();
    let has_xyz     = (vm & 0x04) != 0;
    let has_uv      = (vm & 0x40) != 0;
    let has_norm    = (vm & 0x80) != 0;
    let compact_xyz = has_xyz  && (vm & 0x01) != 0 && lv == 4;
    let compact_uv  = has_uv   && (vm & 0x02) != 0 && lv == 4;
    let compact_norm= has_norm && (vm & 0x08) != 0;

    let vf = gen_face_verts(edges, orient);

    // Compute normal and plane bias (used for cdim when has_xyz).
    let (n, bias) = if has_xyz {
        let e1 = sub3(vf[1], vf[0]);
        let e2 = sub3(vf[2], vf[0]);
        let mut nv = cross3(e1, e2);
        if nv == [0,0,0] { nv = cross3(e2, sub3(vf[3], vf[0])); }
        let v0w = [vf[0][0]*size+vo[0], vf[0][1]*size+vo[1], vf[0][2]*size+vo[2]];
        (nv, -dot3(nv, v0w))
    } else {
        ([0i32;3], 0i32)
    };

    let cdim = |cv:i32,rv:i32|->i32 {
        if n[dim]!=0 { -(bias+n[vc]*cv+n[vr]*rv)/n[dim] } else { vo[dim] }
    };

    // Compute the 4 candidate positions and the set of included indices after
    // applying the C++ convertoldsurfaces deduplication logic.
    let pos4: [[i32;3]; 4];
    let included: Vec<usize>;

    if !has_xyz {
        // Non-merge: derive geometry from face edges using the C++ convex/vis test.
        let e1 = sub3(vf[1], vf[0]);
        let e2 = sub3(vf[2], vf[0]);
        let nn = cross3(e1, e2);
        let e3 = sub3(vf[0], vf[3]);
        let convex = dot3(e3, nn);
        let vis: usize = if convex != 0 { 3 }
            else if cross3(e3, e2) == [0,0,0] { if nn != [0,0,0] { 1 } else { 3 } }
            else if nn == [0,0,0] { 2 }
            else { 3 };
        let order = if convex < 0 { 1usize } else { 0 };
        let fv = |fi: usize| { let v = vf[fi&3]; [v[0]*size+vo[0], v[1]*size+vo[1], v[2]*size+vo[2]] };
        pos4 = [
            fv(order),
            if vis&1!=0 { fv(order+1) } else { fv(order) },
            fv(order+2),
            if vis&2!=0 { fv((order+3)&3) } else { fv(order) },
        ];
        included = (0..4usize).filter(|&k| k == 0 || (pos4[k] != pos4[0] && pos4[k] != pos4[k-1])).collect();
    } else if compact_xyz {
        let [c1,r1,c2,r2] = surf.xyz_bound.unwrap_or([0;4]).map(|v|v as i32);
        pos4 = [
            { let mut p=[0i32;3]; p[vc]=c1; p[vr]=r1; p[dim]=cdim(c1,r1); p },
            { let mut p=[0i32;3]; p[vc]=c1; p[vr]=r2; p[dim]=cdim(c1,r2); p },
            { let mut p=[0i32;3]; p[vc]=c2; p[vr]=r2; p[dim]=cdim(c2,r2); p },
            { let mut p=[0i32;3]; p[vc]=c2; p[vr]=r1; p[dim]=cdim(c2,r1); p },
        ];
        included = (0..4).collect();
    } else {
        // Per-vertex xy (merge coords, old format or modern per-vertex).
        let mut arr = [[0i32;3]; 4];
        for (k, v) in surf.verts.iter().enumerate().take(4) {
            let [cv,rv] = v.xy.unwrap_or([0,0]).map(|x|x as i32);
            arr[k][vc]=cv; arr[k][vr]=rv; arr[k][dim]=cdim(cv,rv);
        }
        pos4 = arr;
        included = (0..4usize).filter(|&k| k == 0 || (pos4[k] != pos4[0] && pos4[k] != pos4[k-1])).collect();
    }

    // UV and norm arrays indexed by k=0..3 (original, pre-dedup index).
    let mut uv: [[u16;2]; 4] = [[0,0]; 4];
    if has_uv {
        if compact_uv {
            let [u0,v0,u2,v2] = surf.uv_plane.unwrap_or([0;4]);
            let uo = ((vm&0x30)>>4) as usize;
            uv[uo]=[u0,v0]; uv[(uo+1)&3]=[u0,v2]; uv[(uo+2)&3]=[u2,v2]; uv[(uo+3)&3]=[u2,v0];
        } else {
            for (k,v) in surf.verts.iter().enumerate().take(4) { uv[k]=v.uv.unwrap_or([0,0]); }
        }
    }

    let mut norm: [u16; 4] = [0; 4];
    if has_norm {
        if compact_norm {
            let base = surf.base_norm.unwrap_or(0);
            norm = [base; 4];
        } else {
            for (k,v) in surf.verts.iter().enumerate().take(4) { norm[k]=v.norm.unwrap_or(0); }
        }
    }

    // Build primary verts from included (deduped) indices.
    let mut result = Vec::with_capacity(included.len() * 2);
    for &k in &included {
        let [x,y,z] = pos4[k].map(|v| v as u16);
        result.push([x,y,z, uv[k][0],uv[k][1], norm[k]]);
    }

    // Dup layer verts (LAYER_DUP), same positions + different UV.
    if !surf.verts_dup_uv.is_empty() || surf.uv_plane_dup.is_some() {
        let mut uv2: [[u16;2]; 4] = [[0,0]; 4];
        if let Some([u0,v0,u2,v2]) = surf.uv_plane_dup {
            let uo = ((vm&0x30)>>4) as usize;
            uv2[uo]=[u0,v0]; uv2[(uo+1)&3]=[u0,v2]; uv2[(uo+2)&3]=[u2,v2]; uv2[(uo+3)&3]=[u2,v0];
        } else {
            for (k,p) in surf.verts_dup_uv.iter().enumerate().take(4) { uv2[k]=*p; }
        }
        for &k in &included {
            let [x,y,z] = pos4[k].map(|v| v as u16);
            result.push([x,y,z, uv2[k][0],uv2[k][1], norm[k]]);
        }
    }

    result
}
