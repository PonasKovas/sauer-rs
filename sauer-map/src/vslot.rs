use crate::Node;
use ogz_rs::data::{VSlot, VSlotProps};
use sauer_proto::data::clipboard::VSlotData;
use sauer_proto::data::TexRef;
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

/// The texture slot whose base is the unchanged vslot `root` (an index the
/// table has as `None`). The engine hands slots their base vslot by counting
/// unchanged vslots in index order (`emptyvslot` in texture.cpp), so a map
/// with variants stored below `root` (saved by a client with fewer texture
/// slots) has them differ.
pub fn slot_of_root(vslots: &[Option<VSlot>], root: i32) -> i32 {
    let below = &vslots[..(root.max(0) as usize).min(vslots.len())];
    root - below.iter().filter(|v| v.is_some()).count() as i32
}

/// The unchanged vslot that is the base of texture `slot`, the inverse of
/// [`slot_of_root`].
pub fn root_of_slot(vslots: &[Option<VSlot>], slot: u16) -> u16 {
    let mut left = slot as usize;
    for (i, v) in vslots.iter().enumerate() {
        if v.is_none() {
            if left == 0 {
                return i as u16;
            }
            left -= 1;
        }
    }
    (vslots.len() + left) as u16
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
    ds: &VSlotProps,
    delta: bool,
) -> u16 {
    lookup_vslot(vslots, base_tex, ds, delta, true).expect("inserting lookup always succeeds")
}

/// Like [`resolve_vslot`], but with `insert == false` only finds an existing
/// variant (the engine's `findvslot`).
pub fn lookup_vslot(
    vslots: &mut Vec<Option<VSlot>>,
    base_tex: u16,
    ds: &VSlotProps,
    delta: bool,
    insert: bool,
) -> Option<u16> {
    if vslots.len() <= base_tex as usize {
        vslots.resize(base_tex as usize + 1, None);
    }
    let root = root_base_of(vslots, base_tex);
    let chain_tail = find_chain_tail(vslots, root);
    let mut props = match vslots.get(base_tex as usize) {
        Some(Some(vs)) => vs.props.clone(),
        _ => VSlotProps::default(),
    };
    props.merge(ds, delta);

    for (i, vs_opt) in vslots.iter().enumerate() {
        if let Some(vs) = vs_opt {
            if root_base_of(vslots, i as u16) == root && vs.props == props {
                return Some(i as u16);
            }
        }
    }
    if !insert {
        return None;
    }

    // Always at the end: an empty entry is some slot's base (see
    // `slot_of_root`), and filling it would shift every slot after it.
    vslots.push(Some(VSlot {
        prev: chain_tail,
        props,
    }));
    Some((vslots.len() - 1) as u16)
}

/// Resolves a texture a client sent (`N_EDITT`, `N_REPLACE`) to a local
/// index, like the engine's `unpacktex`. Plain indices are returned as they
/// are. `None` when `insert` is false and no such variant exists.
pub fn resolve_tex(vslots: &mut Vec<Option<VSlot>>, tex: &TexRef, insert: bool) -> Option<u16> {
    match tex {
        TexRef::Index(i) => Some(*i),
        TexRef::Variant { slot, vslot } => {
            let root = root_of_slot(vslots, *slot);
            if vslot.is_empty() {
                return Some(root);
            }
            lookup_vslot(vslots, root, vslot, false, insert)
        }
    }
}

/// Brings a client's packed vslots (from a clipboard or undo block) into this
/// table, like the engine's `unpackvslots`, and points `nodes` at them.
/// Texture indices without packed data are left alone, like the engine does.
pub fn import(vslots: &mut Vec<Option<VSlot>>, incoming: &[VSlotData], nodes: &mut [Node]) {
    let incoming: Vec<_> = incoming
        .iter()
        .filter(|vd| vd.index != 0 && vd.slot != 0) // engine: end marker / sky can't be edited
        .map(|vd| (vd, root_of_slot(vslots, vd.slot)))
        .collect();
    // Bases past the end must exist before variants get appended there.
    if let Some(max) = incoming.iter().map(|(_, r)| *r as usize).max() {
        if vslots.len() <= max {
            vslots.resize(max + 1, None);
        }
    }
    let mut map = HashMap::new();
    for (vd, root) in incoming {
        map.insert(
            vd.index,
            resolve_vslot(vslots, root, &vd.vslot, false),
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
