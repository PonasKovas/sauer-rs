//! Checks that loading a map keeps every face on the same texture: each
//! face's (engine slot, vslot props) must match between the raw file and
//! the loaded `Map`, after vslot compacting and normalizing.
//!
//! Usage: `cargo run -p sauer-map --example vslot_check -- a.ogz [b.ogz ...]`

use sauer_map::vslot::{root_base_of, slot_of_root};
fn faces(n: &sauer_map::Node, out: &mut Vec<u16>) {
    match n {
        sauer_map::Node::Leaf(c) => out.extend(c.textures),
        sauer_map::Node::Branch(ch) => ch.iter().for_each(|c| faces(c, out)),
    }
}
fn key(vs: &[Option<ogz_rs::data::VSlot>], t: u16) -> (i32, String) {
    let root = match vs.get(t as usize) {
        Some(Some(_)) => root_base_of(vs, t),
        _ => t as i32,
    };
    let props = match vs.get(t as usize) {
        Some(Some(v)) => format!(
            "{:?}{:?}{:?}{:?}{:?}{:?}{:?}",
            v.props.params,
            v.props.scale,
            v.props.rotation,
            v.props.offset,
            v.props.scroll,
            v.props.alpha,
            v.props.color
        ),
        _ => String::new(),
    };
    (slot_of_root(vs, root), props)
}
fn main() {
    for p in std::env::args().skip(1) {
        let bytes = std::fs::read(&p).unwrap();
        let raw = ogz_rs::OgzMap::parse(&bytes).unwrap();
        let mut plain = sauer_map::Map::from_bytes(&bytes).unwrap();
        // the raw table, as the octree was before normalizing
        plain.root = sauer_map::octree::build_node(&raw.octree);
        let map = sauer_map::Map::from_bytes(&bytes).unwrap();
        let (mut a, mut b) = (vec![], vec![]);
        faces(&plain.root, &mut a);
        // map.root may be enlarged; compare only via the original octant
        let mut n = &map.root;
        let mut s = map.size;
        while s > raw.world_size {
            if let sauer_map::Node::Branch(c) = n {
                n = &c[0]
            }
            s /= 2
        }
        faces(n, &mut b);
        let bad = a
            .iter()
            .zip(&b)
            .filter(|(x, y)| key(&raw.vslots, **x) != key(&map.vslots, **y))
            .count();
        println!(
            "{p}: vslots {} -> {}, faces {} vs {}, mismatched {}",
            raw.vslots.len(),
            map.vslots.len(),
            a.len(),
            b.len(),
            bad
        );
    }
}
