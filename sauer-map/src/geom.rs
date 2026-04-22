use crate::node::Cube;

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
/// Guaranteed to be mathematically symmetric regardless of vertex orientation order.
fn midedge(a: &[i32; 3], b: &[i32; 3], xd: usize, yd: usize, perfect: &mut bool) -> i32 {
    let mut ax = a[xd];
    let mut ay = a[yd];
    let mut bx = b[xd];
    let mut by = b[yd];

    // Enforce strict deterministic order to prevent chunk alignment gaps
    if ax > bx || (ax == bx && ay > by) {
        std::mem::swap(&mut ax, &mut bx);
        std::mem::swap(&mut ay, &mut by);
    }

    if ay == by {
        return ay;
    }
    if ax == bx {
        *perfect = false;
        return ay;
    }

    let crossx = ax < 8 && bx > 8; // Simplified since ax < bx is guaranteed
    let crossy = (ay < 8 && by > 8) || (ay > 8 && by < 8);

    if crossy && !crossx {
        let mut temp_a = [0; 3];
        let mut temp_b = [0; 3];
        temp_a[xd] = ax;
        temp_a[yd] = ay;
        temp_b[xd] = bx;
        temp_b[yd] = by;
        midedge(&temp_a, &temp_b, yd, xd, perfect);
        return 8;
    }

    if bx <= 8 {
        return by; // bx is closest to 8
    }
    if ax >= 8 {
        return ay; // ax is closest to 8
    }

    let risex = (by - ay) * (8 - ax) * 256;
    let s = risex / (bx - ax);
    let y = s / 256 + ay;

    if (s.abs() & 0xFF) != 0 || (crossy && y != 8) || y < 0 || y > 16 {
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
        return [
            child.clone(),
            child.clone(),
            child.clone(),
            child.clone(),
            child.clone(),
            child.clone(),
            child.clone(),
            child.clone(),
        ];
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
                (e[rd][cd] as i32 - dd as i32 * 8).clamp(0, 8) as u8,
            );
            set_cubeedge(
                &mut ch[i],
                d,
                1,
                0,
                z,
                (e[1 + rd][cd] as i32 - dd as i32 * 8).clamp(0, 8) as u8,
            );
            set_cubeedge(
                &mut ch[i],
                d,
                0,
                1,
                z,
                (e[rd][1 + cd] as i32 - dd as i32 * 8).clamp(0, 8) as u8,
            );
            set_cubeedge(
                &mut ch[i],
                d,
                1,
                1,
                z,
                (e[1 + rd][1 + cd] as i32 - dd as i32 * 8).clamp(0, 8) as u8,
            );
        }
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

    // Assert all 4 edges on the face are strictly identical and flat
    if e0 == e1 && e0 == e2 && e0 == e3 && (e0 >> 4) == (e0 & 0xF) {
        cube.edges = [0x00; 12];
    }
}
