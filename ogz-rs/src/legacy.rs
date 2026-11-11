//! Cube data from maps before v32, with the fixes the engine's `loadc`
//! applies to even older versions.

use crate::data::*;
use crate::error::OgzError;
use byteorder::{LittleEndian, ReadBytesExt};
use std::io::Read;

/// The engine's `LMID_*`.
const LMID_AMBIENT1: u8 = 1;
const LMID_BRIGHT1: u8 = 3;
const LMID_DARK: u8 = 4;

pub(crate) fn read_surfaces<R: Read>(
    r: &mut R,
    version: i32,
    octsav: u8,
) -> Result<LegacySurfaceData, OgzError> {
    let mask = r.read_u8()?;
    let material = if mask & 0x80 != 0 {
        Some(crate::de::old_material(r.read_u8()?, version))
    } else {
        None
    };

    let mut faces: [Option<LegacyFace>; 6] = Default::default();
    if mask & 0x3F != 0 {
        // Blend layers come after all the faces, in face order.
        let mut blended = Vec::new();
        for (i, face) in faces.iter_mut().enumerate() {
            if mask & (1 << i) == 0 {
                continue;
            }
            let surface = read_surface(r, version)?;
            let normals = if mask & 0x40 != 0 {
                let mut n = [[0; 3]; 4];
                for v in &mut n {
                    r.read_exact(v)?;
                }
                Some(n)
            } else {
                None
            };
            if surface.layer & 2 != 0 {
                blended.push(i);
            }
            *face = Some(LegacyFace {
                surface,
                normals,
                blend: None,
            });
        }
        for i in blended {
            let blend = read_surface(r, version)?;
            if let Some(face) = &mut faces[i] {
                face.blend = Some(blend);
            }
        }
    }

    let mut merged = None;
    let mut merges: [Option<LegacyMerge>; 6] = Default::default();
    if version >= 20 && octsav & 0x80 != 0 {
        let m = r.read_u8()?;
        merged = Some(m & 0x3F);
        if m & 0x80 != 0 {
            let merge_mask = r.read_u8()?;
            for (i, merge) in merges.iter_mut().enumerate() {
                if merge_mask & (1 << i) == 0 {
                    continue;
                }
                let mut m = LegacyMerge {
                    u1: r.read_u16::<LittleEndian>()?,
                    u2: r.read_u16::<LittleEndian>()?,
                    v1: r.read_u16::<LittleEndian>()?,
                    v2: r.read_u16::<LittleEndian>()?,
                };
                if version <= 25 {
                    let (uo, vo) = (m.u1 & 0xE000, m.v1 & 0xE000);
                    m.u1 = m.u1.wrapping_sub(uo) << 2;
                    m.u2 = m.u2.wrapping_sub(uo) << 2;
                    m.v1 = m.v1.wrapping_sub(vo) << 2;
                    m.v2 = m.v2.wrapping_sub(vo) << 2;
                }
                *merge = Some(m);
            }
        }
    }

    // Before v12 faces 0, 1 and 4, 5 were stored the other way around.
    if version <= 11 {
        faces.swap(0, 4);
        faces.swap(1, 5);
    }

    Ok(LegacySurfaceData {
        material,
        faces,
        merged,
        merges,
    })
}

fn read_surface<R: Read>(r: &mut R, version: i32) -> Result<LegacySurface, OgzError> {
    let mut texcoords = [0; 8];
    r.read_exact(&mut texcoords)?;
    let w = r.read_u8()?;
    let h = r.read_u8()?;
    let x = r.read_u16::<LittleEndian>()?;
    let y = r.read_u16::<LittleEndian>()?;
    let mut lmid = r.read_u8()?;
    let layer = r.read_u8()?;
    // Lightmap ids that were added over time.
    if version < 10 {
        lmid = lmid.wrapping_add(1);
    }
    if version < 18 {
        if lmid >= LMID_AMBIENT1 {
            lmid = lmid.wrapping_add(1);
        }
        if lmid >= LMID_BRIGHT1 {
            lmid = lmid.wrapping_add(1);
        }
    }
    if version < 19 && lmid >= LMID_DARK {
        lmid = lmid.wrapping_add(2);
    }
    Ok(LegacySurface {
        texcoords,
        w,
        h,
        x,
        y,
        lmid,
        layer,
    })
}

/// The edges and textures of a v7 to v31 cube as the engine has them after
/// loading. Maps before v7 get no fixes, as in the engine.
pub(crate) fn fix_cube(version: i32, edges: Option<&mut [u8; 12]>, textures: &mut [u16; 6]) {
    if !(7..=11).contains(&version) {
        return;
    }
    if let Some(edges) = edges {
        if version <= 8 {
            edgespan_to_vector(edges);
        }
        // `swap(c.faces[0], c.faces[2])`
        for i in 0..4 {
            edges.swap(i, 8 + i);
        }
    }
    textures.swap(0, 4);
    textures.swap(1, 5);
}

type Vec3 = [f32; 3];

fn cross(a: Vec3, b: Vec3) -> Vec3 {
    [
        a[1] * b[2] - a[2] * b[1],
        a[2] * b[0] - a[0] * b[2],
        a[0] * b[1] - a[1] * b[0],
    ]
}

fn dot(a: Vec3, b: Vec3) -> f32 {
    a[0] * b[0] + a[1] * b[1] + a[2] * b[2]
}

/// The engine's `cubeedge` index.
fn edge_index(dim: usize, x: usize, y: usize) -> usize {
    (dim << 2) + (y << 1) + x
}

fn edge_get(edge: u8, high: bool) -> u8 {
    if high { edge >> 4 } else { edge & 0xF }
}

/// A plane as the engine's `plane`: a normal and an offset.
#[derive(PartialEq, Clone, Copy)]
struct Plane(Vec3, f32);

/// The engine's `genvertp`.
fn vert_plane(edges: &[u8; 12], p: [[i32; 3]; 3], solid: bool) -> Plane {
    const R: [usize; 3] = [1, 2, 0];
    const C: [usize; 3] = [2, 0, 1];
    let dim = if p[0][1] == p[1][1] && p[1][1] == p[2][1] {
        1
    } else if p[0][2] == p[1][2] && p[1][2] == p[2][2] {
        2
    } else {
        0
    };
    let coord = p[0][dim];
    let v = p.map(|q| {
        let mut v = q;
        v[dim] = if solid {
            coord * 8
        } else {
            let e = edges[edge_index(dim, (q[R[dim]] >> 3) as usize, (q[C[dim]] >> 3) as usize)];
            i32::from(edge_get(e, coord != 0))
        };
        v.map(|c| c as f32)
    });
    let sub = |a: Vec3, b: Vec3| [a[0] - b[0], a[1] - b[1], a[2] - b[2]];
    let mut n = cross(sub(v[1], v[0]), sub(v[2], v[0]));
    let mag = dot(n, n).sqrt();
    if mag == 0.0 {
        return Plane(n, 0.0);
    }
    n = n.map(|c| c / mag);
    Plane(n, -dot(n, v[0]))
}

/// The engine's `threeplaneintersect`, including what it leaves in `dest`
/// when the planes don't meet.
fn three_plane_intersect(a: Plane, b: Plane, c: Plane) -> Vec3 {
    let t4 = cross(a.0, b.0);
    let t1 = t4.map(|v| v * c.1);
    let t2 = cross(c.0, a.0).map(|v| v * b.1);
    let t3 = cross(b.0, c.0).map(|v| v * a.1);
    let t = [0, 1, 2].map(|i| -(t1[i] + t2[i] + t3[i]));
    let d = dot(t4, c.0);
    if d == 0.0 { t } else { t.map(|v| v / d) }
}

/// The engine's `edgespan2vectorcube`: before v9 edges held spans that are
/// now corner positions.
fn edgespan_to_vector(edges: &mut [u8; 12]) {
    let empty = edges[..4] == [0; 4];
    let solid = *edges == [0x80; 12];
    if empty || solid {
        return;
    }
    let o = *edges;
    for x in 0..2 {
        for y in 0..2 {
            for z in 0..2 {
                let p = [8 * x, 8 * y, 8 * z];
                let p1 = [8 - p[0], p[1], p[2]];
                let p2 = [p[0], 8 - p[1], p[2]];
                let p3 = [p[0], p[1], 8 - p[2]];
                let mut pl1 = vert_plane(&o, [p, p1, p2], false);
                let mut pl2 = vert_plane(&o, [p, p2, p3], false);
                let pl3 = vert_plane(&o, [p, p3, p1], false);
                if pl1 == pl2 {
                    pl1 = vert_plane(&o, [p, p1, p2], true);
                }
                if pl1 == pl3 {
                    pl1 = vert_plane(&o, [p, p1, p2], true);
                }
                if pl2 == pl3 {
                    pl2 = vert_plane(&o, [p, p2, p3], true);
                }
                let v =
                    three_plane_intersect(pl1, pl2, pl3).map(|c| (c.clamp(0.0, 8.0) + 0.49) as u8);

                let (x, y, z) = (x as usize, y as usize, z as usize);
                edge_set(&mut edges[edge_index(0, y, z)], x == 1, v[0]);
                edge_set(&mut edges[edge_index(1, z, x)], y == 1, v[1]);
                edge_set(&mut edges[edge_index(2, x, y)], z == 1, v[2]);
            }
        }
    }
}

fn edge_set(edge: &mut u8, high: bool, val: u8) {
    *edge = if high {
        (*edge & 0xF) | (val << 4)
    } else {
        (*edge & 0xF0) | val
    };
}
