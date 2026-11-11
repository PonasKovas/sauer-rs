use crate::map::Map;
use crate::{Cube, Node};

/// Contains information about a successful raycast hit.
#[derive(Debug, Clone)]
pub struct RayHit<'a> {
    /// Distance from the origin to the hit point
    pub dist: f32,
    /// Reference to the hit cube
    pub cube: &'a Cube,
    /// The bottom-left-front origin coordinate of the hit cube
    pub pos: [i32; 3],
    /// The spatial scale of the hit cube
    pub size: i32,
    /// The orientation of the face that was hit (0..=5).
    /// Matches Sauerbraten's O_LEFT (0), O_RIGHT (1), O_BACK (2), O_FRONT (3), O_BOTTOM (4), O_TOP (5)
    pub orient: u8,
}

// The 6 faces of a cube, defining the 4 corner indices (X, Y, Z) in
// Counter-Clockwise order when looking AT the face from the outside.
const FACES: [[[usize; 3]; 4]; 6] = [
    [[0, 0, 0], [0, 0, 1], [0, 1, 1], [0, 1, 0]], // 0: -X (Left)
    [[1, 0, 0], [1, 1, 0], [1, 1, 1], [1, 0, 1]], // 1: +X (Right)
    [[0, 0, 0], [1, 0, 0], [1, 0, 1], [0, 0, 1]], // 2: -Y (Back)
    [[0, 1, 0], [0, 1, 1], [1, 1, 1], [1, 1, 0]], // 3: +Y (Front)
    [[0, 0, 0], [0, 1, 0], [1, 1, 0], [1, 0, 0]], // 4: -Z (Bottom)
    [[0, 0, 1], [1, 0, 1], [1, 1, 1], [0, 1, 1]], // 5: +Z (Top)
];

/// Casts a ray through the octree to find the exact geometry face intersected.
pub fn raycast<'a>(
    map: &'a Map,
    origin: [f32; 3],
    ray: [f32; 3],
    max_dist: f32,
) -> Option<RayHit<'a>> {
    raycast_with_filter(map, origin, ray, max_dist, |c| !c.is_empty())
}

/// Casts a ray through the octree with a custom cube filter.
pub fn raycast_with_filter<'a, F>(
    map: &'a Map,
    origin: [f32; 3],
    ray: [f32; 3],
    max_dist: f32,
    filter: F,
) -> Option<RayHit<'a>>
where
    F: Fn(&Cube) -> bool + Copy,
{
    let mag = (ray[0] * ray[0] + ray[1] * ray[1] + ray[2] * ray[2]).sqrt();
    if mag == 0.0 {
        return None;
    }

    let ray_norm = [ray[0] / mag, ray[1] / mag, ray[2] / mag];

    let inv_ray = [
        if ray_norm[0].abs() < 1e-8 {
            0.0
        } else {
            1.0 / ray_norm[0]
        },
        if ray_norm[1].abs() < 1e-8 {
            0.0
        } else {
            1.0 / ray_norm[1]
        },
        if ray_norm[2].abs() < 1e-8 {
            0.0
        } else {
            1.0 / ray_norm[2]
        },
    ];

    let min_bound = [0.0, 0.0, 0.0];
    let max_bound = [map.size as f32, map.size as f32, map.size as f32];

    if let Some((tmin, tmax, axis)) = ray_aabb(origin, ray_norm, inv_ray, min_bound, max_bound)
        && tmin <= max_dist
        && tmax >= 0.0
    {
        return traverse(
            &map.root,
            origin,
            ray_norm,
            inv_ray,
            [0, 0, 0],
            map.size,
            tmin,
            tmax.min(max_dist),
            axis,
            filter,
        );
    }
    None
}

fn traverse<'a, F>(
    node: &'a Node,
    origin: [f32; 3],
    ray: [f32; 3],
    inv_ray: [f32; 3],
    cube_pos: [i32; 3],
    cube_size: i32,
    t_min: f32,
    t_max: f32,
    entry_axis: usize,
    filter: F,
) -> Option<RayHit<'a>>
where
    F: Fn(&Cube) -> bool + Copy,
{
    match node {
        Node::Leaf(cube) => {
            if !filter(cube) {
                return None;
            }

            if cube.is_entirely_solid() || cube.is_empty() {
                // Perfect cubic bounding box hit
                let orient = if ray[entry_axis] > 0.0 {
                    (entry_axis * 2) as u8
                } else {
                    (entry_axis * 2 + 1) as u8
                };
                return Some(RayHit {
                    dist: t_min.max(0.0),
                    cube,
                    pos: cube_pos,
                    size: cube_size,
                    orient,
                });
            }

            // --- FULL MESH COLLISION FOR DEFORMED CUBES ---

            let mut best_t = f32::INFINITY;
            let mut best_orient = 0;

            for orient in 0..6 {
                let f = FACES[orient];

                let v0 = corner_pos(cube, cube_pos, cube_size, f[0][0], f[0][1], f[0][2]);
                let v1 = corner_pos(cube, cube_pos, cube_size, f[1][0], f[1][1], f[1][2]);
                let v2 = corner_pos(cube, cube_pos, cube_size, f[2][0], f[2][1], f[2][2]);
                let v3 = corner_pos(cube, cube_pos, cube_size, f[3][0], f[3][1], f[3][2]);

                // Sauerbraten Triangulation Rule: Split quad based on convexity
                // (Prevents the mesh from folding inside out)
                let convex = face_convexity(v0, v1, v2, v3);

                let (tri1, tri2) = if convex < 0.0 {
                    ((v1, v2, v3), (v1, v3, v0)) // Order 1
                } else {
                    ((v0, v1, v2), (v0, v2, v3)) // Order 0
                };

                if let Some(t) = ray_triangle(origin, ray, tri1.0, tri1.1, tri1.2)
                    && t < best_t
                {
                    best_t = t;
                    best_orient = orient as u8;
                }
                if let Some(t) = ray_triangle(origin, ray, tri2.0, tri2.1, tri2.2)
                    && t < best_t
                {
                    best_t = t;
                    best_orient = orient as u8;
                }
            }

            if best_t <= t_max {
                return Some(RayHit {
                    dist: best_t,
                    cube,
                    pos: cube_pos,
                    size: cube_size,
                    orient: best_orient,
                });
            }
            None
        }
        Node::Branch(children) => {
            let half = cube_size / 2;
            let mut hits = [(0.0f32, 0.0f32, 0usize, 0usize); 8];
            let mut num_hits = 0;

            for i in 0..8 {
                let cx = (i & 1) as i32;
                let cy = ((i >> 1) & 1) as i32;
                let cz = ((i >> 2) & 1) as i32;
                let child_pos = [
                    cube_pos[0] + cx * half,
                    cube_pos[1] + cy * half,
                    cube_pos[2] + cz * half,
                ];

                let min_bound = [
                    child_pos[0] as f32,
                    child_pos[1] as f32,
                    child_pos[2] as f32,
                ];
                let max_bound = [
                    (child_pos[0] + half) as f32,
                    (child_pos[1] + half) as f32,
                    (child_pos[2] + half) as f32,
                ];

                if let Some((ct_min, ct_max, c_axis)) =
                    ray_aabb(origin, ray, inv_ray, min_bound, max_bound)
                    && ct_max >= 0.0
                    && ct_min <= t_max
                {
                    let active_axis = if ct_min <= 0.0 { entry_axis } else { c_axis };
                    hits[num_hits] = (ct_min, ct_max, active_axis, i);
                    num_hits += 1;
                }
            }

            // Sort child intersections closest to furthest
            hits[..num_hits].sort_unstable_by(|a, b| {
                a.0.partial_cmp(&b.0).unwrap_or(std::cmp::Ordering::Equal)
            });

            // Traverse children iteratively. If a ray misses the mesh inside a hit AABB,
            // we MUST continue to the next child bounding box behind it!
            for &(ct_min, ct_max, c_axis, i) in &hits[..num_hits] {
                let cx = (i & 1) as i32;
                let cy = ((i >> 1) & 1) as i32;
                let cz = ((i >> 2) & 1) as i32;
                let child_pos = [
                    cube_pos[0] + cx * half,
                    cube_pos[1] + cy * half,
                    cube_pos[2] + cz * half,
                ];

                if let Some(hit) = traverse(
                    &children[i],
                    origin,
                    ray,
                    inv_ray,
                    child_pos,
                    half,
                    ct_min,
                    ct_max,
                    c_axis,
                    filter,
                ) {
                    return Some(hit);
                }
            }

            None
        }
    }
}

/// Translates the 0..8 local deformation coordinates of a corner into exact 3D world space.
#[inline(always)]
fn corner_pos(cube: &Cube, pos: [i32; 3], size: i32, ix: usize, iy: usize, iz: usize) -> [f32; 3] {
    // We pass d=0, causing geom::get_vertex to map X,Y,Z directly into its permutation algorithm
    let p_local = crate::geom::get_vertex(cube, 0, iy, iz, ix);
    [
        pos[0] as f32 + (p_local[0] as f32 * size as f32 / 8.0),
        pos[1] as f32 + (p_local[1] as f32 * size as f32 / 8.0),
        pos[2] as f32 + (p_local[2] as f32 * size as f32 / 8.0),
    ]
}

/// Sauerbraten's internal `faceconvexity` math. Determines the correct triangulation split.
#[inline]
fn face_convexity(v0: [f32; 3], v1: [f32; 3], v2: [f32; 3], v3: [f32; 3]) -> f32 {
    let e1 = [v1[0] - v0[0], v1[1] - v0[1], v1[2] - v0[2]];
    let e2 = [v2[0] - v0[0], v2[1] - v0[1], v2[2] - v0[2]];
    let n = cross(e1, e2);
    let e3 = [v3[0] - v0[0], v3[1] - v0[1], v3[2] - v0[2]];
    dot(n, e3)
}

/// Fast Möller-Trumbore Ray-Triangle intersection
#[inline]
fn ray_triangle(
    orig: [f32; 3],
    dir: [f32; 3],
    v0: [f32; 3],
    v1: [f32; 3],
    v2: [f32; 3],
) -> Option<f32> {
    let e1 = [v1[0] - v0[0], v1[1] - v0[1], v1[2] - v0[2]];
    let e2 = [v2[0] - v0[0], v2[1] - v0[1], v2[2] - v0[2]];
    let h = cross(dir, e2);
    let a = dot(e1, h);

    if a > -1e-6 && a < 1e-6 {
        return None; // Ray is parallel to triangle, or triangle is degenerate (flat)
    }

    let f = 1.0 / a;
    let s = [orig[0] - v0[0], orig[1] - v0[1], orig[2] - v0[2]];
    let u = f * dot(s, h);

    if u < 0.0 || u > 1.0 {
        return None;
    }

    let q = cross(s, e1);
    let v = f * dot(dir, q);

    if v < 0.0 || u + v > 1.0 {
        return None;
    }

    let t = f * dot(e2, q);
    if t > 1e-5 { Some(t) } else { None }
}

/// Fast AABB intersect algorithm calculating intersection bounds and tracking the entry axis
fn ray_aabb(
    origin: [f32; 3],
    ray: [f32; 3],
    inv_ray: [f32; 3],
    min: [f32; 3],
    max: [f32; 3],
) -> Option<(f32, f32, usize)> {
    let mut tmin = f32::NEG_INFINITY;
    let mut tmax = f32::INFINITY;
    let mut hit_axis = 0;

    for i in 0..3 {
        if ray[i].abs() < 1e-8 {
            if origin[i] < min[i] || origin[i] > max[i] {
                return None;
            }
        } else {
            let mut t1 = (min[i] - origin[i]) * inv_ray[i];
            let mut t2 = (max[i] - origin[i]) * inv_ray[i];

            if t1 > t2 {
                std::mem::swap(&mut t1, &mut t2);
            }
            if t1 > tmin {
                tmin = t1;
                hit_axis = i;
            }
            tmax = tmax.min(t2);

            if tmax < tmin {
                return None;
            }
        }
    }
    Some((tmin, tmax, hit_axis))
}

#[inline(always)]
fn cross(a: [f32; 3], b: [f32; 3]) -> [f32; 3] {
    [
        a[1] * b[2] - a[2] * b[1],
        a[2] * b[0] - a[0] * b[2],
        a[0] * b[1] - a[1] * b[0],
    ]
}
#[inline(always)]
fn dot(a: [f32; 3], b: [f32; 3]) -> f32 {
    a[0] * b[0] + a[1] * b[1] + a[2] * b[2]
}
