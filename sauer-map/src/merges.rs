//! Face merges (the engine's `remip`) carried from the loaded file to the
//! maps the server sends.
//!
//! Merged faces cut the geometry clients build on load several times over
//! (a 1.5M triangle map went to 290k and its "fixing t-joints" from 15s to
//! under 1s). The merge data describes the tree as it was loaded, so it is
//! only written where that part of the tree is still the same:
//!
//! - A merged polygon lives on one "origin" cube and covers faces of other
//!   "member" cubes, which only carry a merged bit. The engine builds them
//!   inside aligned blocks (`maxmerge`, 64 by default), so the whole block
//!   is kept or dropped together. The block size is read off the polygons
//!   (never below 64), so maps merged with a bigger `maxmerge` still work.
//! - Anything else (single-cube surfaces, lightmap info) only depends on its
//!   own cube.
//!
//! Either way a 1 unit border is checked too, since neighbours decide which
//! faces were visible when merging.

use crate::Node;
use crate::geom::{C, R};
use ogz_rs::data::{ModernSurfaces, OctreeNode, Surfaces};
use std::collections::HashMap;

/// `1 << maxmerge` with the engine's default `maxmerge`.
const DEFAULT_MERGE_BLOCK: i32 = 64;

#[derive(Debug)]
pub struct Merges {
    /// The tree the merge data describes.
    original: Node,
    size: i32,
    entries: HashMap<([i32; 3], i32), Entry>,
    /// Size of the aligned blocks merge groups stay within.
    group: i32,
}

#[derive(Debug)]
struct Entry {
    merged: Option<u8>,
    surfaces: Option<ModernSurfaces>,
    /// Part of a merge group (an origin whose polygon leaves its cube, or a
    /// member), rather than self-contained.
    grouped: bool,
}

/// The merge data that can still be written for each leaf, keyed by
/// position and size.
pub struct Kept<'a>(HashMap<([i32; 3], i32), &'a Entry>);

impl Kept<'_> {
    pub fn get(&self, pos: [i32; 3], size: i32) -> Option<(Option<u8>, Option<ModernSurfaces>)> {
        self.0
            .get(&(pos, size))
            .map(|e| (e.merged, e.surfaces.clone()))
    }
}

impl Merges {
    /// None when the file has no merge or surface data.
    pub fn collect(octree: &OctreeNode, original: &Node, size: i32) -> Option<Self> {
        let mut entries = HashMap::new();
        // A group's polygons need not span its whole block, so never go
        // below the engine's default block.
        let mut group = DEFAULT_MERGE_BLOCK.min(size);
        collect(octree, [0; 3], size, size, &mut entries, &mut group);
        (!entries.is_empty()).then(|| Merges {
            original: original.clone(),
            size,
            entries,
            group,
        })
    }

    /// The entries whose part of the tree hasn't changed since load.
    /// `root`/`size` is the current world, which may have been enlarged
    /// (the old world stays in the origin octant).
    pub fn kept(&self, root: &Node, size: i32) -> Kept<'_> {
        let mut out = HashMap::new();
        let mut cur = root;
        let mut cur_size = size;
        while cur_size > self.size {
            match cur {
                Node::Branch(c) => cur = &c[0],
                Node::Leaf(_) => return Kept(out),
            }
            cur_size /= 2;
        }
        if cur_size != self.size {
            return Kept(out);
        }
        let mut blocks: HashMap<[i32; 3], bool> = HashMap::new();
        for (&(pos, s), e) in &self.entries {
            let changed = if e.grouped {
                let g = self.group;
                let block = pos.map(|v| v.div_euclid(g) * g);
                *blocks.entry(block).or_insert_with(|| {
                    self.differs(cur, block.map(|v| v - 1), block.map(|v| v + g + 1))
                })
            } else {
                self.differs(cur, pos.map(|v| v - 1), pos.map(|v| v + s + 1))
            };
            if !changed {
                out.insert((pos, s), e);
            }
        }
        Kept(out)
    }

    fn differs(&self, cur: &Node, lo: [i32; 3], hi: [i32; 3]) -> bool {
        differs(&self.original, cur, [0; 3], self.size, lo, hi)
    }
}

fn differs(a: &Node, b: &Node, o: [i32; 3], s: i32, lo: [i32; 3], hi: [i32; 3]) -> bool {
    if (0..3).any(|k| o[k] >= hi[k] || o[k] + s <= lo[k]) {
        return false;
    }
    if (0..3).all(|k| o[k] >= lo[k] && o[k] + s <= hi[k]) {
        return a != b;
    }
    match (a, b) {
        (Node::Branch(ac), Node::Branch(bc)) => {
            let h = s / 2;
            (0..8).any(|i| {
                let co = std::array::from_fn(|k| o[k] + ((i >> k) & 1) as i32 * h);
                differs(&ac[i], &bc[i], co, h, lo, hi)
            })
        }
        (Node::Leaf(_), Node::Leaf(_)) => a != b,
        _ => true,
    }
}

fn collect(
    n: &OctreeNode,
    o: [i32; 3],
    s: i32,
    world: i32,
    entries: &mut HashMap<([i32; 3], i32), Entry>,
    group: &mut i32,
) {
    let info = match n {
        OctreeNode::Children(c) | OctreeNode::LodCube { children: c, .. } => {
            let h = s / 2;
            for (i, child) in c.iter().enumerate() {
                let co = std::array::from_fn(|k| o[k] + ((i >> k) & 1) as i32 * h);
                collect(child, co, h, world, entries, group);
            }
            return;
        }
        OctreeNode::Empty(info) | OctreeNode::Solid(info) | OctreeNode::Normal { info, .. } => info,
    };
    let Surfaces::Modern(m) = &info.surfaces else {
        return;
    };
    if m.merged.is_none() && m.surfaces.is_none() {
        return;
    }
    let merged = m.merged.unwrap_or(0);
    let mut grouped = false;
    let mut faces = m.surfaces.iter().flat_map(|s| {
        let mut it = s.faces.iter();
        (0..6).map(move |i| {
            if s.surfmask & (1 << i) != 0 {
                it.next()
            } else {
                None
            }
        })
    });
    for orient in 0..6 {
        let face = faces.next().flatten();
        if merged & (1 << orient) == 0 {
            continue;
        }
        let Some(face) = face.filter(|f| f.numverts & 0x0F != 0) else {
            grouped = true; // covered by another cube's polygon
            continue;
        };
        let pts: Vec<[u16; 2]> = match face.xyz {
            Some([c1, r1, c2, r2]) => vec![[c1, r1], [c2, r2]],
            None => face.verts.iter().filter_map(|v| v.xyz).collect(),
        };
        if pts.is_empty() {
            continue; // the cube's own face
        }
        // Vertex coordinates are in eighths, relative to the 4096 block.
        let d = orient / 2;
        let (vc, vr) = (C[d], R[d]);
        let base = o.map(|v| v & !0xFFF);
        let mut lo = o;
        let mut hi = o.map(|v| v + s);
        for [c, r] in &pts {
            for (k, v) in [(vc, *c), (vr, *r)] {
                lo[k] = lo[k].min(base[k] + (v as i32 >> 3));
                hi[k] = hi[k].max(base[k] + ((v as i32 + 7) >> 3));
            }
        }
        if (lo, hi) == (o, o.map(|v| v + s)) {
            continue; // stays on its own cube
        }
        grouped = true;
        // The engine's block holds the origin cube, so across the face the
        // cube alone places it.
        for k in 0..3 {
            (lo[k], hi[k]) = (lo[k].max(0), hi[k].min(world));
        }
        let mut g = s.max(1);
        while g < world && (0..3).any(|k| lo[k].div_euclid(g) != (hi[k] - 1).div_euclid(g)) {
            g *= 2;
        }
        *group = (*group).max(g);
    }
    entries.insert(
        (o, s),
        Entry {
            merged: m.merged,
            surfaces: m.surfaces.clone(),
            grouped,
        },
    );
}
