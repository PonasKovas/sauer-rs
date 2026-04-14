use glam::Vec3;
use ogz_rs::data::{Clip, Material, OctreeNode, OgzMap};

/// The high-level, physics-ready representation of the map.
#[derive(Debug, Clone)]
pub struct World {
    pub size: i32,
    pub root: Node,
}

#[derive(Debug, Clone)]
pub enum Node {
    Branch(Box<[Node; 8]>),
    Leaf(Cube),
}

/// A spatial leaf node stripped of visual noise.
#[derive(Debug, Clone)]
pub struct Cube {
    /// 12 edges defining the solid geometry inside this grid cell.
    /// Each byte is packed: (high_coord << 4) | low_coord.
    /// 0x80 means low=0, high=8 (fully solid along this edge).
    /// 0x00 means low=0, high=0 (fully empty along this edge).
    pub edges: [u8; 12],
    pub textures: [u16; 6],
    pub material: Material,
}

pub struct BoundingBox {
    pub center: Vec3,
    pub rx: f32,
    pub ry: f32,
    pub rz: f32,
}

impl BoundingBox {
    /// Creates a physics bounding box for a player/entity.
    /// `z_base` is the feet, `height` is total height.
    pub fn new(x: f32, y: f32, z_base: f32, radius_xy: f32, height: f32) -> Self {
        Self {
            center: Vec3::new(x, y, z_base + (height / 2.0)),
            rx: radius_xy,
            ry: radius_xy,
            rz: height / 2.0,
        }
    }

    /// Fast AABB vs AABB broadphase check
    pub fn intersects_cube_bounds(&self, c_min: Vec3, c_max: Vec3) -> bool {
        self.center.x - self.rx <= c_max.x
            && self.center.x + self.rx >= c_min.x
            && self.center.y - self.ry <= c_max.y
            && self.center.y + self.ry >= c_min.y
            && self.center.z - self.rz <= c_max.z
            && self.center.z + self.rz >= c_min.z
    }
}

pub struct Plane {
    pub normal: Vec3,
    pub offset: f32, // Distance from origin
}

impl Plane {
    /// Creates a plane from 3 counter-clockwise vertices.
    pub fn from_pts(p0: Vec3, p1: Vec3, p2: Vec3) -> Option<Self> {
        let n = (p1 - p0).cross(p2 - p0).normalize_or_zero();

        // If the vertices are collinear or identical, the cross product length is 0.
        if n.length_squared() < 1e-6 {
            return None;
        }

        // Sauerbraten math: dist = dot(n, v) + offset => offset = -dot(n, v)
        let offset = -n.dot(p0);
        Some(Self { normal: n, offset })
    }
}

// --- 2. Collision Result ---

pub enum CollisionResult {
    /// No collision. Space is empty.
    Free,
    /// Hit a wall/slope. Returns the normal to slide against and the penetration depth.
    HitSurface { normal: Vec3, penetration: f32 },
    /// Deeply submerged in solid geometry. Sliding is impossible.
    TrappedInside,
}

// Sauerbraten vertex indexing for the 6 faces.
// 0:Left(-X), 1:Right(+X), 2:Back(-Y), 3:Front(+Y), 4:Bottom(-Z), 5:Top(+Z)
const FV: [[usize; 4]; 6] = [
    [2, 1, 6, 5],
    [3, 4, 7, 0],
    [4, 5, 6, 7],
    [1, 2, 3, 0],
    [6, 1, 0, 7],
    [5, 4, 3, 2],
];

impl Cube {
    /// Decodes the 12 compressed edges into 8 absolute 3D coordinates.
    pub fn get_vertices(&self, x: f32, y: f32, z: f32, size: f32) -> [Vec3; 8] {
        let mut verts = [Vec3::ZERO; 8];
        let sf = size / 8.0;

        let edge_val = |byte_idx: usize, coord: usize| -> f32 {
            let val = self.edges[byte_idx];
            let length = if coord == 1 { val >> 4 } else { val & 0x0F };
            length as f32 * sf
        };

        // Sauerbraten's specific vertex mapping expected by the `FV` face array
        let cc = [
            [0, 1, 0], // 0
            [1, 1, 0], // 1
            [1, 0, 0], // 2
            [0, 0, 0], // 3
            [0, 0, 1], // 4
            [1, 0, 1], // 5
            [1, 1, 1], // 6
            [0, 1, 1], // 7
        ];

        for i in 0..8 {
            let cx = cc[i][0];
            let cy = cc[i][1];
            let cz = cc[i][2];

            let ex = edge_val(0 + (cy * 2) + cz, cx);
            let ey = edge_val(4 + (cz * 2) + cx, cy);
            let ez = edge_val(8 + (cx * 2) + cy, cz);

            verts[i] = Vec3::new(x + ex, y + ey, z + ez);
        }
        verts
    }

    /// Converts the cube's vertices into an array of collision planes.
    pub fn get_clipplanes(&self, x: f32, y: f32, z: f32, size: f32) -> Vec<Plane> {
        let mut planes = Vec::with_capacity(12);
        let v = self.get_vertices(x, y, z, size);

        for f in 0..6 {
            let p0 = v[FV[f][0]];
            let p1 = v[FV[f][1]];
            let p2 = v[FV[f][2]];
            let p3 = v[FV[f][3]];

            // Sauerbraten Face Convexity check
            let n = (p1 - p0).cross(p2 - p0);
            let convex = (p0 - p3).dot(n);

            // If convex < 0 (concave), we shift the vertex order to generate proper collision planes.
            let order = if convex < 0.0 { 1 } else { 0 };

            let v0 = v[FV[f][(0 + order) % 4]];
            let v1 = v[FV[f][(1 + order) % 4]];
            let v2 = v[FV[f][(2 + order) % 4]];
            let v3 = v[FV[f][(3 + order) % 4]];

            if let Some(p) = Plane::from_pts(v0, v1, v2) {
                planes.push(p);
            }

            // If the face is flat or convex, we push the second triangle plane.
            // If concave, the engine drops it to avoid getting stuck in weird geometry folds.
            if convex >= 0.0 {
                if let Some(p) = Plane::from_pts(v0, v2, v3) {
                    planes.push(p);
                }
            }
        }
        planes
    }
}

struct CollisionHit {
    normal: Vec3,
    penetration: f32,
}

impl World {
    /// Constructs the spatial tree from the raw OGZ data.
    pub fn from_ogz(map: &OgzMap) -> Self {
        World {
            size: map.world_size,
            root: Self::build_node(&map.octree),
        }
    }

    fn build_node(ogz_node: &OctreeNode) -> Node {
        match ogz_node {
            OctreeNode::Children(children) => {
                let mut branch = Vec::with_capacity(8);
                for child in children.as_ref() {
                    branch.push(Self::build_node(child));
                }
                Node::Branch(Box::new(branch.try_into().unwrap()))
            }
            OctreeNode::LodCube { children, .. } => {
                // For physics/server logic, we ignore the LOD proxy geometry
                // and drill straight down into the actual children.
                let mut branch = Vec::with_capacity(8);
                for child in children.as_ref() {
                    branch.push(Self::build_node(child));
                }
                Node::Branch(Box::new(branch.try_into().unwrap()))
            }
            OctreeNode::Empty(info) => Node::Leaf(Cube {
                edges: [0x00; 12], // F_EMPTY translates to all 0s
                textures: info.textures,
                material: info.surfaces.material_id(),
            }),
            OctreeNode::Solid(info) => Node::Leaf(Cube {
                edges: [0x80; 12], // F_SOLID is 0x80808080 (low=0, high=8)
                textures: info.textures,
                material: info.surfaces.material_id(),
            }),
            OctreeNode::Normal { info, edges } => Node::Leaf(Cube {
                edges: *edges,
                textures: info.textures,
                material: info.surfaces.material_id(),
            }),
        }
    }

    /// Queries the octree at a specific 3D coordinate.
    /// Returns the Cube, its spatial size, and its origin (bottom-left-front corner).
    pub fn get_cube(&self, x: f32, y: f32, z: f32) -> Option<(&Cube, i32, [i32; 3])> {
        // Bounds check
        if x < 0.0
            || y < 0.0
            || z < 0.0
            || x >= self.size as f32
            || y >= self.size as f32
            || z >= self.size as f32
        {
            return None;
        }

        let mut current_node = &self.root;
        let mut current_size = self.size;
        let mut ox = 0;
        let mut oy = 0;
        let mut oz = 0;

        let ix = x as i32;
        let iy = y as i32;
        let iz = z as i32;

        while let Node::Branch(children) = current_node {
            current_size /= 2;

            // Sauerbraten child index bitmask: X is bit 0, Y is bit 1, Z is bit 2.
            let cx = if ix >= ox + current_size { 1 } else { 0 };
            let cy = if iy >= oy + current_size { 1 } else { 0 };
            let cz = if iz >= oz + current_size { 1 } else { 0 };

            let child_idx = cx | (cy << 1) | (cz << 2);
            current_node = &children[child_idx];

            if cx == 1 {
                ox += current_size;
            }
            if cy == 1 {
                oy += current_size;
            }
            if cz == 1 {
                oz += current_size;
            }
        }

        if let Node::Leaf(cube) = current_node {
            Some((cube, current_size, [ox, oy, oz]))
        } else {
            None
        }
    }

    /// Performs a full physics check of a BoundingBox against the world geometry.
    pub fn collide(&self, bbox: &BoundingBox) -> CollisionResult {
        let mut hits = Vec::new();
        self.collide_node(&self.root, 0.0, 0.0, 0.0, self.size as f32, bbox, &mut hits);

        if hits.is_empty() {
            return CollisionResult::Free;
        }

        // Find the plane we penetrated the most. This is the dominant surface pushing us out.
        let mut deepest_pen = -1.0;
        let mut best_norm = Vec3::ZERO;

        for hit in hits {
            if hit.penetration > deepest_pen {
                deepest_pen = hit.penetration;
                best_norm = hit.normal;
            }
        }

        // If the penetration depth is larger than the player's radius, they are completely swallowed by a block.
        let r_eff =
            bbox.rx * best_norm.x.abs() + bbox.ry * best_norm.y.abs() + bbox.rz * best_norm.z.abs();
        if deepest_pen >= r_eff * 1.5 {
            CollisionResult::TrappedInside
        } else {
            CollisionResult::HitSurface {
                normal: best_norm,
                penetration: deepest_pen,
            }
        }
    }

    fn collide_node(
        &self,
        node: &Node,
        nx: f32,
        ny: f32,
        nz: f32,
        size: f32,
        bbox: &BoundingBox,
        hits: &mut Vec<CollisionHit>,
    ) {
        // 1. Broad Phase: Fast AABB rejection
        let c_min = Vec3::new(nx, ny, nz);
        let c_max = Vec3::new(nx + size, ny + size, nz + size);

        if !bbox.intersects_cube_bounds(c_min, c_max) {
            return;
        }

        match node {
            Node::Branch(children) => {
                let half = size / 2.0;
                for i in 0..8 {
                    let cx = nx + ((i & 1) as f32 * half);
                    let cy = ny + (((i >> 1) & 1) as f32 * half);
                    let cz = nz + (((i >> 2) & 1) as f32 * half);
                    self.collide_node(&children[i as usize], cx, cy, cz, half, bbox, hits);
                }
            }
            Node::Leaf(cube) => {
                // 1. Check material physics overrides
                let mut treat_as_solid = false;

                match cube.material.clip {
                    Clip::NoClip => return,   // Players pass right through
                    Clip::GameClip => return, // In Sauerbraten, GameClip usually only blocks AI/Monsters
                    Clip::Clip => {
                        // MAT_CLIP forces the entire grid cell to be solid, even if the geometry is empty.
                        treat_as_solid = true;
                    }
                    Clip::None => {}
                }

                let is_empty = cube.edges.iter().all(|&e| e == 0x00);

                if is_empty && !treat_as_solid {
                    return; // Normal empty space
                }

                // 2. Narrow Phase: Separating Axis Theorem (SAT) vs Planes
                let planes = if treat_as_solid {
                    // If it's an invisible wall, ignore its real edges and generate planes for a perfect cube.
                    let solid_cube = Cube {
                        edges: [0x80; 12],
                        textures: cube.textures,
                        material: cube.material.clone(),
                    };
                    solid_cube.get_clipplanes(nx, ny, nz, size)
                } else {
                    // Otherwise, generate planes based on the actual deformed geometry (slopes, etc.)
                    cube.get_clipplanes(nx, ny, nz, size)
                };

                let mut min_pen = f32::MAX;
                let mut push_normal = Vec3::ZERO;

                for plane in &planes {
                    // Distance from AABB center to the plane
                    let dist = plane.normal.dot(bbox.center) + plane.offset;

                    // Projected radius of the AABB onto the plane's normal
                    let r_eff = bbox.rx * plane.normal.x.abs()
                        + bbox.ry * plane.normal.y.abs()
                        + bbox.rz * plane.normal.z.abs();

                    let penetration = r_eff - dist;

                    // If penetration is negative, the AABB is completely in front of the plane.
                    // Because the cube is a convex hull, if we are outside ANY plane,
                    // we are not colliding with this cube. Early out!
                    if penetration <= 0.0 {
                        return;
                    }

                    // We want the plane with the *least* penetration.
                    if penetration < min_pen {
                        min_pen = penetration;
                        push_normal = plane.normal;
                    }
                }

                hits.push(CollisionHit {
                    normal: push_normal,
                    penetration: min_pen,
                });
            }
        }
    }
}

// Helper extension trait to pull the material out of OgzSurfaces smoothly
trait SurfaceMaterialExt {
    fn material_id(&self) -> Material;
}

impl SurfaceMaterialExt for ogz_rs::data::Surfaces {
    fn material_id(&self) -> Material {
        match self {
            ogz_rs::data::Surfaces::Modern(m) => m.material,
            ogz_rs::data::Surfaces::Legacy(l) => l.material,
            ogz_rs::data::Surfaces::None => None,
        }
        .unwrap_or_default()
    }
}
