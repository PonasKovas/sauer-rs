use ogz_rs::{
    OgzMap,
    data::{CubeInfo, Material, ModernSurfaceData, OctreeNode, Surfaces},
};

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
                material: info.surfaces.material().unwrap_or_default(),
            }),
            OctreeNode::Solid(info) => Node::Leaf(Cube {
                edges: [0x80; 12], // F_SOLID is 0x80808080 (low=0, high=8)
                textures: info.textures,
                material: info.surfaces.material().unwrap_or_default(),
            }),
            OctreeNode::Normal { info, edges } => Node::Leaf(Cube {
                edges: *edges,
                textures: info.textures,
                material: info.surfaces.material().unwrap_or_default(),
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

    /// Apply a closure to every leaf `Cube` whose bounding box overlaps the
    /// given axis-aligned selection.
    ///
    /// `o` is the selection origin (world-unit corner), `s` is the count of
    /// cubes in each dimension at `grid` granularity.  Nodes that are coarser
    /// than `grid` are subdivided on the fly so that edits land at the right
    /// resolution.
    pub fn edit_sel<F>(&mut self, o: [i32; 3], s: [i32; 3], grid: i32, mut f: F)
    where
        F: FnMut(&mut Cube),
    {
        if grid <= 0 || s.iter().any(|&n| n <= 0) {
            return;
        }
        let sel_max = [
            o[0] + s[0] * grid,
            o[1] + s[1] * grid,
            o[2] + s[2] * grid,
        ];
        apply_region(&mut self.root, self.size, [0, 0, 0], o, sel_max, grid, &mut f);
    }

    /// Convert the tracked `World` back into an `OctreeNode` tree.
    ///
    /// Lightmap surfaces are not preserved (they are stripped when the `World`
    /// is constructed via `from_ogz`).  The resulting tree encodes only the
    /// geometry, textures, and materials tracked by this type.
    pub fn to_ogz_octree(&self) -> OctreeNode {
        node_to_ogz(&self.root)
    }
}

// ── Octree helpers ────────────────────────────────────────────────────────────

fn node_to_ogz(node: &Node) -> OctreeNode {
    match node {
        Node::Branch(children) => {
            let converted: Vec<OctreeNode> = children.iter().map(node_to_ogz).collect();
            OctreeNode::Children(Box::new(converted.try_into().unwrap()))
        }
        Node::Leaf(cube) => {
            let surfaces = Surfaces::Modern(ModernSurfaceData {
                material: if cube.material != Material::default() {
                    Some(cube.material)
                } else {
                    None
                },
                merged: None,
                surfaces: None,
            });
            let info = CubeInfo {
                textures: cube.textures,
                surfaces,
            };
            if cube.edges == [0x00u8; 12] {
                OctreeNode::Empty(info)
            } else if cube.edges == [0x80u8; 12] {
                OctreeNode::Solid(info)
            } else {
                OctreeNode::Normal {
                    info,
                    edges: cube.edges,
                }
            }
        }
    }
}

/// Recursively apply `f` to all leaves whose bounds overlap the closed
/// selection `[sel_min, sel_max)`.  Leaf nodes coarser than `grid` are
/// subdivided into eight equal children (each inheriting the parent's cube
/// data) before recursing.
fn apply_region(
    node: &mut Node,
    node_size: i32,
    [ox, oy, oz]: [i32; 3],
    sel_min: [i32; 3],
    sel_max: [i32; 3],
    grid: i32,
    f: &mut impl FnMut(&mut Cube),
) {
    // Reject if this node's AABB has no overlap with the selection.
    if ox >= sel_max[0] || ox + node_size <= sel_min[0] {
        return;
    }
    if oy >= sel_max[1] || oy + node_size <= sel_min[1] {
        return;
    }
    if oz >= sel_max[2] || oz + node_size <= sel_min[2] {
        return;
    }

    // If this is a leaf that is coarser than the target grid, subdivide it so
    // subsequent edits land at the correct resolution.
    if matches!(node, Node::Leaf(_)) && node_size > grid {
        let cube = match node {
            Node::Leaf(c) => c.clone(),
            _ => unreachable!(),
        };
        let children = std::array::from_fn(|_| Node::Leaf(cube.clone()));
        *node = Node::Branch(Box::new(children));
    }

    match node {
        Node::Leaf(cube) => f(cube),
        Node::Branch(children) => {
            let half = node_size / 2;
            for i in 0..8usize {
                let cx = (i & 1) as i32;
                let cy = ((i >> 1) & 1) as i32;
                let cz = ((i >> 2) & 1) as i32;
                let child_origin = [ox + cx * half, oy + cy * half, oz + cz * half];
                apply_region(
                    &mut children[i],
                    half,
                    child_origin,
                    sel_min,
                    sel_max,
                    grid,
                    f,
                );
            }
        }
    }
}
