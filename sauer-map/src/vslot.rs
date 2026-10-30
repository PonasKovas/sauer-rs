use crate::Node;
use ogz_rs::data::{VSlot, VSlotParam};
use sauer_proto::data::clipboard::VSlotData;
use sauer_proto::data::{VSlotDelta, VSlotDeltaParam};
use std::collections::HashMap;

pub fn root_base_of(vslots: &[Option<VSlot>], idx: u16) -> i32 {
    let mut current = idx as i32;
    let mut fuel = 10000;
    loop {
        fuel -= 1;
        if fuel == 0 {
            return current;
        }
        match vslots.get(current as usize) {
            Some(Some(vs)) => {
                let p = vs.prev;
                if p < 0 || (p as usize) >= vslots.len() || p == current {
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
    let mut is_parent = vec![false; vslots.len()];
    for vs_opt in vslots.iter() {
        if let Some(vs) = vs_opt {
            if vs.prev >= 0 && (vs.prev as usize) < vslots.len() {
                is_parent[vs.prev as usize] = true;
            }
        }
    }

    for i in 0..vslots.len() {
        if let Some(Some(_)) = vslots.get(i) {
            if !is_parent[i] && root_base_of(vslots, i as u16) == root {
                return i as i32;
            }
        }
    }
    root
}

pub fn resolve_vslot(
    vslots: &mut Vec<Option<VSlot>>,
    base_tex: u16,
    ds: &VSlotDelta,
    delta: bool,
) -> u16 {
    lookup_vslot(vslots, base_tex, ds, delta, true).expect("inserting lookup always succeeds")
}

/// Like [`resolve_vslot`], but with `insert == false` only finds an existing
/// variant (the engine's `findvslot`).
pub fn lookup_vslot(
    vslots: &mut Vec<Option<VSlot>>,
    base_tex: u16,
    ds: &VSlotDelta,
    delta: bool,
    insert: bool,
) -> Option<u16> {
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
            for p_delta in &ds.params {
                if let Some(p) = new_vs.params.iter_mut().find(|p| p.name == p_delta.name) {
                    p.values = p_delta.values;
                } else {
                    new_vs.params.push(VSlotParam {
                        name: p_delta.name.clone(),
                        values: p_delta.values,
                    });
                }
            }
        } else {
            new_vs.params = ds
                .params
                .iter()
                .map(|p_delta| VSlotParam {
                    name: p_delta.name.clone(),
                    values: p_delta.values,
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
                    return Some(i as u16);
                }
            }
        }
    }
    if !insert {
        return None;
    }

    let mut insert_idx = None;
    let min_safe_idx = 1704;
    for i in min_safe_idx..vslots.len() {
        if vslots[i].is_none() {
            insert_idx = Some(i);
            break;
        }
    }

    if let Some(idx) = insert_idx {
        vslots[idx] = Some(new_vs);
        Some(idx as u16)
    } else {
        if vslots.len() < min_safe_idx {
            vslots.resize(min_safe_idx, None);
        }
        let idx = vslots.len();
        vslots.push(Some(new_vs));
        Some(idx as u16)
    }
}

/// The props of a vslot as a full (non-delta) description.
pub fn to_delta(vs: &VSlot) -> VSlotDelta {
    VSlotDelta {
        params: vs
            .params
            .iter()
            .map(|p| VSlotDeltaParam {
                name: p.name.clone(),
                values: p.values,
            })
            .collect(),
        scale: vs.scale,
        rotation: vs.rotation,
        offset: vs.offset,
        scroll: vs.scroll,
        layer: vs.layer,
        alpha: vs.alpha,
        color: vs.color,
    }
}

/// Resolves a texture a client sent as `0x10000 | slot` plus a packed vslot
/// (`N_EDITT`, `N_REPLACE`) to a local index, like the engine's `unpacktex`.
/// Plain indices are returned as they are. `None` when `insert` is false and
/// no such variant exists, or the data is malformed.
pub fn unpack_tex(
    vslots: &mut Vec<Option<VSlot>>,
    tex: i32,
    extra: &mut &[u8],
    insert: bool,
) -> Option<u16> {
    if tex < 0x10000 {
        return Some(tex as u16);
    }
    let ds = VSlotDelta::read(extra).ok()?;
    let slot = (tex & 0xFFFF) as u16;
    if ds.is_empty() {
        return Some(slot);
    }
    lookup_vslot(vslots, slot, &ds, false, insert)
}

/// Brings a client's packed vslots (from a clipboard or undo block) into this
/// table, like the engine's `unpackvslots`, and points `nodes` at them.
/// Texture indices without packed data are the same on every client (plain
/// slots), so they are left alone.
pub fn import(vslots: &mut Vec<Option<VSlot>>, incoming: &[VSlotData], nodes: &mut [Node]) {
    let mut map = HashMap::new();
    for vd in incoming {
        if vd.index == 0 || vd.slot == 0 {
            continue; // engine: end marker / sky can't be edited
        }
        map.insert(
            vd.index,
            resolve_vslot(vslots, vd.slot, &to_delta(&vd.vslot), false),
        );
    }
    if map.is_empty() {
        return;
    }
    fn remap(node: &mut Node, map: &HashMap<u16, u16>) {
        match node {
            Node::Leaf(c) => {
                for t in &mut c.textures {
                    if let Some(n) = map.get(t) {
                        *t = *n;
                    }
                }
            }
            Node::Branch(ch) => ch.iter_mut().for_each(|c| remap(c, map)),
        }
    }
    nodes.iter_mut().for_each(|n| remap(n, &map));
}
