use ogz_rs::data::{VSlot, VSlotParam};
use sauer_proto::data::VSlotDelta;

pub fn root_base_of(vslots: &[Option<VSlot>], idx: u16) -> i32 {
    let mut current = idx as i32;
    let mut fuel = 10000;
    loop {
        fuel -= 1;
        if fuel == 0 { return current; }
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
                    return i as u16;
                }
            }
        }
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
        idx as u16
    } else {
        if vslots.len() < min_safe_idx {
            vslots.resize(min_safe_idx, None);
        }
        let idx = vslots.len();
        vslots.push(Some(new_vs));
        idx as u16
    }
}
