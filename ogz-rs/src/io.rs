use crate::{error::OgzError, types::*};

// ── safety limits ─────────────────────────────────────────────────────────────

/// Safety limit for u16-prefixed strings (well below u16::MAX to prevent large allocs).
const MAX_STRING_LEN: usize = 4_096;
/// Maximum length of the game-type string (stored as u8 on disk, so hard max is 255).
const MAX_GAME_TYPE_LEN: usize = 255;
const MAX_VARS: i32 = 65_536;
const MAX_VSLOTS: i32 = 65_536;
const MAX_ENTS: i32 = 10_000;
const MAX_LIGHTMAPS: i32 = 8_192;
const MAX_SHADER_PARAMS: u16 = 1_024;
const MAX_PVS_BLOCKS: i32 = 65_536;
const MAX_PVS_BUFFER: u32 = 64 * 1024 * 1024; // 64 MiB
const MAX_WATER_PLANES: u32 = 1_024;
const MAX_OCTREE_DEPTH: u32 = 20; // world side up to 2^20
const MAX_VIEWCELLS_DEPTH: u32 = 1024;
const MAX_BLENDMAP_DEPTH: u32 = 20;

// ── Reader ────────────────────────────────────────────────────────────────────

/// A zero-copy cursor over a byte slice.  Inspired by nom's combinator style but
/// stateful — each `read_*` call advances the internal position.
pub(crate) struct Reader<'a> {
    data: &'a [u8],
    pos: usize,
}

impl<'a> Reader<'a> {
    pub fn new(data: &'a [u8]) -> Self {
        Reader { data, pos: 0 }
    }

    fn take(&mut self, n: usize) -> Result<&'a [u8], OgzError> {
        let end = self.pos.checked_add(n).ok_or(OgzError::Incomplete)?;
        if end > self.data.len() {
            return Err(OgzError::Incomplete);
        }
        let slice = &self.data[self.pos..end];
        self.pos = end;
        Ok(slice)
    }

    pub fn read_u8(&mut self) -> Result<u8, OgzError> {
        Ok(self.take(1)?[0])
    }

    pub fn read_u16(&mut self) -> Result<u16, OgzError> {
        Ok(u16::from_le_bytes(self.take(2)?.try_into().unwrap()))
    }

    pub fn read_i16(&mut self) -> Result<i16, OgzError> {
        Ok(self.read_u16()? as i16)
    }

    pub fn read_u32(&mut self) -> Result<u32, OgzError> {
        Ok(u32::from_le_bytes(self.take(4)?.try_into().unwrap()))
    }

    pub fn read_i32(&mut self) -> Result<i32, OgzError> {
        Ok(self.read_u32()? as i32)
    }

    pub fn read_f32(&mut self) -> Result<f32, OgzError> {
        Ok(f32::from_le_bytes(self.take(4)?.try_into().unwrap()))
    }

    pub fn read_bytes(&mut self, n: usize) -> Result<&'a [u8], OgzError> {
        self.take(n)
    }

    pub fn read_vec3(&mut self) -> Result<mint::Vector3<f32>, OgzError> {
        Ok(mint::Vector3 { x: self.read_f32()?, y: self.read_f32()?, z: self.read_f32()? })
    }

    /// u16-length-prefixed string (no null terminator).
    pub fn read_string_u16(&mut self) -> Result<SauerString, OgzError> {
        let len = self.read_u16()? as usize;
        if len > MAX_STRING_LEN {
            return Err(OgzError::StringTooLong { length: len, limit: MAX_STRING_LEN });
        }
        let bytes = self.take(len)?;
        Ok(SauerString::from_bytes(bytes.to_vec()))
    }

    /// u8-length game-type string.  The length excludes the null terminator that
    /// follows it on disk.
    pub fn read_game_type_string(&mut self) -> Result<SauerString, OgzError> {
        let len = self.read_u8()? as usize;
        // MAX_GAME_TYPE_LEN == u8::MAX so this is only reachable via a future change.
        if len > MAX_GAME_TYPE_LEN {
            return Err(OgzError::StringTooLong { length: len, limit: MAX_GAME_TYPE_LEN });
        }
        let bytes = self.take(len + 1)?; // +1: consume the null terminator
        Ok(SauerString::from_bytes(bytes[..len].to_vec()))
    }

    pub fn skip(&mut self, n: usize) -> Result<(), OgzError> {
        self.take(n).map(|_| ())
    }
}

// ── Writer ────────────────────────────────────────────────────────────────────

/// A growable byte buffer.  Each `write_*` call appends little-endian bytes.
pub(crate) struct Writer {
    buf: Vec<u8>,
}

impl Writer {
    pub fn new() -> Self {
        Writer { buf: Vec::new() }
    }

    pub fn into_vec(self) -> Vec<u8> {
        self.buf
    }

    pub fn write_u8(&mut self, v: u8) {
        self.buf.push(v);
    }

    pub fn write_u16(&mut self, v: u16) {
        self.buf.extend_from_slice(&v.to_le_bytes());
    }

    pub fn write_i16(&mut self, v: i16) {
        self.buf.extend_from_slice(&v.to_le_bytes());
    }

    pub fn write_u32(&mut self, v: u32) {
        self.buf.extend_from_slice(&v.to_le_bytes());
    }

    pub fn write_i32(&mut self, v: i32) {
        self.buf.extend_from_slice(&v.to_le_bytes());
    }

    pub fn write_f32(&mut self, v: f32) {
        self.buf.extend_from_slice(&v.to_le_bytes());
    }

    pub fn write_bytes(&mut self, v: &[u8]) {
        self.buf.extend_from_slice(v);
    }

    pub fn write_vec3(&mut self, v: &mint::Vector3<f32>) {
        self.write_f32(v.x);
        self.write_f32(v.y);
        self.write_f32(v.z);
    }

    /// Write a u16-length-prefixed string (no null terminator).
    pub fn write_string_u16(&mut self, s: &[u8]) {
        self.write_u16(s.len() as u16);
        self.write_bytes(s);
    }

    /// Write a u8-length game-type string followed by a null terminator.
    pub fn write_game_type_string(&mut self, s: &[u8]) {
        self.write_u8(s.len() as u8);
        self.write_bytes(s);
        self.write_u8(0); // null terminator required by spec
    }
}

// ── Header ────────────────────────────────────────────────────────────────────

const MAGIC: &[u8; 4] = b"OCTA";
pub const VERSION: i32 = 33;
const HEADER_SIZE: i32 = 40;

/// Reads the map header and returns:
/// `(world_size, num_vars, num_vslots, num_ents, num_pvs, num_lightmaps, blendmap_flag, version)`
///
/// Handles both the modern `octaheader` (v29+) and the legacy `compatheader` (v≤28).
pub(crate) fn read_header(
    r: &mut Reader,
) -> Result<(i32, i32, i32, i32, i32, i32, i32, i32), OgzError> {
    let magic = r.read_bytes(4)?;
    if magic != MAGIC {
        let mut found = [0u8; 4];
        found.copy_from_slice(magic);
        return Err(OgzError::InvalidMagic { found });
    }
    let version = r.read_i32()?;
    if version < 1 || version > VERSION {
        return Err(OgzError::UnsupportedVersion { found: version });
    }
    let _header_size = r.read_i32()?;
    let world_size   = r.read_i32()?;
    let num_ents     = r.read_i32()?;
    let num_pvs      = r.read_i32()?;
    let num_lightmaps = r.read_i32()?;
    // 7 ints (28 bytes) read above.

    let (blendmap, num_vars, num_vslots) = if version <= 28 {
        // compatheader: 196 bytes total.  After the first 28 bytes, 168 more follow.
        // Layout of those 168 bytes:
        //   [0..4]  lightprecision (int)
        //   [4..8]  lighterror     (int)
        //   [8..12] lightlod       (int)
        //   [12]    ambient        (uchar)
        //   [13..16] watercolour   (uchar[3])
        //   [16]    blendmap       (uchar)  ← this is what we need
        //   ...     (remainder unused here)
        let compat = r.read_bytes(168)?;
        let blendmap = compat[16] as i32;
        (blendmap, 0i32, 0i32)
    } else if version == 29 {
        // v29: reads blendmap + numvars (8 bytes); numvslots is absent → 0.
        let blendmap = r.read_i32()?;
        let num_vars = r.read_i32()?;
        (blendmap, num_vars, 0i32)
    } else {
        // v30+: reads blendmap + numvars + numvslots (12 bytes).
        let blendmap = r.read_i32()?;
        let num_vars = r.read_i32()?;
        let num_vslots = r.read_i32()?;
        (blendmap, num_vars, num_vslots)
    };

    Ok((world_size, num_vars, num_vslots, num_ents, num_pvs, num_lightmaps, blendmap, version))
}

pub(crate) fn write_header(w: &mut Writer, map: &Map) {
    let num_pvs = map.pvs.as_ref().map_or(0, |p| p.block_lengths.len() as i32);
    w.write_bytes(MAGIC);
    w.write_i32(VERSION);
    w.write_i32(HEADER_SIZE);
    w.write_i32(map.world_size);
    w.write_i32(map.entities.len() as i32);
    w.write_i32(num_pvs);
    w.write_i32(map.lightmaps.len() as i32);
    w.write_i32(if map.blendmap.is_some() { 1 } else { 0 });
    w.write_i32(map.variables.len() as i32);
    w.write_i32(map.vslots.len() as i32);
}

// ── Map variables ─────────────────────────────────────────────────────────────

pub(crate) fn read_variables(r: &mut Reader, count: i32) -> Result<Vec<MapVariable>, OgzError> {
    if count < 0 || count > MAX_VARS {
        return Err(OgzError::CountOutOfRange {
            field: "num_vars",
            value: count as i64,
            max: MAX_VARS as i64,
        });
    }
    let mut vars = Vec::with_capacity(count as usize);
    for _ in 0..count {
        let var_type = r.read_u8()?;
        let name = r.read_string_u16()?;
        let value = match var_type {
            0 => MapVariableValue::Int(r.read_i32()?),
            1 => MapVariableValue::Float(r.read_f32()?),
            2 => MapVariableValue::String(r.read_string_u16()?),
            _ => return Err(OgzError::UnknownVarType(var_type)),
        };
        vars.push(MapVariable { name, value });
    }
    Ok(vars)
}

pub(crate) fn write_variables(w: &mut Writer, vars: &[MapVariable]) {
    for var in vars {
        match &var.value {
            MapVariableValue::Int(v) => {
                w.write_u8(0);
                w.write_string_u16(var.name.as_bytes());
                w.write_i32(*v);
            }
            MapVariableValue::Float(v) => {
                w.write_u8(1);
                w.write_string_u16(var.name.as_bytes());
                w.write_f32(*v);
            }
            MapVariableValue::String(s) => {
                w.write_u8(2);
                w.write_string_u16(var.name.as_bytes());
                w.write_string_u16(s.as_bytes());
            }
        }
    }
}

// ── Game type / extras ────────────────────────────────────────────────────────

pub(crate) fn read_game_type(r: &mut Reader, version: i32) -> Result<GameTypeData, OgzError> {
    let game_type;
    let extra_entity_info_size;
    let extra_game_data;

    if version >= 16 {
        game_type = GameType::from(r.read_game_type_string()?);
        extra_entity_info_size = r.read_u16()?;
        let extra_size = r.read_u16()?;
        extra_game_data = r.read_bytes(extra_size as usize)?.to_vec();
    } else {
        game_type = GameType::Fps;
        extra_entity_info_size = 0;
        extra_game_data = vec![];
    }

    // Texture MRU: v<14 stores 256 u8 indices; v≥14 stores u16 count + u16 values.
    let texture_mru = if version < 14 {
        let bytes = r.read_bytes(256)?;
        bytes.iter().map(|&b| b as u16).collect()
    } else {
        let mru_count = r.read_u16()?;
        let mut mru = Vec::with_capacity(mru_count as usize);
        for _ in 0..mru_count {
            mru.push(r.read_u16()?);
        }
        mru
    };

    Ok(GameTypeData { game_type, extra_entity_info_size, extra_game_data, texture_mru })
}

pub(crate) fn write_game_type(w: &mut Writer, gt: &GameTypeData) {
    w.write_game_type_string(gt.game_type.as_bytes());
    w.write_u16(gt.extra_entity_info_size);
    w.write_u16(gt.extra_game_data.len() as u16);
    w.write_bytes(&gt.extra_game_data);
    w.write_u16(gt.texture_mru.len() as u16);
    for &idx in &gt.texture_mru {
        w.write_u16(idx);
    }
}

// ── Entities ──────────────────────────────────────────────────────────────────

/// Apply entity-type and attribute fixups for maps saved by older engine versions.
///
/// Mirrors the C++ `fixent()` function in worldio.cpp.
fn fixent(entity_type: &mut u8, attrs: &mut [i16; 5], version: i32) {
    // Entity type table expanded incrementally across versions.
    if version <= 10 && *entity_type >= 7 { *entity_type += 1; }
    if version <= 12 && *entity_type >= 8 { *entity_type += 1; }
    if version <= 14 && *entity_type >= 2 && *entity_type <= 16 {
        if *entity_type == 16 { *entity_type = 2; }
        else                  { *entity_type += 1; }
    }
    if version <= 20 && *entity_type >= 4 { *entity_type += 1; }
    if version <= 21 && *entity_type >= 5 { *entity_type += 1; }
    if version <= 22 && *entity_type >= 6 { *entity_type += 1; }
    if version <= 23 && *entity_type >= 7 { *entity_type += 1; }
    // ET_MAPMODEL=2, ET_PLAYERSTART=3: yaw was stored as (yaw - 180).
    if version <= 30 && (*entity_type == 2 || *entity_type == 3) {
        attrs[0] = ((attrs[0] as i32 + 180) % 360) as i16;
    }
    // ET_MAPMODEL yaw was snapped to 15° but stored raw; re-snap here.
    if version <= 31 && *entity_type == 2 {
        let yaw = (attrs[0] as i32 % 360 + 360) % 360 + 7;
        attrs[0] = (yaw - yaw % 15) as i16;
    }
}

pub(crate) fn read_entities(
    r: &mut Reader,
    count: i32,
    extra_size: u16,
    version: i32,
) -> Result<Vec<Entity>, OgzError> {
    if count < 0 {
        return Err(OgzError::CountOutOfRange {
            field: "num_ents",
            value: count as i64,
            max: i32::MAX as i64,
        });
    }
    // Entities beyond MAXENTS are consumed but dropped, matching engine behaviour.
    let keep = count.min(MAX_ENTS) as usize;
    let mut ents = Vec::with_capacity(keep);
    for i in 0..count {
        let mut position = r.read_vec3()?;
        let mut attrs = [
            r.read_i16()?,
            r.read_i16()?,
            r.read_i16()?,
            r.read_i16()?,
            r.read_i16()?,
        ];
        let mut entity_type_raw = r.read_u8()?;
        let _reserved = r.read_u8()?;
        let extra_payload = r.read_bytes(extra_size as usize)?.to_vec();

        // Apply version-specific entity fixups.
        fixent(&mut entity_type_raw, &mut attrs, version);

        // FPS game entity fixups (mirrors fpsgame/entities.cpp `readent`).
        // These are game-layer fixups that the engine applies when loading a map
        // in the fps game context.  The yaw convention changed at v30→v31.
        if version <= 30 {
            // Entity types whose attr1 stores a yaw in the old (pre-v30) convention.
            // TELEDEST=20, MONSTER=21, RESPAWNPOINT=25, BOX=26,
            // BARREL=27, PLATFORM=28, ELEVATOR=29, FLAG=30
            const YAW_FIX: &[u8] = &[20, 21, 25, 26, 27, 28, 29, 30];
            if YAW_FIX.contains(&entity_type_raw) {
                attrs[0] = ((attrs[0] as i32 + 180) % 360) as i16;
            }
        }
        if version <= 31 {
            // Entity types whose attr1 is additionally snapped to the nearest 15°.
            // BOX=26, BARREL=27, PLATFORM=28, ELEVATOR=29
            const YAW_SNAP: &[u8] = &[26, 27, 28, 29];
            if YAW_SNAP.contains(&entity_type_raw) {
                let yaw = (attrs[0] as i32 % 360 + 360) % 360 + 7;
                attrs[0] = (yaw - yaw % 15) as i16;
            }
        }

        // v≤14 mapmodel: Z position was stored with an offset.
        if version <= 14 && entity_type_raw == 2 {
            position.z += attrs[2] as f32;
            attrs[2] = 0;
            attrs[3] = 0;
        }

        let entity_type = EntityType::from_u8(entity_type_raw);
        if (i as usize) < keep {
            ents.push(Entity { position, attrs, entity_type, extra_payload });
        }
    }
    Ok(ents)
}

pub(crate) fn write_entities(w: &mut Writer, entities: &[Entity]) {
    for e in entities {
        w.write_vec3(&e.position);
        for &a in &e.attrs {
            w.write_i16(a);
        }
        w.write_u8(e.entity_type.to_u8());
        w.write_u8(0); // reserved
        w.write_bytes(&e.extra_payload);
    }
}

// ── VSlots ────────────────────────────────────────────────────────────────────

pub(crate) fn read_vslots(r: &mut Reader, mut remaining: i32, version: i32) -> Result<Vec<VSlot>, OgzError> {
    // VSlots were added in v30.
    if version < 30 {
        return Ok(vec![]);
    }
    if remaining < 0 || remaining > MAX_VSLOTS {
        return Err(OgzError::CountOutOfRange {
            field: "num_vslots",
            value: remaining as i64,
            max: MAX_VSLOTS as i64,
        });
    }
    let mut slots = Vec::with_capacity(remaining as usize);
    while remaining > 0 {
        let changed = r.read_i32()?;
        if changed < 0 {
            // Negative changed = run of empty slots.
            // saturating_add handles i32::MIN without overflow; clamp to 0.
            let count = remaining.saturating_add(changed).max(0);
            let empty_count = remaining - count;
            for _ in 0..empty_count {
                slots.push(VSlot::Empty);
            }
            remaining = count;
        } else {
            let raw_prev = r.read_i32()?;
            let predecessor = if raw_prev < 0 { None } else { Some(raw_prev as u32) };
            let mut data = VSlotData {
                predecessor,
                shader_params: vec![],
                scale: None,
                rotation: None,
                offset: None,
                scroll: None,
                layer: None,
                alpha: None,
                color: None,
            };
            if changed & (1 << 0) != 0 {
                let nparams = r.read_u16()?;
                if nparams > MAX_SHADER_PARAMS {
                    return Err(OgzError::CountOutOfRange {
                        field: "shader_params_per_vslot",
                        value: nparams as i64,
                        max: MAX_SHADER_PARAMS as i64,
                    });
                }
                for _ in 0..nparams {
                    let name = r.read_string_u16()?;
                    let val = [r.read_f32()?, r.read_f32()?, r.read_f32()?, r.read_f32()?];
                    data.shader_params.push(ShaderParam { name, val });
                }
            }
            if changed & (1 << 1) != 0 { data.scale = Some(r.read_f32()?); }
            if changed & (1 << 2) != 0 { data.rotation = Some(r.read_i32()?); }
            if changed & (1 << 3) != 0 { data.offset = Some([r.read_i32()?, r.read_i32()?]); }
            if changed & (1 << 4) != 0 { data.scroll = Some([r.read_f32()?, r.read_f32()?]); }
            if changed & (1 << 5) != 0 { data.layer = Some(r.read_i32()?); }
            if changed & (1 << 6) != 0 { data.alpha = Some([r.read_f32()?, r.read_f32()?]); }
            if changed & (1 << 7) != 0 {
                data.color = Some([r.read_f32()?, r.read_f32()?, r.read_f32()?]);
            }
            slots.push(VSlot::Data(data));
            remaining -= 1;
        }
    }
    Ok(slots)
}

/// Derive the `changed` bitmask from which fields are populated.
fn vslot_changed_mask(data: &VSlotData) -> i32 {
    let mut m = 0i32;
    if !data.shader_params.is_empty() { m |= 1 << 0; }
    if data.scale.is_some()    { m |= 1 << 1; }
    if data.rotation.is_some() { m |= 1 << 2; }
    if data.offset.is_some()   { m |= 1 << 3; }
    if data.scroll.is_some()   { m |= 1 << 4; }
    if data.layer.is_some()    { m |= 1 << 5; }
    if data.alpha.is_some()    { m |= 1 << 6; }
    if data.color.is_some()    { m |= 1 << 7; }
    m
}

pub(crate) fn write_vslots(w: &mut Writer, slots: &[VSlot]) {
    let mut i = 0;
    while i < slots.len() {
        if matches!(slots[i], VSlot::Empty) {
            // Run-length-encode consecutive empty slots as a single negative entry.
            let start = i;
            while i < slots.len() && matches!(slots[i], VSlot::Empty) {
                i += 1;
            }
            w.write_i32(-((i - start) as i32));
        } else {
            if let VSlot::Data(data) = &slots[i] {
                let changed = vslot_changed_mask(data);
                w.write_i32(changed);
                w.write_i32(data.predecessor.map_or(-1i32, |i| i as i32));
                if changed & (1 << 0) != 0 {
                    w.write_u16(data.shader_params.len() as u16);
                    for p in &data.shader_params {
                        w.write_string_u16(p.name.as_bytes());
                        for &v in &p.val { w.write_f32(v); }
                    }
                }
                if let Some(v) = data.scale    { w.write_f32(v); }
                if let Some(v) = data.rotation { w.write_i32(v); }
                if let Some([x, y]) = data.offset { w.write_i32(x); w.write_i32(y); }
                if let Some([x, y]) = data.scroll { w.write_f32(x); w.write_f32(y); }
                if let Some(v) = data.layer    { w.write_i32(v); }
                if let Some([f, b]) = data.alpha { w.write_f32(f); w.write_f32(b); }
                if let Some([r, g, b]) = data.color { w.write_f32(r); w.write_f32(g); w.write_f32(b); }
            }
            i += 1;
        }
    }
}

// ── Old-format geometry helpers ───────────────────────────────────────────────

/// Get a 4-bit nibble from an edge byte.
/// `coord = 0` → low nibble, `coord = 1` → high nibble.
#[inline]
fn edge_get(edge: u8, coord: usize) -> u8 {
    if coord != 0 { edge >> 4 } else { edge & 0xF }
}

/// Set a 4-bit nibble in an edge byte in-place.
#[inline]
fn edge_set(edge: &mut u8, coord: usize, val: u8) {
    if coord != 0 {
        *edge = (*edge & 0x0F) | ((val & 0xF) << 4);
    } else {
        *edge = (*edge & 0xF0) | (val & 0xF);
    }
}

/// Read the edge value for dimension `dim` at corner `(px, py, pz)` (each 0 or 8)
/// with `coord` selecting the low (0) or high (1) end of the edge.
///
/// Implements `edgeval(c, p, dim, coord)` from the C++ source.
/// `R = [1,2,0]`, `C = [2,0,1]` → `cubeedge(c, d, p[R[d]]>>3, p[C[d]]>>3)`.
#[inline]
fn edge_get_from_corner(edges: &[u8; 12], px: usize, py: usize, pz: usize, dim: usize, coord: usize) -> u8 {
    let ix = px / 8;
    let iy = py / 8;
    let iz = pz / 8;
    match dim {
        0 => edge_get(edges[iz * 2 + iy], coord),      // R[0]=1→y, C[0]=2→z
        1 => edge_get(edges[4 + ix * 2 + iz], coord),  // R[1]=2→z, C[1]=0→x
        2 => edge_get(edges[8 + iy * 2 + ix], coord),  // R[2]=0→x, C[2]=1→y
        _ => unreachable!(),
    }
}

#[inline]
fn cross3(a: [f64; 3], b: [f64; 3]) -> [f64; 3] {
    [
        a[1] * b[2] - a[2] * b[1],
        a[2] * b[0] - a[0] * b[2],
        a[0] * b[1] - a[1] * b[0],
    ]
}

#[inline]
fn dot3(a: [f64; 3], b: [f64; 3]) -> f64 {
    a[0] * b[0] + a[1] * b[1] + a[2] * b[2]
}

/// Build a plane `[nx, ny, nz, d]` (where `n·x + d = 0`) through three points,
/// optionally using the "solid" fallback (sets the constant dimension coord to 0
/// or 8 instead of reading from edges).
///
/// Mirrors `genvertp()` in octa.cpp.
fn genvertp_plane(
    edges: &[u8; 12],
    q1: [f64; 3],
    q2: [f64; 3],
    q3: [f64; 3],
    solid: bool,
) -> [f64; 4] {
    // Detect the dimension that is constant across all three points.
    let dim = if q1[1] == q2[1] && q2[1] == q3[1] { 1usize }
              else if q1[2] == q2[2] && q2[2] == q3[2] { 2usize }
              else { 0usize };

    let coord = (q1[dim] as usize) / 8; // 0 or 1

    let get_dim = |p: [f64; 3]| -> f64 {
        if solid {
            q1[dim] // coord * 8 = the constant coordinate value
        } else {
            edge_get_from_corner(edges, p[0] as usize, p[1] as usize, p[2] as usize, dim, coord) as f64
        }
    };

    let mut v1 = q1;
    let mut v2 = q2;
    let mut v3 = q3;
    v1[dim] = get_dim(q1);
    v2[dim] = get_dim(q2);
    v3[dim] = get_dim(q3);

    let e1 = [v2[0] - v1[0], v2[1] - v1[1], v2[2] - v1[2]];
    let e2 = [v3[0] - v1[0], v3[1] - v1[1], v3[2] - v1[2]];
    let n_raw = cross3(e1, e2);
    let len = (n_raw[0] * n_raw[0] + n_raw[1] * n_raw[1] + n_raw[2] * n_raw[2]).sqrt();
    let n = if len < 1e-12 {
        [0.0f64, 0.0, 0.0]
    } else {
        [n_raw[0] / len, n_raw[1] / len, n_raw[2] / len]
    };
    let d = -(n[0] * v1[0] + n[1] * v1[1] + n[2] * v1[2]);
    [n[0], n[1], n[2], d]
}

#[inline]
fn planes_equal(p: [f64; 4], q: [f64; 4]) -> bool {
    p[0] == q[0] && p[1] == q[1] && p[2] == q[2] && p[3] == q[3]
}

/// Compute the three-plane intersection.
///
/// Returns the clamped intersection point, or `None` if the system is degenerate.
/// Mirrors `threeplaneintersect()` in octa.cpp.
fn three_plane_intersect(pl1: [f64; 4], pl2: [f64; 4], pl3: [f64; 4]) -> Option<[f64; 3]> {
    let n1 = [pl1[0], pl1[1], pl1[2]];
    let n2 = [pl2[0], pl2[1], pl2[2]];
    let n3 = [pl3[0], pl3[1], pl3[2]];

    let t4 = cross3(n1, n2);
    let det = dot3(t4, n3);
    if det.abs() < 1e-10 {
        return None;
    }

    // t1 = (n1 × n2) * d3
    let t1 = { let c = cross3(n1, n2); [c[0] * pl3[3], c[1] * pl3[3], c[2] * pl3[3]] };
    // t2 = (n3 × n1) * d2
    let t2 = { let c = cross3(n3, n1); [c[0] * pl2[3], c[1] * pl2[3], c[2] * pl2[3]] };
    // t3 = (n2 × n3) * d1
    let t3 = { let c = cross3(n2, n3); [c[0] * pl1[3], c[1] * pl1[3], c[2] * pl1[3]] };

    let sum = [t1[0] + t2[0] + t3[0], t1[1] + t2[1] + t3[1], t1[2] + t2[2] + t3[2]];
    let v = [
        (-sum[0] / det).max(0.0).min(8.0),
        (-sum[1] / det).max(0.0).min(8.0),
        (-sum[2] / det).max(0.0).min(8.0),
    ];
    Some(v)
}

/// Compute the converted position for corner `(ix, iy, iz)` (each 0 or 1) of a
/// cube given the old edge-span `edges`.
///
/// Mirrors `genedgespanvert()` in octa.cpp.
fn genedgespanvert(edges: &[u8; 12], ix: usize, iy: usize, iz: usize) -> [f64; 3] {
    let px = (ix * 8) as f64;
    let py = (iy * 8) as f64;
    let pz = (iz * 8) as f64;

    let p  = [px,       py,       pz      ];
    let p1 = [8.0 - px, py,       pz      ]; // opposite X
    let p2 = [px,       8.0 - py, pz      ]; // opposite Y
    let p3 = [px,       py,       8.0 - pz]; // opposite Z

    let mut plane1 = genvertp_plane(edges, p, p1, p2, false); // const Z
    let mut plane2 = genvertp_plane(edges, p, p2, p3, false); // const X
    let plane3 = genvertp_plane(edges, p, p3, p1, false); // const Y

    // Degenerate-plane fallback: regenerate with solid=true.
    if planes_equal(plane1, plane2) { plane1 = genvertp_plane(edges, p, p1, p2, true); }
    if planes_equal(plane1, plane3) { plane1 = genvertp_plane(edges, p, p1, p2, true); }
    if planes_equal(plane2, plane3) { plane2 = genvertp_plane(edges, p, p2, p3, true); }

    match three_plane_intersect(plane1, plane2, plane3) {
        Some(v) => v,
        None    => [px, py, pz], // degenerate system → use corner as-is
    }
}

/// Convert the 12 edge bytes from the old "edge-span" format to the modern
/// "vector cube" format in-place.
///
/// Mirrors `edgespan2vectorcube()` in octa.cpp.  No-op for empty or entirely
/// solid cubes.
fn edgespan2vectorcube(edges: &mut [u8; 12]) {
    // isempty: faces[0] (= edges[0..4]) is all zero.
    if edges[0] == 0 && edges[1] == 0 && edges[2] == 0 && edges[3] == 0 {
        return;
    }
    // isentirelysolid: all edge bytes are 0x80.
    if edges.iter().all(|&e| e == 0x80) {
        return;
    }

    let old = *edges;

    for iz in 0..2usize {
        for iy in 0..2usize {
            for ix in 0..2usize {
                let v = genedgespanvert(&old, ix, iy, iz);
                // Store new corner positions.
                // cubeedge(c, 0, y, z) = edges[z*2 + y]
                edge_set(&mut edges[iz * 2 + iy],     ix, (v[0] + 0.49) as u8);
                // cubeedge(c, 1, z, x) = edges[4 + x*2 + z]
                edge_set(&mut edges[4 + ix * 2 + iz], iy, (v[1] + 0.49) as u8);
                // cubeedge(c, 2, x, y) = edges[8 + y*2 + x]
                edge_set(&mut edges[8 + iy * 2 + ix], iz, (v[2] + 0.49) as u8);
            }
        }
    }
}

// ── Old-format surface structures ─────────────────────────────────────────────

/// LMID sentinel values (lightmap ID constants).
const LMID_AMBIENT: u8  = 0;
const LMID_AMBIENT1: u8 = 1;
#[allow(dead_code)]
const LMID_BRIGHT: u8   = 2;
const LMID_BRIGHT1: u8  = 3;
const LMID_DARK: u8     = 4;
const LMID_RESERVED: u8 = 6;

/// Adjust an LMID read from an old map to account for constants added in later versions.
fn adjust_lmid(mut lmid: u8, version: i32) -> u8 {
    if version < 10 {
        lmid = lmid.saturating_add(1);
    }
    if version < 18 {
        if lmid >= LMID_AMBIENT1 { lmid = lmid.saturating_add(1); }
        if lmid >= LMID_BRIGHT1  { lmid = lmid.saturating_add(1); }
    }
    if version < 19 {
        if lmid >= LMID_DARK { lmid = lmid.saturating_add(2); }
    }
    lmid
}

/// Old per-face surface data (v7–v31).
#[derive(Clone, Default)]
struct SurfaceCompat {
    texcoords: [u8; 8], // u,v pairs for 4 vertices
    w: u8,
    h: u8,
    x: u16,
    y: u16,
    lmid: u8,
    layer: u8,
}

fn read_surfacecompat(r: &mut Reader, version: i32) -> Result<SurfaceCompat, OgzError> {
    let mut texcoords = [0u8; 8];
    texcoords.copy_from_slice(r.read_bytes(8)?);
    let w = r.read_u8()?;
    let h = r.read_u8()?;
    let x = r.read_u16()?;
    let y = r.read_u16()?;
    let lmid_raw = r.read_u8()?;
    let layer    = r.read_u8()?;
    let lmid = adjust_lmid(lmid_raw, version);
    Ok(SurfaceCompat { texcoords, w, h, x, y, lmid, layer })
}

/// Old per-face normal data (v7–v31), stored alongside surfaces when mask & 0x40.
struct NormalsCompat {
    normals: [[u8; 3]; 4],
}

fn read_normalscompat(r: &mut Reader) -> Result<NormalsCompat, OgzError> {
    let mut normals = [[0u8; 3]; 4];
    for n in normals.iter_mut() {
        n.copy_from_slice(r.read_bytes(3)?);
    }
    Ok(NormalsCompat { normals })
}

/// Returns (u1, u2, v1, v2) merge coordinates in the modern (v26+) convention.
fn read_mergecompat(r: &mut Reader, version: i32) -> Result<(u16, u16, u16, u16), OgzError> {
    let mut u1 = r.read_u16()?;
    let mut u2 = r.read_u16()?;
    let mut v1 = r.read_u16()?;
    let mut v2 = r.read_u16()?;
    // v≤25: high 3 bits of u1/v1 encode the atlas-tile origin; strip and shift.
    if version <= 25 {
        let uorigin = u1 & 0xE000;
        let vorigin = v1 & 0xE000;
        u1 = (u1.wrapping_sub(uorigin)) << 2;
        u2 = (u2.wrapping_sub(uorigin)) << 2;
        v1 = (v1.wrapping_sub(vorigin)) << 2;
        v2 = (v2.wrapping_sub(vorigin)) << 2;
    }
    Ok((u1, u2, v1, v2))
}

// ── Old-format surface conversion ─────────────────────────────────────────────

/// Encode a UV coordinate from old lightmap atlas coordinates to the u16
/// on-disk format used by modern surfaces.
///
/// `u = x + (texcoord / 255) * (w - 1)`, then scale to `[0, 65535]` over
/// the 512-pixel lightmap pack.
///
/// `bias`: primary surfaces add `+0.5f` before floor; dup-layer surfaces do not.
fn encode_old_uv(x: u16, y: u16, w: u8, h: u8, tc_u: u8, tc_v: u8, bias: bool) -> [u16; 2] {
    // Match C++: primary: `ushort(floor(clamp(u * 65536 / 512 + 0.5, 0, 65535)))`
    //            dup-layer: `ushort(floor(clamp(u * 65536 / 512, 0, 65535)))`
    const PACK: f32 = 512.0;
    const SCALE: f32 = 65536.0; // USHRT_MAX + 1

    let u = x as f32 + (tc_u as f32 / 255.0_f32) * (w as f32 - 1.0_f32);
    let v = y as f32 + (tc_v as f32 / 255.0_f32) * (h as f32 - 1.0_f32);

    let factor = SCALE / PACK; // 128.0
    let b = if bias { 0.5_f32 } else { 0.0_f32 };
    let eu = (u * factor + b).floor().clamp(0.0, 65535.0) as u16;
    let ev = (v * factor + b).floor().clamp(0.0, 65535.0) as u16;
    [eu, ev]
}

/// Encode a bvec3 normal (3 × u8, 0–255) into a u16 packed normal.
///
/// `bvec(128, 128, 128)` is the "unset" sentinel → returns 0.
/// Mirrors `encodenormal(bvec.tonormal().normalize())` from the C++ source.
fn encode_bvec_normal(bvec: [u8; 3]) -> u16 {
    if bvec == [128, 128, 128] {
        return 0;
    }
    let x = bvec[0] as f64 * (2.0 / 255.0) - 1.0;
    let y = bvec[1] as f64 * (2.0 / 255.0) - 1.0;
    let z = bvec[2] as f64 * (2.0 / 255.0) - 1.0;
    let len = (x * x + y * y + z * z).sqrt();
    if len < 1e-10 {
        return 0;
    }
    let (nx, ny, nz) = (x / len, y / len, z / len);
    // encodenormal: yaw = int(-atan2(n.x, n.y) / RAD), pitch = int(asin(n.z) / RAD)
    let yaw   = (-(nx.atan2(ny)).to_degrees()) as i32;
    let pitch = (nz.asin().clamp(-1.0, 1.0).to_degrees()) as i32;
    let yaw_n = ((yaw % 360) + 360) % 360;
    ((pitch + 90).clamp(0, 180) * 360 + yaw_n + 1) as u16
}

/// Convert a pair of `SurfaceCompat` entries (primary + optional blend) into a
/// modern `SurfaceInfo`.
///
/// UV and normal data are filled; vertex XYZ positions are **not** computed
/// (they are re-derived from cube edges at runtime, as in the engine).
/// Per-face vertex ordering derived from the C++ `facecoords[orient][k]` ivec table.
/// Each entry is (col_hi, row_hi): 0 selects u1/v1, 1 selects u2/v2.
/// col_hi  = facecoords[orient][k][vc] != 0
/// row_hi  = facecoords[orient][k][vr] != 0
/// where vc=C[dim], vr=R[dim], C=[2,0,1], R=[1,2,0].
const MERGE_FACE_COORDS: [[(u8, u8); 4]; 6] = [
    [(1,1),(0,1),(0,0),(1,0)], // orient 0: dim=0, vc=Z, vr=Y
    [(1,1),(1,0),(0,0),(0,1)], // orient 1: dim=0, vc=Z, vr=Y
    [(1,1),(0,1),(0,0),(1,0)], // orient 2: dim=1, vc=X, vr=Z
    [(0,0),(0,1),(1,1),(1,0)], // orient 3: dim=1, vc=X, vr=Z
    [(0,0),(1,0),(1,1),(0,1)], // orient 4: dim=2, vc=Y, vr=X
    [(0,0),(0,1),(1,1),(1,0)], // orient 5: dim=2, vc=Y, vr=X
];

/// Overlay merge (column/row) coordinates onto an existing SurfaceInfo's verts.
/// Sets has_xyz (0x04) in verts_mask and writes xy=[cc,rc] for each of the 4
/// corner verts according to the per-face facecoords ordering.  If the surface
/// has no verts yet (neither UV nor norm), 4 skeleton verts are created.
fn apply_merge_xyz(si: &mut SurfaceInfo, u1: u16, u2: u16, v1: u16, v2: u16, orient: usize) {
    si.verts_mask |= 0x04; // has_xyz
    si.verts_mask &= !0x01; // clear compact-xyz flag
    si.xyz_bound = None;
    let coords = MERGE_FACE_COORDS[orient];
    if si.verts.is_empty() {
        // No UV/norm verts: create 4 with only xy.
        for &(cu, cv) in &coords {
            let cc = if cu == 0 { u1 } else { u2 };
            let rc = if cv == 0 { v1 } else { v2 };
            si.verts.push(SurfaceVert { xy: Some([cc, rc]), uv: None, norm: None });
        }
    } else {
        // Set xy on the existing 4 verts.
        for (k, &(cu, cv)) in coords.iter().enumerate() {
            let cc = if cu == 0 { u1 } else { u2 };
            let rc = if cv == 0 { v1 } else { v2 };
            if k < si.verts.len() {
                si.verts[k].xy = Some([cc, rc]);
            }
        }
    }
}

/// Build a SurfaceInfo for a face that has only merge-coordinate data (no lightmap).
/// The 4 vertex positions come from the merge extent: column range [u1,u2],
/// row range [v1,v2].  No UV or normal data.
fn merge_only_surface(u1: u16, u2: u16, v1: u16, v2: u16, orient: usize) -> SurfaceInfo {
    // Use the C++ facecoords table to get the correct vertex ordering per face.
    let coords = MERGE_FACE_COORDS[orient];
    let verts = coords.iter().map(|&(cu, cv)| {
        let cc = if cu == 0 { u1 } else { u2 };
        let rc = if cv == 0 { v1 } else { v2 };
        SurfaceVert { xy: Some([cc, rc]), uv: None, norm: None }
    }).collect();
    SurfaceInfo {
        lmid:         [LMID_AMBIENT, LMID_AMBIENT],
        verts_mask:   0x04, // has_xyz, per-vertex
        layer_top:    true,
        layer_bottom: false,
        verts,
        xyz_bound:    None,
        uv_plane:     None,
        base_norm:    None,
        uv_plane_dup: None,
        verts_dup_uv: vec![],
    }
}

fn convert_old_surface(
    src: &SurfaceCompat,
    blend: Option<&SurfaceCompat>,
    norms: Option<&NormalsCompat>,
) -> SurfaceInfo {
    // Determine layer flags.
    let layer_top    = src.layer != 1;
    let layer_bottom = src.layer == 1 || (src.layer & 2) != 0;
    let has_dup_layer = blend.is_some() && {
        let b = blend.unwrap();
        b.lmid >= LMID_RESERVED && (b.x != src.x || b.y != src.y || b.w != src.w || b.h != src.h || b.texcoords != src.texcoords)
    };

    let lmid0 = if (src.layer & 2) != 0 { src.lmid } else if src.layer == 1 { LMID_AMBIENT } else { src.lmid };
    let lmid1 = if (src.layer & 2) != 0 { blend.map_or(LMID_AMBIENT, |b| b.lmid) }
                else if src.layer == 1 { src.lmid }
                else { LMID_AMBIENT };

    // Decide whether UV and norm data are useful.
    let uselms   = src.lmid >= LMID_RESERVED || lmid1 >= LMID_RESERVED || !layer_top || layer_bottom;
    let usenorms = norms.is_some() && norms.unwrap().normals[0] != [128, 128, 128];

    let verts_mask: u8 = if uselms { 0x40 } else { 0 } | if usenorms { 0x80 } else { 0 };

    // Generate 4 per-vertex entries (simplified: no position-based dedup).
    let mut verts: Vec<SurfaceVert> = Vec::with_capacity(4);
    let mut verts_dup_uv: Vec<[u16; 2]> = Vec::new();

    if uselms || usenorms {
        for k in 0..4usize {
            let uv = if uselms {
                Some(encode_old_uv(src.x, src.y, src.w, src.h, src.texcoords[k * 2], src.texcoords[k * 2 + 1], true))
            } else {
                None
            };
            let norm = if usenorms {
                let n = norms.unwrap().normals[k];
                if n != [128, 128, 128] { Some(encode_bvec_normal(n)) } else { None }
            } else {
                None
            };
            verts.push(SurfaceVert { xy: None, uv, norm });
        }

        if has_dup_layer {
            let b = blend.unwrap();
            for k in 0..4usize {
                verts_dup_uv.push(encode_old_uv(b.x, b.y, b.w, b.h, b.texcoords[k * 2], b.texcoords[k * 2 + 1], false));
            }
        }
    }

    SurfaceInfo {
        lmid: [lmid0, lmid1],
        verts_mask,
        layer_top,
        layer_bottom,
        xyz_bound: None,
        uv_plane: None,
        uv_plane_dup: None,
        base_norm: None,
        verts,
        verts_dup_uv,
    }
}

/// Convert old material byte to modern u16 material.
/// For v<27 uses a fixed lookup table; for v≥27 uses the bit-field formula.
fn convert_old_material(mat: u8, version: i32) -> u16 {
    if version < 27 {
        // matconv table from worldio.cpp:
        // { MAT_AIR=0, MAT_WATER=4, MAT_CLIP=32, MAT_GLASS|MAT_CLIP=40,
        //   MAT_NOCLIP=64, MAT_LAVA|MAT_DEATH=268, MAT_GAMECLIP=96, MAT_DEATH=256 }
        const MATCONV: [u16; 8] = [0, 4, 32, 40, 64, 268, 96, 256];
        if (mat as usize) < MATCONV.len() { MATCONV[mat as usize] } else { 0 }
    } else {
        // convertoldmaterial:
        //   ((mat&7)<<MATF_VOLUME_SHIFT=2) | (((mat>>3)&3)<<MATF_CLIP_SHIFT=5) | (((mat>>5)&7)<<MATF_FLAG_SHIFT=8)
        let mat = mat as u16;
        ((mat & 7) << 2) | (((mat >> 3) & 3) << 5) | (((mat >> 5) & 7) << 8)
    }
}

/// Read the post-texture node data for format versions 7–31.
///
/// Returns `(material, merged, surfaces)`.
fn read_old_cube_data(
    r: &mut Reader,
    version: i32,
    octsav: u8,
    edges: &mut [u8; 12],
) -> Result<(Option<u16>, Option<u8>, Option<Box<NodeSurfaces>>), OgzError> {
    let mask = r.read_u8()?;

    // Material (bit 7 of mask).
    let material: Option<u16> = if (mask & 0x80) != 0 {
        let mat = r.read_u8()?;
        Some(convert_old_material(mat, version))
    } else {
        None
    };

    // Per-face surface + normal data.
    // `numsurfs` can grow as blend-layer surfaces are encountered.
    let has_norms_global = (mask & 0x40) != 0;
    let mut surfs: [Option<SurfaceCompat>; 12]       = Default::default();
    let mut norms: [Option<NormalsCompat>; 6]         = core::array::from_fn(|_| None);
    let mut hassurfs: u8 = 0;
    let mut hasnorms: u8 = 0;
    let mut numsurfs: usize = 6;

    if (mask & 0x3F) != 0 {
        let mut i = 0usize;
        while i < numsurfs {
            if i >= 6 || (mask & (1 << i)) != 0 {
                surfs[i] = Some(read_surfacecompat(r, version)?);
                if i < 6 {
                    if has_norms_global {
                        hasnorms |= 1 << i;
                        norms[i] = Some(read_normalscompat(r)?);
                    }
                    let s = surfs[i].as_ref().unwrap();
                    if s.lmid != LMID_AMBIENT || s.layer != 0 {
                        hassurfs |= 1 << i;
                    }
                    if (s.layer & 2) != 0 {
                        numsurfs += 1;
                    }
                }
            }
            i += 1;
        }
    }

    // Edge-span → vector-cube conversion (v≤8).
    if version <= 8 {
        edgespan2vectorcube(edges);
    }

    // Face / texture swap for v≤11 (happens to `hassurfs` bitmask + surface indices).
    let hassurfs_adj = if version <= 11 && (hassurfs & 0x33) != 0 {
        // Swap surface entries 0,1 ↔ 4,5 in the working array.
        surfs.swap(0, 4);
        surfs.swap(1, 5);
        norms.swap(0, 4);
        norms.swap(1, 5);
        (hassurfs & 0xCC) | ((hassurfs & 0x30) >> 4) | ((hassurfs & 0x03) << 4)
    } else {
        hassurfs
    };

    // Merged data (v≥20, signalled by octsav bit 7).
    // We store per-face merge coordinates so we can generate vertex positions
    // for faces whose geometry was extended across cube boundaries.
    let mut merges: [Option<(u16, u16, u16, u16)>; 6] = [None; 6];
    let mut hasmerges: u8 = 0;
    let merged: Option<u8> = if version >= 20 && (octsav & 0x80) != 0 {
        let merged_byte = r.read_u8()?;
        let merged_val  = merged_byte & 0x3F;
        if (merged_byte & 0x80) != 0 {
            let merge_mask = r.read_u8()?;
            for i in 0..6usize {
                if (merge_mask & (1 << i)) != 0 {
                    let (u1, u2, v1, v2) = read_mergecompat(r, version)?;
                    // Only store valid merge extents (u1<u2 and v1<v2).
                    if u1 < u2 && v1 < v2 {
                        merges[i] = Some((u1, u2, v1, v2));
                        hasmerges |= 1 << i;
                    }
                }
            }
        }
        Some(merged_val)
    } else {
        None
    };

    // Convert old surfaces to modern SurfaceInfo.
    let surfaces: Option<Box<NodeSurfaces>> = if hassurfs_adj != 0 || hasnorms != 0 || hasmerges != 0 {
        let mut out: NodeSurfaces = Default::default();
        let mut blend_idx: usize = 6;
        for i in 0..6usize {
            let any = (hassurfs_adj | hasnorms | hasmerges) & (1 << i) != 0;
            if !any { continue; }

            if (hassurfs_adj | hasnorms) & (1 << i) != 0 {
                let Some(ref src) = surfs[i] else { continue };
                let blend = if (src.layer & 2) != 0 {
                    let b = surfs[blend_idx].as_ref();
                    blend_idx += 1;
                    b
                } else {
                    None
                };
                let norm = norms[i].as_ref();
                let mut si = convert_old_surface(src, blend, norm);
                // When merge coordinates are also present, the engine uses them
                // for XYZ vertex positions even if lightmap data exists.
                if let Some((u1, u2, v1, v2)) = merges[i] {
                    apply_merge_xyz(&mut si, u1, u2, v1, v2, i);
                }
                out[i] = Some(si);
            } else if let Some((u1, u2, v1, v2)) = merges[i] {
                // Merge-only face: no lightmap data, but vertex positions come
                // from the merge extent coordinates (u1/u2 = column range,
                // v1/v2 = row range in the face plane).
                out[i] = Some(merge_only_surface(u1, u2, v1, v2, i));
            }
        }
        // Only allocate the box if at least one surface is present.
        if out.iter().any(|s| s.is_some()) {
            Some(Box::new(out))
        } else {
            None
        }
    } else {
        None
    };

    Ok((material, merged, surfaces))
}

// ── Octree geometry ───────────────────────────────────────────────────────────

/// Read the map's octree root.
pub(crate) fn read_octree_root(r: &mut Reader, version: i32) -> Result<OctreeNode, OgzError> {
    let children = read_children(r, 0, version)?;
    Ok(OctreeNode::Children { children, material: None, merged: None })
}

/// Write the map's octree root.
pub(crate) fn write_octree_root(w: &mut Writer, root: &OctreeNode) {
    if let OctreeNode::Children { children, .. } = root {
        for child in children.iter() {
            write_octree_node(w, child, 1);
        }
    }
}

pub(crate) fn read_octree_node(r: &mut Reader, depth: u32, version: i32) -> Result<OctreeNode, OgzError> {
    if depth > MAX_OCTREE_DEPTH {
        return Err(OgzError::DepthLimitExceeded { structure: "octree", depth });
    }
    let octsav = r.read_u8()?;
    let node_type = octsav & 0x07;

    tracing::trace!(depth, node_type, "octree node");

    // CHILDREN: just recurse with no node-level data in any format version.
    if node_type == 0 {
        let children = read_children(r, depth, version)?;
        return Ok(OctreeNode::Children { children, material: None, merged: None });
    }

    let has_lodcube_children = node_type == 4;

    // Edges (NORMAL nodes only).
    let mut edges = [0u8; 12];
    if node_type == 3 {
        let edge_bytes = r.read_bytes(12)?;
        edges.copy_from_slice(edge_bytes);
    }

    // Textures: u8 for v<14, u16 for v≥14.
    let mut textures: [u16; 6] = if version < 14 {
        [
            r.read_u8()? as u16, r.read_u8()? as u16, r.read_u8()? as u16,
            r.read_u8()? as u16, r.read_u8()? as u16, r.read_u8()? as u16,
        ]
    } else {
        read_textures(r)?
    };

    // Apply face/texture swap for v≤11 (X ↔ Z).
    if version <= 11 {
        // cubeedge swap: faces[0] ↔ faces[2] (edges 0-3 ↔ 8-11)
        for k in 0..4usize {
            edges.swap(k, 8 + k);
        }
        textures.swap(0, 4);
        textures.swap(1, 5);
    }

    let (material, merged, surfaces) = if version < 7 {
        // v<7: just skip 3 bytes, no surface data.
        r.skip(3)?;
        (None, None, None)
    } else if version <= 31 {
        // v7–v31: mask byte + old surface format.
        read_old_cube_data(r, version, octsav, &mut edges)?
    } else {
        // v32–v33: modern format, flags in octsav.
        let has_material = (octsav & 0x40) != 0;
        let has_merged   = (octsav & 0x80) != 0;
        let has_surfaces = (octsav & 0x20) != 0;

        let material: Option<u16> = if has_material {
            Some(if version <= 32 {
                // v32: single-byte material, convert with old formula.
                convert_old_material(r.read_u8()?, version)
            } else {
                // v33: native u16 material.
                r.read_u16()?
            })
        } else {
            None
        };
        let merged   = read_if(has_merged,   || r.read_u8())?;
        let surfaces = read_if(has_surfaces, || read_surfaces(r).map(Box::new))?;
        (material, merged, surfaces)
    };

    // Children for LOD cubes.
    let children = if has_lodcube_children {
        Some(read_children(r, depth, version)?)
    } else {
        None
    };

    match (node_type, children) {
        (1, None) => Ok(OctreeNode::Empty  { textures, material, merged, surfaces }),
        (2, None) => Ok(OctreeNode::Solid  { textures, material, merged, surfaces }),
        (3, None) => Ok(OctreeNode::Normal { textures, edges, material, merged, surfaces }),
        (4, Some(ch)) => Ok(OctreeNode::LodCube { textures, children: ch, material, merged, surfaces }),
        _ => Err(OgzError::UnknownOctreeNodeType(node_type)),
    }
}

/// Helper: read a value only when `flag` is true, otherwise return `None`.
#[inline]
fn read_if<T, F: FnOnce() -> Result<T, OgzError>>(
    flag: bool,
    f: F,
) -> Result<Option<T>, OgzError> {
    if flag { Ok(Some(f()?)) } else { Ok(None) }
}

fn read_children(r: &mut Reader, depth: u32, version: i32) -> Result<Box<[OctreeNode; 8]>, OgzError> {
    let mut v = Vec::with_capacity(8);
    for _ in 0..8 {
        v.push(read_octree_node(r, depth + 1, version)?);
    }
    Ok(Box::new(v.try_into().unwrap()))
}

fn read_textures(r: &mut Reader) -> Result<[u16; 6], OgzError> {
    Ok([
        r.read_u16()?, r.read_u16()?, r.read_u16()?,
        r.read_u16()?, r.read_u16()?, r.read_u16()?,
    ])
}

fn read_surfaces(r: &mut Reader) -> Result<NodeSurfaces, OgzError> {
    let surf_mask    = r.read_u8()?;
    let _total_verts = r.read_u8()?; // consumed but not stored; derived on write
    let mut surfaces: NodeSurfaces = Default::default();
    for i in 0..6usize {
        if (surf_mask & (1 << i)) != 0 {
            let lmid = [r.read_u8()?, r.read_u8()?];
            let verts_mask    = r.read_u8()?;
            let num_verts_raw = r.read_u8()?;
            surfaces[i] = Some(read_surface_info(r, lmid, verts_mask, num_verts_raw)?);
        }
    }
    Ok(surfaces)
}

fn read_surface_info(
    r: &mut Reader,
    lmid: [u8; 2],
    verts_mask: u8,
    num_verts_raw: u8,
) -> Result<SurfaceInfo, OgzError> {
    let layer_verts  = (num_verts_raw & 0x0F) as usize;
    let layer_top    = (num_verts_raw & 0x20) != 0;
    let layer_bottom = (num_verts_raw & 0x40) != 0;
    let layer_dup    = (num_verts_raw & 0x80) != 0;
    let has_xyz  = (verts_mask & 0x04) != 0;
    let has_uv   = (verts_mask & 0x40) != 0;
    let has_norm = (verts_mask & 0x80) != 0;

    if layer_verts > 15 {
        return Err(OgzError::TooManyFaceVerts { count: layer_verts });
    }

    let mut xyz_bound    = None;
    let mut uv_plane     = None;
    let mut uv_plane_dup = None;
    let mut base_norm    = None;

    if layer_verts == 4 {
        if has_xyz && (verts_mask & 0x01) != 0 {
            xyz_bound = Some([r.read_u16()?, r.read_u16()?, r.read_u16()?, r.read_u16()?]);
        }
        if has_uv && (verts_mask & 0x02) != 0 {
            uv_plane = Some([r.read_u16()?, r.read_u16()?, r.read_u16()?, r.read_u16()?]);
            if layer_dup {
                uv_plane_dup =
                    Some([r.read_u16()?, r.read_u16()?, r.read_u16()?, r.read_u16()?]);
            }
        }
    }
    if has_norm && (verts_mask & 0x08) != 0 {
        base_norm = Some(r.read_u16()?);
    }

    // After compact sections, the corresponding has_* flags are "consumed" —
    // the C++ does NOT write per-vertex data for fields that were encoded compactly.
    let per_xyz  = has_xyz  && !(layer_verts == 4 && (verts_mask & 0x01) != 0);
    let per_uv   = has_uv   && !(layer_verts == 4 && (verts_mask & 0x02) != 0);
    let per_norm = has_norm && !((verts_mask & 0x08) != 0);

    let mut verts = Vec::with_capacity(layer_verts);
    for _ in 0..layer_verts {
        let xy   = if per_xyz  { Some([r.read_u16()?, r.read_u16()?]) } else { None };
        let uv   = if per_uv   { Some([r.read_u16()?, r.read_u16()?]) } else { None };
        let norm = if per_norm { Some(r.read_u16()?)                  } else { None };
        verts.push(SurfaceVert { xy, uv, norm });
    }

    let mut verts_dup_uv = Vec::new();
    if layer_dup && per_uv {
        for _ in 0..layer_verts {
            verts_dup_uv.push([r.read_u16()?, r.read_u16()?]);
        }
    }

    Ok(SurfaceInfo { lmid, verts_mask, layer_top, layer_bottom, xyz_bound, uv_plane, uv_plane_dup, base_norm, verts, verts_dup_uv })
}

pub(crate) fn write_octree_node(w: &mut Writer, node: &OctreeNode, depth: u32) {
    let (type_bits, textures, edges, material, merged, surfaces, children) = match node {
        OctreeNode::Children { children, material, merged } =>
            (0u8, None, None, material, merged, None, Some(children.as_ref())),
        OctreeNode::Empty { textures, material, merged, surfaces } =>
            (1, Some(textures), None, material, merged, surfaces.as_deref(), None),
        OctreeNode::Solid { textures, material, merged, surfaces } =>
            (2, Some(textures), None, material, merged, surfaces.as_deref(), None),
        OctreeNode::Normal { textures, edges, material, merged, surfaces } =>
            (3, Some(textures), Some(edges), material, merged, surfaces.as_deref(), None),
        OctreeNode::LodCube { textures, children, material, merged, surfaces } =>
            (4, Some(textures), None, material, merged, surfaces.as_deref(), Some(children.as_ref())),
    };

    let octsav = type_bits
        | if material.is_some()  { 0x40 } else { 0 }
        | if merged.is_some()    { 0x80 } else { 0 }
        | if surfaces.is_some()  { 0x20 } else { 0 };
    w.write_u8(octsav);

    // NORMAL nodes (type 3): edges come BEFORE textures in the file
    if let Some(e) = edges {
        w.write_bytes(e);
    }
    if let Some(texs) = textures {
        for &t in texs { w.write_u16(t); }
    }
    if let Some(m) = material { w.write_u16(*m); }
    if let Some(m) = merged   { w.write_u8(*m);  }
    if let Some(surfs) = surfaces {
        write_surfaces(w, surfs);
    }
    if let Some(ch) = children {
        for child in ch.iter() {
            write_octree_node(w, child, depth + 1);
        }
    }
}

fn write_surfaces(w: &mut Writer, surfaces: &NodeSurfaces) {
    let surf_mask: u8 = surfaces
        .iter()
        .enumerate()
        .fold(0, |m, (i, s)| if s.is_some() { m | (1u8 << i) } else { m });
    let total_verts: u8 = surfaces
        .iter()
        .flatten()
        .map(|s| s.layer_verts() as u8)
        .fold(0u8, |a, b| a.saturating_add(b));

    w.write_u8(surf_mask);
    w.write_u8(total_verts);

    for surf in surfaces.iter().flatten() {
        let verts_mask    = surf.verts_mask;
        let num_verts_raw = surf.num_verts_raw();
        w.write_u8(surf.lmid[0]);
        w.write_u8(surf.lmid[1]);
        w.write_u8(verts_mask);
        w.write_u8(num_verts_raw);

        let layer_verts = surf.verts.len();
        let has_xyz  = (verts_mask & 0x04) != 0;
        let has_uv   = (verts_mask & 0x40) != 0;
        let has_norm = (verts_mask & 0x80) != 0;

        if layer_verts == 4 {
            if has_xyz && (verts_mask & 0x01) != 0 {
                if let Some(xyz) = &surf.xyz_bound {
                    for &v in xyz { w.write_u16(v); }
                }
            }
            if has_uv && (verts_mask & 0x02) != 0 {
                if let Some(uv) = &surf.uv_plane {
                    for &v in uv { w.write_u16(v); }
                    if let Some(uv_dup) = &surf.uv_plane_dup {
                        for &v in uv_dup { w.write_u16(v); }
                    }
                }
            }
        }
        if has_norm && (verts_mask & 0x08) != 0 {
            if let Some(n) = surf.base_norm { w.write_u16(n); }
        }

        // Compact sections consume has_xyz/has_uv/has_norm for per-vertex writing.
        let per_xyz  = has_xyz && !(layer_verts == 4 && (verts_mask & 0x01) != 0);
        let per_uv   = has_uv  && !(layer_verts == 4 && (verts_mask & 0x02) != 0);
        let per_norm = has_norm && !((verts_mask & 0x08) != 0);
        for vert in &surf.verts {
            if per_xyz  { if let Some([x, y]) = vert.xy   { w.write_u16(x); w.write_u16(y); } }
            if per_uv   { if let Some([u, v]) = vert.uv   { w.write_u16(u); w.write_u16(v); } }
            if per_norm { if let Some(n)       = vert.norm { w.write_u16(n); } }
        }
        for &[u, v] in &surf.verts_dup_uv {
            w.write_u16(u);
            w.write_u16(v);
        }
    }
}


// ── Lightmaps ─────────────────────────────────────────────────────────────────

pub(crate) fn read_lightmaps(r: &mut Reader, count: i32, version: i32) -> Result<Vec<Lightmap>, OgzError> {
    // Lightmaps were added in v7.
    if version < 7 || count <= 0 {
        return Ok(vec![]);
    }
    if count > MAX_LIGHTMAPS {
        return Err(OgzError::CountOutOfRange {
            field: "num_lightmaps",
            value: count as i64,
            max: MAX_LIGHTMAPS as i64,
        });
    }
    let mut lms = Vec::with_capacity(count as usize);
    for _ in 0..count {
        let (lm_type, unlit_coords) = if version >= 17 {
            let raw       = r.read_u8()?;
            let lm_type   = raw & 0x7F;
            let has_unlit = (raw & 0x80) != 0 && version >= 20;
            let unlit_coords = if has_unlit { Some([r.read_u16()?, r.read_u16()?]) } else { None };
            (lm_type, unlit_coords)
        } else {
            // v7–v16: no type byte; type defaults to 0 (diffuse, 3 BPP).
            (0u8, None)
        };
        let bpp = lightmap_bpp(lm_type);
        let data = r.read_bytes(bpp * 512 * 512)?.to_vec();
        lms.push(Lightmap { lm_type, unlit_coords, data });
    }
    Ok(lms)
}

pub(crate) fn write_lightmaps(w: &mut Writer, lightmaps: &[Lightmap]) {
    for lm in lightmaps {
        let raw = lm.lm_type | if lm.unlit_coords.is_some() { 0x80 } else { 0 };
        w.write_u8(raw);
        if let Some([ux, uy]) = lm.unlit_coords {
            w.write_u16(ux);
            w.write_u16(uy);
        }
        w.write_bytes(&lm.data);
    }
}

#[inline]
fn lightmap_bpp(lm_type: u8) -> usize {
    if (lm_type & 0x10) != 0 && (lm_type & 0x0F) != 2 { 4 } else { 3 }
}

// ── PVS ───────────────────────────────────────────────────────────────────────

pub(crate) fn read_pvs(r: &mut Reader, num_pvs: i32) -> Result<Option<PvsData>, OgzError> {
    if num_pvs <= 0 {
        return Ok(None);
    }
    if num_pvs > MAX_PVS_BLOCKS {
        return Err(OgzError::CountOutOfRange {
            field: "num_pvs",
            value: num_pvs as i64,
            max: MAX_PVS_BLOCKS as i64,
        });
    }
    let _span = tracing::debug_span!("pvs").entered();

    let totallen_raw  = r.read_u32()?;
    let has_water     = (totallen_raw & 0x8000_0000) != 0;
    let buffer_length = totallen_raw & 0x7FFF_FFFF;
    if buffer_length > MAX_PVS_BUFFER {
        return Err(OgzError::CountOutOfRange {
            field: "pvs_buffer_length",
            value: buffer_length as i64,
            max: MAX_PVS_BUFFER as i64,
        });
    }

    let water_heights = if has_water {
        let num_wp = r.read_u32()?;
        if num_wp > MAX_WATER_PLANES {
            return Err(OgzError::CountOutOfRange {
                field: "pvs_water_planes",
                value: num_wp as i64,
                max: MAX_WATER_PLANES as i64,
            });
        }
        let mut wh = Vec::with_capacity(num_wp as usize);
        for _ in 0..num_wp { wh.push(r.read_i32()?); }
        wh
    } else {
        vec![]
    };

    let mut block_lengths = Vec::with_capacity(num_pvs as usize);
    for _ in 0..num_pvs { block_lengths.push(r.read_u16()?); }

    let buffer    = r.read_bytes(buffer_length as usize)?.to_vec();
    let viewcells = read_viewcells(r, 0)?;

    Ok(Some(PvsData { water_heights, block_lengths, buffer, viewcells }))
}

fn read_viewcells(r: &mut Reader, depth: u32) -> Result<ViewcellsNode, OgzError> {
    if depth > MAX_VIEWCELLS_DEPTH {
        return Err(OgzError::DepthLimitExceeded { structure: "viewcells", depth });
    }
    tracing::trace!(depth, "viewcells node");

    let leaf_mask = r.read_u8()?;
    let mut children = Vec::with_capacity(8);
    for bit in 0..8u8 {
        if (leaf_mask & (1 << bit)) != 0 {
            children.push(ViewcellsNode::Leaf(r.read_i32()?));
        } else {
            children.push(read_viewcells(r, depth + 1)?);
        }
    }
    Ok(ViewcellsNode::Branch(Box::new(children.try_into().unwrap())))
}

pub(crate) fn write_pvs(w: &mut Writer, pvs: &Option<PvsData>) {
    let Some(pvs) = pvs else { return };

    let has_water     = !pvs.water_heights.is_empty();
    let buffer_length = pvs.buffer.len() as u32;
    let totallen      = buffer_length | if has_water { 0x8000_0000 } else { 0 };
    w.write_u32(totallen);

    if has_water {
        w.write_u32(pvs.water_heights.len() as u32);
        for &h in &pvs.water_heights { w.write_i32(h); }
    }
    for &bl in &pvs.block_lengths { w.write_u16(bl); }
    w.write_bytes(&pvs.buffer);
    write_viewcells(w, &pvs.viewcells);
}

fn write_viewcells(w: &mut Writer, node: &ViewcellsNode) {
    // The format only ever stores Branch nodes at any given position; leaves are
    // encoded inline by the parent's leaf_mask.  A top-level Leaf is not valid.
    let ViewcellsNode::Branch(children) = node else { return };

    let leaf_mask: u8 = children.iter().enumerate().fold(0, |m, (i, c)| {
        if matches!(c, ViewcellsNode::Leaf(_)) { m | (1u8 << i) } else { m }
    });
    w.write_u8(leaf_mask);
    for child in children.iter() {
        match child {
            ViewcellsNode::Leaf(idx) => w.write_i32(*idx),
            ViewcellsNode::Branch(_) => write_viewcells(w, child),
        }
    }
}

// ── Blendmap ──────────────────────────────────────────────────────────────────

pub(crate) fn read_blendmap(r: &mut Reader, has_blendmap: i32, version: i32) -> Result<Option<BlendmapNode>, OgzError> {
    // v≤28 (compat header): blendmap is stored as a scale scalar in the header,
    // not as a quad-tree in the stream.  Nothing to read here.
    if has_blendmap <= 0 || version <= 28 {
        return Ok(None);
    }
    let _span = tracing::debug_span!("blendmap").entered();
    Ok(Some(read_blendmap_node(r, 0)?))
}

fn read_blendmap_node(r: &mut Reader, depth: u32) -> Result<BlendmapNode, OgzError> {
    if depth > MAX_BLENDMAP_DEPTH {
        return Err(OgzError::DepthLimitExceeded { structure: "blendmap", depth });
    }
    tracing::trace!(depth, "blendmap node");

    match r.read_u8()? {
        0 => { // BRANCH
            let mut children = Vec::with_capacity(4);
            for _ in 0..4 { children.push(read_blendmap_node(r, depth + 1)?); }
            Ok(BlendmapNode::Branch(Box::new(children.try_into().unwrap())))
        }
        1 => Ok(BlendmapNode::Solid(r.read_u8()?)),
        2 => { // IMAGE: 64×64 bytes
            let bytes = r.read_bytes(4096)?;
            let mut arr = Box::new([0u8; 4096]);
            arr.copy_from_slice(bytes);
            Ok(BlendmapNode::Image(arr))
        }
        t => Err(OgzError::UnknownBlendmapNodeType(t)),
    }
}

pub(crate) fn write_blendmap(w: &mut Writer, blendmap: &Option<BlendmapNode>) {
    let Some(node) = blendmap else { return };
    write_blendmap_node(w, node);
}

fn write_blendmap_node(w: &mut Writer, node: &BlendmapNode) {
    match node {
        BlendmapNode::Branch(children) => {
            w.write_u8(0);
            for child in children.iter() { write_blendmap_node(w, child); }
        }
        BlendmapNode::Solid(v)  => { w.write_u8(1); w.write_u8(*v); }
        BlendmapNode::Image(px) => { w.write_u8(2); w.write_bytes(px.as_ref()); }
    }
}
