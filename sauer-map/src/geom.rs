use crate::Cube;

// Sauerbraten's coordinate mapping permutations per axis
pub const R: [usize; 3] = [1, 2, 0];
pub const C: [usize; 3] = [2, 0, 1];
pub const D: [usize; 3] = [0, 1, 2];

pub fn dimension(orient: usize) -> usize {
    orient >> 1
}
pub fn dimcoord(orient: usize) -> usize {
    orient & 1
}

pub fn octaindex(d: usize, x: usize, y: usize, z: usize) -> usize {
    (z << D[d]) | (y << C[d]) | (x << R[d])
}

pub fn get_cubeedge(cube: &Cube, d: usize, x: usize, y: usize, coord: usize) -> u8 {
    let edge_idx = (d << 2) + (y << 1) + x;
    let edge = cube.edges[edge_idx];
    if coord == 1 { edge >> 4 } else { edge & 0x0F }
}

pub fn set_cubeedge(cube: &mut Cube, d: usize, x: usize, y: usize, coord: usize, val: u8) {
    let edge_idx = (d << 2) + (y << 1) + x;
    let edge = cube.edges[edge_idx];
    cube.edges[edge_idx] = if coord == 1 {
        (edge & 0x0F) | (val << 4)
    } else {
        (edge & 0xF0) | val
    };
}

pub fn get_cubevector(cube: &Cube, i: usize) -> [i32; 3] {
    let mut p = [0; 3];
    p[0] = get_cubeedge(cube, 0, (i >> R[0]) & 1, (i >> C[0]) & 1, (i >> D[0]) & 1) as i32;
    p[1] = get_cubeedge(cube, 1, (i >> R[1]) & 1, (i >> C[1]) & 1, (i >> D[1]) & 1) as i32;
    p[2] = get_cubeedge(cube, 2, (i >> R[2]) & 1, (i >> C[2]) & 1, (i >> D[2]) & 1) as i32;
    p
}

/// Robustly resolves interpolations of a subdivided Normal leaf grid point.
fn midedge(a: &[i32; 3], b: &[i32; 3], xd: usize, yd: usize, perfect: &mut bool) -> i32 {
    let ax = a[xd];
    let ay = a[yd];
    let bx = b[xd];
    let by = b[yd];

    if ay == by {
        return ay;
    }
    if ax == bx {
        *perfect = false;
        return ay;
    }

    let crossx = (ax < 8 && bx > 8) || (ax > 8 && bx < 8);
    let crossy = (ay < 8 && by > 8) || (ay > 8 && by < 8);

    if crossy && !crossx {
        midedge(a, b, yd, xd, perfect);
        return 8;
    }

    if ax <= 8 && bx <= 8 {
        return if ax > bx { ay } else { by };
    }
    if ax >= 8 && bx >= 8 {
        return if ax < bx { ay } else { by };
    }

    let risex = (by - ay) * (8 - ax) * 256;
    let s = risex / (bx - ax);
    let y = s / 256 + ay;

    if (s.abs() & 0xFF) != 0 || (crossy && y != 8) || !(0..=16).contains(&y) {
        *perfect = false;
    }

    if crossy { 8 } else { y.clamp(0, 16) }
}

fn crosscenter(a: &[i32; 3], b: &[i32; 3], xd: usize, yd: usize) -> bool {
    let ax = a[xd];
    let ay = a[yd];
    let bx = b[xd];
    let by = b[yd];
    (((ax <= 8 && bx <= 8) || (ax >= 8 && bx >= 8))
        && ((ay <= 8 && by <= 8) || (ay >= 8 && by >= 8)))
        || (ax + bx == 16 && ay + by == 16)
}

/// Validates subdivided geometry exactly like Sauerbraten's internal engine loader.
/// Deletes mathematically malformed cubes (crossed edges, unconnected corners)
/// to prevent floating artifacts from rendering via clipboard paste.
pub fn validatec(cube: &mut Cube) {
    for j in 0..3 {
        let e0 = cube.edges[j * 4];
        let e1 = cube.edges[j * 4 + 1];
        let e2 = cube.edges[j * 4 + 2];
        let e3 = cube.edges[j * 4 + 3];

        // If start == end for all 4 edges on this face, the cube is collapsed (zero thickness).
        if (e0 & 0x0F) == (e0 >> 4)
            && (e1 & 0x0F) == (e1 >> 4)
            && (e2 & 0x0F) == (e2 >> 4)
            && (e3 & 0x0F) == (e3 >> 4)
        {
            cube.edges = [0x00; 12];
            break;
        }

        // If any edge is crossed (end < start) or out of bounds (end > 8), the cube is invalid.
        let mut valid = true;
        for e in [e0, e1, e2, e3] {
            let start = e & 0x0F;
            let end = e >> 4;
            if end < start || end > 8 {
                valid = false;
                break;
            }
        }

        if !valid {
            cube.edges = [0x00; 12];
            break;
        }
    }
}

/// Splits any solid, empty or deformed (Normal) leaf grid correctly down to the scale beneath it.
pub fn subdivide(cube: &Cube) -> [Cube; 8] {
    if cube.is_empty() || cube.is_entirely_solid() {
        let child = Cube {
            edges: if cube.is_empty() {
                [0x00; 12]
            } else {
                [0x80; 12]
            },
            textures: cube.textures,
            material: cube.material,
        };
        return [child, child, child, child, child, child, child, child];
    }

    let mut ch = [Cube {
        edges: [0; 12],
        textures: cube.textures,
        material: cube.material,
    }; 8];
    let mut perfect = true;

    // Rescale cube to parent granularity limits
    let mut v = [[0; 3]; 8];
    for i in 0..8 {
        let mut corner = get_cubevector(cube, i);
        corner[0] *= 2;
        corner[1] *= 2;
        corner[2] *= 2;
        v[i] = corner;
    }

    for j in 0..6 {
        let d = dimension(j);
        let z = dimcoord(j);

        let v00 = &v[octaindex(d, 0, 0, z)];
        let v10 = &v[octaindex(d, 1, 0, z)];
        let v01 = &v[octaindex(d, 0, 1, z)];
        let v11 = &v[octaindex(d, 1, 1, z)];

        let mut e = [[0; 3]; 3];
        e[0][0] = v00[d];
        e[0][2] = v01[d];
        e[2][0] = v10[d];
        e[2][2] = v11[d];

        e[0][1] = midedge(v00, v01, C[d], d, &mut perfect);
        e[1][0] = midedge(v00, v10, R[d], d, &mut perfect);
        e[1][2] = midedge(v11, v01, R[d], d, &mut perfect);
        e[2][1] = midedge(v11, v10, C[d], d, &mut perfect);

        let mut p1 = perfect;
        let mut p2 = perfect;
        let c1 = midedge(v00, v11, R[d], d, &mut p1);
        let c2 = midedge(v01, v10, R[d], d, &mut p2);

        if if z == 1 { c1 > c2 } else { c1 < c2 } {
            e[1][1] = c1;
            perfect = p1 && (c1 == c2 || crosscenter(v00, v11, C[d], R[d]));
        } else {
            e[1][1] = c2;
            perfect = p2 && (c1 == c2 || crosscenter(v01, v10, C[d], R[d]));
        }

        for i in 0..8 {
            let rd = (i >> R[d]) & 1;
            let cd = (i >> C[d]) & 1;
            let dd = (i >> D[d]) & 1;

            // Generate offset localized properties per child relative to parent bounds
            set_cubeedge(
                &mut ch[i],
                d,
                0,
                0,
                z,
                (e[rd][cd] - dd as i32 * 8).clamp(0, 8) as u8,
            );
            set_cubeedge(
                &mut ch[i],
                d,
                1,
                0,
                z,
                (e[1 + rd][cd] - dd as i32 * 8).clamp(0, 8) as u8,
            );
            set_cubeedge(
                &mut ch[i],
                d,
                0,
                1,
                z,
                (e[rd][1 + cd] - dd as i32 * 8).clamp(0, 8) as u8,
            );
            set_cubeedge(
                &mut ch[i],
                d,
                1,
                1,
                z,
                (e[1 + rd][1 + cd] - dd as i32 * 8).clamp(0, 8) as u8,
            );
        }
    }

    // Mathematically sanitize children post-subdivision to prevent floating face artifacts
    for child in &mut ch {
        validatec(child);
    }

    ch
}

pub fn get_vertex(cube: &Cube, d: usize, x: usize, y: usize, z: usize) -> [u8; 3] {
    let mut v = [0, 0, 0];
    v[d] = z as u8;
    v[R[d]] = x as u8;
    v[C[d]] = y as u8;

    let mut p = [0; 3];
    for i in 0..3 {
        p[i] = get_cubeedge(
            cube,
            i,
            v[R[i]] as usize,
            v[C[i]] as usize,
            v[D[i]] as usize,
        );
    }
    p
}

pub fn push_edge(edges: &mut [u8; 12], d: usize, x: usize, y: usize, dir: i32, dc: usize) {
    let edge_idx = (d << 2) + (y << 1) + x;
    let edge = edges[edge_idx];

    let oe = if dc == 1 { edge >> 4 } else { edge & 0xF };
    let ne = ((oe as i32) + dir).clamp(0, 8) as u8;

    let mut new_edge = if dc == 1 {
        (edge & 0x0F) | (ne << 4)
    } else {
        (edge & 0xF0) | ne
    };

    let other_oe = if dc == 1 { edge & 0xF } else { edge >> 4 };
    if (dir < 0 && dc == 1 && other_oe > ne) || (dir > 0 && dc == 0 && other_oe < ne) {
        if dc == 0 {
            new_edge = (new_edge & 0x0F) | (ne << 4);
        } else {
            new_edge = (new_edge & 0xF0) | ne;
        }
    }
    edges[edge_idx] = new_edge;
}

pub fn linked_push(cube: &mut Cube, d: usize, x: usize, y: usize, dc: usize, dir: i32) {
    let target_v = get_vertex(cube, d, x, y, dc);
    for i in 0..2 {
        for j in 0..2 {
            let p = get_vertex(cube, d, i, j, dc);
            if target_v == p {
                push_edge(&mut cube.edges, d, i, j, dir, dc);
            }
        }
    }
}

pub fn optiface(cube: &mut Cube, d: usize) {
    let e0 = cube.edges[d * 4];
    let e1 = cube.edges[d * 4 + 1];
    let e2 = cube.edges[d * 4 + 2];
    let e3 = cube.edges[d * 4 + 3];

    // Check if the geometry face has been collapsed perfectly flat
    if (e0 >> 4) == (e0 & 0xF)
        && (e1 >> 4) == (e1 & 0xF)
        && (e2 >> 4) == (e2 & 0xF)
        && (e3 >> 4) == (e3 & 0xF)
    {
        cube.edges = [0x00; 12];
    }
}
