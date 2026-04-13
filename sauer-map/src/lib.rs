use ogz_rs::data::{Material, OctreeNode, OgzMap};

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
