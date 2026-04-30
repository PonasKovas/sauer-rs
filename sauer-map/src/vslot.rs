use cube_string::CubeString;
use ogz_rs::data::{VSlot, VSlotParam};

pub struct PackedVSlot {
    pub params: Vec<(CubeString, [f32; 4])>,
    pub scale: Option<f32>,
    pub rotation: Option<i32>,
    pub offset: Option<(i32, i32)>,
    pub scroll: Option<(f32, f32)>,
    pub layer: Option<i32>,
    pub alpha: Option<(f32, f32)>,
    pub color: Option<(f32, f32, f32)>,
}

fn read_varint(data: &[u8], pos: &mut usize) -> Option<i32> {
    if *pos >= data.len() {
        return None;
    }
    let c = data[*pos] as i8;
    *pos += 1;
    match c {
        -128 => {
            if *pos + 2 > data.len() {
                return None;
            }
            let v = i16::from_le_bytes([data[*pos], data[*pos + 1]]) as i32;
            *pos += 2;
            Some(v)
        }
        -127 => {
            if *pos + 4 > data.len() {
                return None;
            }
            let v = i32::from_le_bytes(data[*pos..*pos + 4].try_into().ok()?);
            *pos += 4;
            Some(v)
        }
        _ => Some(c as i32),
    }
}

fn read_f32_le(data: &[u8], pos: &mut usize) -> Option<f32> {
    if *pos + 4 > data.len() {
        return None;
    }
    let v = f32::from_le_bytes(data[*pos..*pos + 4].try_into().ok()?);
    *pos += 4;
    Some(v)
}

fn read_sendstring(data: &[u8], pos: &mut usize) -> Option<CubeString> {
    let mut bytes = Vec::new();
    loop {
        let c = read_varint(data, pos)? as u8;
        if c == 0 {
            break;
        }
        bytes.push(c);
    }
    Some(CubeString::from_bytes(bytes))
}

fn read_getuint(data: &[u8], pos: &mut usize) -> Option<i32> {
    let b0 = *data.get(*pos)? as i32;
    *pos += 1;
    if b0 & 0x80 == 0 {
        return Some(b0);
    }
    let b1 = *data.get(*pos)? as i32;
    *pos += 1;
    let mut n = b0 + (b1 << 7) - 0x80;
    if n & (1 << 14) != 0 {
        let b2 = *data.get(*pos)? as i32;
        *pos += 1;
        n += (b2 << 14) - (1 << 14);
    }
    if n & (1 << 21) != 0 {
        let b3 = *data.get(*pos)? as i32;
        *pos += 1;
        n += (b3 << 21) - (1 << 21);
    }
    if n & (1 << 28) != 0 {
        n |= !0i32 << 28;
    }
    Some(n)
}

pub fn parse_packed_vslot(data: &[u8]) -> Option<PackedVSlot> {
    let mut pos = 0;
    let mut params = Vec::new();
    let mut scale = None;
    let mut rotation = None;
    let mut offset = None;
    let mut scroll = None;
    let mut layer = None;
    let mut alpha = None;
    let mut color = None;

    loop {
        if pos >= data.len() {
            break;
        }
        let type_byte = data[pos];
        pos += 1;
        if type_byte >= 0x80 {
            break;
        }
        match type_byte {
            0 => {
                let name = read_sendstring(data, &mut pos)?;
                let vals = [
                    read_f32_le(data, &mut pos)?,
                    read_f32_le(data, &mut pos)?,
                    read_f32_le(data, &mut pos)?,
                    read_f32_le(data, &mut pos)?,
                ];
                params.push((name, vals));
            }
            1 => scale = Some(read_f32_le(data, &mut pos)?),
            2 => rotation = Some(read_varint(data, &mut pos)?),
            3 => offset = Some((read_varint(data, &mut pos)?, read_varint(data, &mut pos)?)),
            4 => scroll = Some((read_f32_le(data, &mut pos)?, read_f32_le(data, &mut pos)?)),
            5 => layer = Some(read_getuint(data, &mut pos)?),
            6 => alpha = Some((read_f32_le(data, &mut pos)?, read_f32_le(data, &mut pos)?)),
            7 => {
                color = Some((
                    read_f32_le(data, &mut pos)?,
                    read_f32_le(data, &mut pos)?,
                    read_f32_le(data, &mut pos)?,
                ))
            }
            _ => return None,
        }
    }
    Some(PackedVSlot {
        params,
        scale,
        rotation,
        offset,
        scroll,
        layer,
        alpha,
        color,
    })
}

pub fn root_base_of(vslots: &[Option<VSlot>], idx: u16) -> i32 {
    let mut current = idx as i32;
    loop {
        match vslots.get(current as usize) {
            Some(Some(vs)) => {
                let p = vs.prev;
                if p < 0 || (p as usize) >= vslots.len() {
                    return current;
                }
                match &vslots[p as usize] {
                    None => return p,
                    Some(_) => current = p,
                }
            }
            _ => return current,
        }
    }
}

pub fn find_chain_tail(vslots: &[Option<VSlot>], root: i32) -> i32 {
    let mut tail = root;
    for i in 0..vslots.len() {
        if let Some(Some(_)) = vslots.get(i) {
            if root_base_of(vslots, i as u16) == root {
                tail = i as i32;
            }
        }
    }
    tail
}

pub fn resolve_vslot(
    vslots: &mut Vec<Option<VSlot>>,
    base_tex: u16,
    ds: &PackedVSlot,
    delta: bool,
) -> u16 {
    if vslots.len() <= base_tex as usize {
        vslots.resize(base_tex as usize + 1, None);
    }
    let root = root_base_of(vslots, base_tex);
    let chain_tail = find_chain_tail(vslots, root);
    let existing = vslots
        .get(base_tex as usize)
        .cloned()
        .flatten()
        .unwrap_or_default();
    let mut new_vs = existing.clone();
    new_vs.prev = chain_tail;

    if !ds.params.is_empty() {
        if delta {
            for (n, v) in &ds.params {
                if let Some(p) = new_vs.params.iter_mut().find(|p| p.name == *n) {
                    p.values = *v;
                } else {
                    new_vs.params.push(VSlotParam {
                        name: n.clone(),
                        values: *v,
                    });
                }
            }
        } else {
            new_vs.params = ds
                .params
                .iter()
                .map(|(n, v)| VSlotParam {
                    name: n.clone(),
                    values: *v,
                })
                .collect();
        }
    }
    if let Some(ds_scale) = ds.scale {
        if delta {
            let cur = new_vs.scale.unwrap_or(1.0);
            new_vs.scale = Some((cur * ds_scale).clamp(1.0 / 8.0, 8.0));
        } else {
            new_vs.scale = Some(ds_scale);
        }
    }
    if let Some(ds_rot) = ds.rotation {
        if delta {
            let cur = new_vs.rotation.unwrap_or(0);
            new_vs.rotation = Some((cur + ds_rot).clamp(0, 7));
        } else {
            new_vs.rotation = Some(ds_rot);
        }
    }
    if let Some(ds_offset) = ds.offset {
        if delta {
            let cur = new_vs.offset.unwrap_or((0, 0));
            new_vs.offset = Some((cur.0 + ds_offset.0, cur.1 + ds_offset.1));
        } else {
            new_vs.offset = Some(ds_offset);
        }
    }
    if let Some(ds_scroll) = ds.scroll {
        if delta {
            let cur = new_vs.scroll.unwrap_or((0.0, 0.0));
            new_vs.scroll = Some((cur.0 + ds_scroll.0, cur.1 + ds_scroll.1));
        } else {
            new_vs.scroll = Some(ds_scroll);
        }
    }
    if let Some(ds_layer) = ds.layer {
        new_vs.layer = Some(ds_layer);
    }
    if let Some(ds_alpha) = ds.alpha {
        new_vs.alpha = Some(ds_alpha);
    }
    if let Some(ds_color) = ds.color {
        if delta {
            let cur = new_vs.color.unwrap_or((1.0, 1.0, 1.0));
            new_vs.color = Some((cur.0 * ds_color.0, cur.1 * ds_color.1, cur.2 * ds_color.2));
        } else {
            new_vs.color = Some(ds_color);
        }
    }

    for (i, vs_opt) in vslots.iter().enumerate() {
        if let Some(vs) = vs_opt {
            if root_base_of(vslots, i as u16) == root {
                if vs.params == new_vs.params
                    && vs.scale == new_vs.scale
                    && vs.rotation == new_vs.rotation
                    && vs.offset == new_vs.offset
                    && vs.scroll == new_vs.scroll
                    && vs.layer == new_vs.layer
                    && vs.alpha == new_vs.alpha
                    && vs.color == new_vs.color
                {
                    return i as u16;
                }
            }
        }
    }
    if vslots.len() < 0x4000 {
        vslots.resize(0x4000, None);
    }
    let idx = vslots.len();
    vslots.push(Some(new_vs));
    idx as u16
}
