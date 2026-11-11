//! Face merges like the engine's `remip`, for the maps the server sends.
//!
//! Clients build one polygon per visible cube face on load, and the engine's
//! t-joint pass is quadratic in the faces along a line, so maps made of tiny
//! cubes take seconds to load (15s for dr_LITTLE-RACE-31). `/remip` fixes
//! that, but `/sendmap` uploads maps without merge data, so the server makes
//! its own when writing the map.
//!
//! Faces of non-alpha cubes smaller than a merge block are merged when they
//! are flat four-corner faces the engine always draws whole, with one texture
//! inside each aligned block (like the engine with its default `maxmerge`):
//!
//! - full squares parallel to the cube's side are joined into rectangles;
//! - other faces spanning the whole cube along one axis (slopes, curbs) are
//!   joined with the same shape repeated along that axis.
//!
//! One polygon lives on an "origin" cube; the faces it covers only get a
//! merged bit. Everything else is left for the client to draw as before.

use crate::geom::{C, R, get_vertex};
use crate::octree::leaf_to_ogz;
use crate::{Cube, Node};
use ogz_rs::data::{ModernFace, ModernSurfaces, ModernVert, OctreeNode};
use std::collections::HashMap;

/// `1 << maxmerge` with the engine's default `maxmerge`.
const BLOCK: i32 = 64;

/// [`crate::octree::node_to_ogz`] with merged faces.
pub fn node_to_ogz_merged(root: &Node, size: i32) -> OctreeNode {
    convert(root, [0; 3], size, [None; 6])
}

/// The nodes across each face (orient = 2 * axis + positive side), the same
/// size or a bigger leaf; None past the world edge.
type Nbrs<'a> = [Option<&'a Node>; 6];

fn child_pos(pos: [i32; 3], half: i32, i: usize) -> [i32; 3] {
    std::array::from_fn(|k| pos[k] + ((i >> k) & 1) as i32 * half)
}

fn child_nbrs<'a>(children: &'a [Node; 8], nbrs: &Nbrs<'a>, i: usize) -> Nbrs<'a> {
    std::array::from_fn(|o| {
        let d = o / 2;
        if (i >> d) & 1 != o & 1 {
            Some(&children[i ^ (1 << d)])
        } else {
            match nbrs[o]? {
                Node::Branch(nc) => Some(&nc[i ^ (1 << d)]),
                leaf => Some(leaf),
            }
        }
    })
}

fn convert(n: &Node, pos: [i32; 3], size: i32, nbrs: Nbrs) -> OctreeNode {
    match n {
        Node::Leaf(cube) => leaf_to_ogz(cube, None, None),
        Node::Branch(_) if size == BLOCK => {
            let mut faces = Vec::new();
            collect(n, pos, size, nbrs, &mut faces);
            emit(n, pos, size, &merge_block(pos, faces))
        }
        Node::Branch(c) => OctreeNode::Children(Box::new(std::array::from_fn(|i| {
            let cn = child_nbrs(c, &nbrs, i);
            convert(&c[i], child_pos(pos, size / 2, i), size / 2, cn)
        }))),
    }
}

/// Coordinates are in eighths of a world unit, like the engine's vertices.
struct Face {
    pos: [i32; 3],
    size: i32,
    orient: usize,
    tex: u16,
    kind: Kind,
}

#[derive(Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
enum Kind {
    /// A full square at `plane` along the face's axis.
    Square { plane: i32 },
    /// Spans `axis` fully; `edge` is its two corners at the low end, as the
    /// other two coordinates (in axis order), so faces with the same `edge`
    /// line up into one strip.
    Strip { axis: usize, edge: [[i32; 2]; 2] },
}

fn collect(n: &Node, pos: [i32; 3], size: i32, nbrs: Nbrs, out: &mut Vec<Face>) {
    match n {
        Node::Branch(c) => {
            for i in 0..8 {
                let cn = child_nbrs(c, &nbrs, i);
                collect(&c[i], child_pos(pos, size / 2, i), size / 2, cn, out);
            }
        }
        Node::Leaf(cube) if !cube.is_empty() && !cube.material.alpha => {
            for orient in 0..6 {
                let Some((kind, touching)) = face_kind(cube, pos, size, orient) else {
                    continue;
                };
                // With 2 corners or fewer on the cube's side the engine draws
                // the face whole (`notouch` in `visibletris`); otherwise only
                // if nothing across can cover it.
                if touching > 2 && !nbrs[orient].is_some_and(|nb| side_clear(nb, orient ^ 1)) {
                    continue;
                }
                out.push(Face {
                    pos,
                    size,
                    orient,
                    tex: cube.textures[orient],
                    kind,
                });
            }
        }
        Node::Leaf(_) => {}
    }
}

/// What the face can merge as, and how many of its corners are on the
/// cube's side. None for faces that aren't flat, non-degenerate quads.
fn face_kind(cube: &Cube, pos: [i32; 3], size: i32, orient: usize) -> Option<(Kind, usize)> {
    let (d, coord) = (orient / 2, orient & 1);
    if cube.is_entirely_solid() {
        // Most leaves; skip working out the corners.
        let plane = (pos[d] + coord as i32 * size) * 8;
        return Some((Kind::Square { plane }, 4));
    }
    let mut local = [[0i32; 3]; 4];
    for (i, (x, y)) in [(0, 0), (1, 0), (1, 1), (0, 1)].into_iter().enumerate() {
        let p = get_vertex(cube, d, x, y, coord);
        let back = get_vertex(cube, d, x, y, coord ^ 1)[d];
        let thick = if coord == 1 { back < p[d] } else { back > p[d] };
        if !thick {
            return None;
        }
        local[i] = p.map(|v| v as i32);
    }
    let sub = |a: [i32; 3], b: [i32; 3]| std::array::from_fn::<i32, 3, _>(|k| a[k] - b[k]);
    let (e1, e2, e3) = (
        sub(local[1], local[0]),
        sub(local[2], local[0]),
        sub(local[3], local[0]),
    );
    let n = [
        e1[1] * e2[2] - e1[2] * e2[1],
        e1[2] * e2[0] - e1[0] * e2[2],
        e1[0] * e2[1] - e1[1] * e2[0],
    ];
    if n == [0; 3] || (0..3).map(|k| e3[k] * n[k]).sum::<i32>() != 0 {
        return None;
    }
    let touching = local.iter().filter(|p| p[d] == coord as i32 * 8).count();
    let abs = |p: [i32; 3]| std::array::from_fn::<i32, 3, _>(|k| pos[k] * 8 + p[k] * size);
    let depth = local[0][d];
    let square = local
        .iter()
        .zip([(0, 0), (8, 0), (8, 8), (0, 8)])
        .all(|(p, (x, y))| p[d] == depth && p[R[d]] == x && p[C[d]] == y);
    if square {
        let plane = abs(local[0])[d];
        return Some((Kind::Square { plane }, touching));
    }
    for axis in [R[d], C[d]] {
        let (lo, hi): (Vec<&[i32; 3]>, Vec<&[i32; 3]>) = local.iter().partition(|p| p[axis] == 0);
        if lo.len() != 2 || hi.iter().any(|p| p[axis] != 8) {
            continue;
        }
        let rest = |p: &[i32; 3]| {
            let mut k = (0..3).filter(|&k| k != axis);
            let a = abs(*p);
            [a[k.next().unwrap()], a[k.next().unwrap()]]
        };
        let lo_rest = [rest(lo[0]), rest(lo[1])];
        let mut hi_rest = [rest(hi[0]), rest(hi[1])];
        let mut lo_sorted = lo_rest;
        lo_sorted.sort();
        hi_rest.sort();
        if lo_sorted != hi_rest || lo_sorted[0] == lo_sorted[1] {
            continue;
        }
        return Some((
            Kind::Strip {
                axis,
                edge: lo_sorted,
            },
            touching,
        ));
    }
    None
}

/// Nothing on the `orient` side of the node can cover a face against it:
/// every leaf there is empty, alpha, or has no corner on that side
/// (`notouchingface`), the cases `visibletris` and `occludesface` treat as
/// not covering.
fn side_clear(n: &Node, orient: usize) -> bool {
    match n {
        Node::Leaf(c) => {
            let (d, coord) = (orient / 2, orient & 1);
            let edges = &c.edges[4 * d..4 * d + 4];
            let no_touch = if coord == 1 {
                edges.iter().all(|e| e >> 4 != 8)
            } else {
                edges.iter().all(|e| e & 0xF != 0)
            };
            c.is_empty() || c.material.alpha || no_touch
        }
        Node::Branch(c) => {
            let d = orient / 2;
            (0..8)
                .filter(|i| (i >> d) & 1 == orient & 1)
                .all(|i| side_clear(&c[i], orient))
        }
    }
}

/// A merged polygon in absolute eighths, as (C, R) coordinates of the face's
/// axis.
enum Poly {
    Rect { min: [i32; 2], max: [i32; 2] },
    Quad([[i32; 2]; 4]),
}

#[derive(Default)]
struct LeafMerge {
    merged: u8,
    /// Per orient, on origin cubes.
    polys: [Option<Poly>; 6],
}

type Merges = HashMap<([i32; 3], i32), LeafMerge>;

fn add_merge(out: &mut Merges, group: &[Face], members: &[usize], origin: usize, poly: Poly) {
    let orient = group[origin].orient;
    for &m in members {
        let f = &group[m];
        out.entry((f.pos, f.size)).or_default().merged |= 1 << orient;
    }
    let o = &group[origin];
    out.entry((o.pos, o.size)).or_default().polys[orient] = Some(poly);
}

/// Joins the block's faces, keyed by the leaf they touch.
fn merge_block(block: [i32; 3], mut faces: Vec<Face>) -> Merges {
    let mut out = Merges::new();
    faces.sort_by_key(|f| (f.orient, f.tex, f.kind));
    // Face index + 1 per in-plane cell, 0 when free.
    let mut grid = vec![0u32; (BLOCK * BLOCK) as usize];
    for group in faces.chunk_by(|a, b| (a.orient, a.tex, a.kind) == (b.orient, b.tex, b.kind)) {
        if group.len() < 2 {
            continue;
        }
        match group[0].kind {
            Kind::Square { .. } => merge_squares(block, group, &mut grid, &mut out),
            Kind::Strip { axis, edge } => merge_strip(group, axis, edge, &mut out),
        }
    }
    out
}

fn merge_squares(block: [i32; 3], group: &[Face], grid: &mut [u32], out: &mut Merges) {
    let d = group[0].orient / 2;
    let cr = |f: &Face| [f.pos[C[d]] - block[C[d]], f.pos[R[d]] - block[R[d]]];
    for (i, f) in group.iter().enumerate() {
        let [c, r] = cr(f);
        for y in r..r + f.size {
            for x in c..c + f.size {
                grid[(y * BLOCK + x) as usize] = i as u32 + 1;
            }
        }
    }
    let at = |grid: &[u32], x: i32, y: i32| grid[(y * BLOCK + x) as usize];
    let mut order: Vec<usize> = (0..group.len()).collect();
    order.sort_by_key(|&i| {
        let [c, r] = cr(&group[i]);
        (r, c)
    });
    for &start in &order {
        let [c0, r0] = cr(&group[start]);
        if at(grid, c0, r0) == 0 {
            continue; // taken by an earlier rectangle
        }
        let s = group[start].size;
        // Widen one column at a time; faces met must stay in the rows, and
        // the width counts once every face met ends inside it.
        let (mut c1, mut reach, mut width_ok) = (c0 + s, c0 + s, c0 + s);
        'widen: while c1 < BLOCK {
            for y in r0..r0 + s {
                let id = at(grid, c1, y);
                if id == 0 {
                    break 'widen;
                }
                let [fc, fr] = cr(&group[id as usize - 1]);
                let fs = group[id as usize - 1].size;
                if fr < r0 || fr + fs > r0 + s {
                    break 'widen;
                }
                reach = reach.max(fc + fs);
            }
            c1 += 1;
            if reach == c1 {
                width_ok = c1;
            }
        }
        let c1 = width_ok;
        let (mut r1, mut reach, mut height_ok) = (r0 + s, r0 + s, r0 + s);
        'grow: while r1 < BLOCK {
            for x in c0..c1 {
                let id = at(grid, x, r1);
                if id == 0 {
                    break 'grow;
                }
                let [fc, fr] = cr(&group[id as usize - 1]);
                let fs = group[id as usize - 1].size;
                if fc < c0 || fc + fs > c1 {
                    break 'grow;
                }
                reach = reach.max(fr + fs);
            }
            r1 += 1;
            if reach == r1 {
                height_ok = r1;
            }
        }
        let r1 = height_ok;
        let mut members = Vec::new();
        for y in r0..r1 {
            for x in c0..c1 {
                let id = std::mem::take(&mut grid[(y * BLOCK + x) as usize]);
                if id != 0 && cr(&group[id as usize - 1]) == [x, y] {
                    members.push(id as usize - 1);
                }
            }
        }
        if members.len() >= 2 {
            let eighths = |c: i32, r: i32| [(block[C[d]] + c) * 8, (block[R[d]] + r) * 8];
            let poly = Poly::Rect {
                min: eighths(c0, r0),
                max: eighths(c1, r1),
            };
            add_merge(out, group, &members, start, poly);
        }
    }
    grid.fill(0);
}

fn merge_strip(group: &[Face], axis: usize, edge: [[i32; 2]; 2], out: &mut Merges) {
    let d = group[0].orient / 2;
    let mut order: Vec<usize> = (0..group.len()).collect();
    order.sort_by_key(|&i| group[i].pos[axis]);
    let mut run = vec![order[0]];
    for k in 1..=order.len() {
        let joins = order.get(k).is_some_and(|&i| {
            let last = &group[*run.last().unwrap()];
            group[i].pos[axis] == last.pos[axis] + last.size
        });
        if joins {
            run.push(order[k]);
            continue;
        }
        if run.len() >= 2 {
            let first = &group[run[0]];
            let last = &group[*run.last().unwrap()];
            let (lo, hi) = (first.pos[axis] * 8, (last.pos[axis] + last.size) * 8);
            let corner = |e: [i32; 2], a: i32| {
                let mut p = [0; 3];
                let mut k = (0..3).filter(|&k| k != axis);
                p[k.next().unwrap()] = e[0];
                p[k.next().unwrap()] = e[1];
                p[axis] = a;
                [p[C[d]], p[R[d]]]
            };
            let poly = [
                corner(edge[0], lo),
                corner(edge[0], hi),
                corner(edge[1], hi),
                corner(edge[1], lo),
            ];
            if let Some(poly) = wind(poly, group[0].orient) {
                add_merge(out, group, &run, run[0], Poly::Quad(poly));
            }
        }
        if let Some(&i) = order.get(k) {
            run = vec![i];
        }
    }
}

/// Orders the corners like the engine's merged faces: counter-clockwise in
/// (C, R) on a side's low face, clockwise on its high face.
fn wind(mut poly: [[i32; 2]; 4], orient: usize) -> Option<[[i32; 2]; 4]> {
    let area: i64 = (0..4)
        .map(|i| {
            let (a, b) = (poly[i], poly[(i + 1) % 4]);
            a[0] as i64 * b[1] as i64 - b[0] as i64 * a[1] as i64
        })
        .sum();
    if area == 0 {
        return None;
    }
    if (area > 0) != (orient & 1 == 0) {
        poly.reverse();
    }
    Some(poly)
}

fn emit(n: &Node, pos: [i32; 3], size: i32, merges: &Merges) -> OctreeNode {
    match n {
        Node::Branch(c) => OctreeNode::Children(Box::new(std::array::from_fn(|i| {
            emit(&c[i], child_pos(pos, size / 2, i), size / 2, merges)
        }))),
        Node::Leaf(cube) => match merges.get(&(pos, size)) {
            Some(m) => leaf_to_ogz(cube, Some(m.merged), surfaces(pos, m)),
            None => leaf_to_ogz(cube, None, None),
        },
    }
}

/// The origin's polygons as the engine saves merged faces: coordinates
/// relative to the 4096 block, rectangles as two corners (the order on each
/// side matches what `savec` writes).
fn surfaces(pos: [i32; 3], m: &LeafMerge) -> Option<ModernSurfaces> {
    let mut faces = Vec::new();
    let mut surfmask = 0;
    for (orient, poly) in m.polys.iter().enumerate() {
        let Some(poly) = poly else { continue };
        let d = orient / 2;
        let base = [(pos[C[d]] & !0xFFF) * 8, (pos[R[d]] & !0xFFF) * 8];
        let rel = |p: [i32; 2]| [(p[0] - base[0]) as u16, (p[1] - base[1]) as u16];
        let (vertmask, xyz, verts) = match poly {
            Poly::Rect { min, max } => {
                let ([c0, r0], [c1, r1]) = (rel(*min), rel(*max));
                let corners = if orient & 1 == 0 {
                    [c0, r1, c1, r0]
                } else {
                    [c0, r0, c1, r1]
                };
                (0x0d, Some(corners), vec![])
            }
            Poly::Quad(q) => {
                let verts = q
                    .iter()
                    .map(|&p| ModernVert {
                        xyz: Some(rel(p)),
                        uv: None,
                        norm: None,
                    })
                    .collect();
                (0x0c, None, verts)
            }
        };
        surfmask |= 1 << orient;
        faces.push(ModernFace {
            lmid: [2, 0], // LMID_BRIGHT, LMID_AMBIENT
            vertmask,
            numverts: 0x20 | 4, // LAYER_TOP, 4 corners
            xyz,
            uv: None,
            dup_uv: None,
            norm: None,
            verts,
            dup_verts: vec![],
        });
    }
    (!faces.is_empty()).then(|| ModernSurfaces {
        surfmask,
        totalverts: 4 * faces.len() as u8,
        faces,
    })
}
