use crate::{Cube, Node};
use ogz_rs::data::{CubeInfo, ModernSurfaceData, ModernSurfaces, OctreeNode, Surfaces};
use sauer_core::Aabb;

/// Size of the biggest shaped (neither empty nor fully solid) leaf touching
/// `[min, max)`, or 0. A clipboard grid below this would split that leaf,
/// and the engine's split can't keep most slopes (subdividecube is not
/// "perfect" for them), so the copy would come back with dents.
pub fn max_shaped_leaf(node: &Node, pos: [i32; 3], size: i32, min: [i32; 3], max: [i32; 3]) -> i32 {
    if (0..3).any(|k| pos[k] + size <= min[k] || pos[k] >= max[k]) {
        return 0;
    }
    match node {
        Node::Leaf(cube) if cube.is_empty() || cube.is_entirely_solid() => 0,
        Node::Leaf(_) => size,
        Node::Branch(children) => {
            let half = size / 2;
            (0..8)
                .map(|i| {
                    let child_pos = [
                        pos[0] + (i & 1) as i32 * half,
                        pos[1] + ((i >> 1) & 1) as i32 * half,
                        pos[2] + ((i >> 2) & 1) as i32 * half,
                    ];
                    max_shaped_leaf(&children[i], child_pos, half, min, max)
                })
                .max()
                .unwrap_or(0)
        }
    }
}

pub fn extract_node_at(
    node: &Node,
    node_pos: [i32; 3],
    node_size: i32,
    target_pos: [i32; 3],
    target_size: i32,
) -> Node {
    // `<=` and the size check stop a bad target (not a power of two, or
    // outside the world) from recursing forever.
    if node_size <= target_size || node_size <= 1 {
        return node.clone();
    }
    match node {
        Node::Leaf(cube) => {
            let children = crate::geom::subdivide(cube);
            let branch = Node::Branch(Box::new(children.map(Node::Leaf)));
            extract_node_at(&branch, node_pos, node_size, target_pos, target_size)
        }
        Node::Branch(children) => {
            let half = node_size / 2;
            let cx = usize::from(target_pos[0] >= node_pos[0] + half);
            let cy = usize::from(target_pos[1] >= node_pos[1] + half);
            let cz = usize::from(target_pos[2] >= node_pos[2] + half);
            let child_pos = [
                node_pos[0] + cx as i32 * half,
                node_pos[1] + cy as i32 * half,
                node_pos[2] + cz as i32 * half,
            ];
            extract_node_at(
                &children[cx | (cy << 1) | (cz << 2)],
                child_pos,
                half,
                target_pos,
                target_size,
            )
        }
    }
}

/// The cells where two trees differ: leaf-sized boxes, one per pair of
/// leaves that aren't equal (a leaf against a branch gets subdivided).
pub fn diff_cells(a: &Node, b: &Node, world_size: i32) -> Vec<Aabb> {
    fn children(node: &Node) -> std::borrow::Cow<'_, [Node; 8]> {
        match node {
            Node::Branch(c) => std::borrow::Cow::Borrowed(c),
            Node::Leaf(l) => std::borrow::Cow::Owned(crate::geom::subdivide(l).map(Node::Leaf)),
        }
    }
    fn walk(a: &Node, b: &Node, pos: [i32; 3], size: i32, out: &mut Vec<Aabb>) {
        if a == b {
            return;
        }
        if let (Node::Leaf(_), Node::Leaf(_)) = (a, b) {
            out.push(Aabb::cube(pos, size));
            return;
        }
        let (ac, bc) = (children(a), children(b));
        let half = size / 2;
        for i in 0..8 {
            let child = std::array::from_fn(|k| pos[k] + ((i >> k) & 1) as i32 * half);
            walk(&ac[i], &bc[i], child, half, out);
        }
    }
    let mut out = Vec::new();
    walk(a, b, [0; 3], world_size, &mut out);
    out
}

/// The smallest box around everything that differs between two trees.
pub fn diff_box(a: &Node, b: &Node, world_size: i32) -> Option<Aabb> {
    Aabb::enclosing(diff_cells(a, b, world_size))
}

pub fn build_node(ogz_node: &OctreeNode) -> Node {
    match ogz_node {
        OctreeNode::Children(children) => {
            let branch = std::array::from_fn(|i| build_node(&children[i]));
            Node::Branch(Box::new(branch))
        }
        OctreeNode::LodCube { children, .. } => {
            let branch = std::array::from_fn(|i| build_node(&children[i]));
            Node::Branch(Box::new(branch))
        }
        OctreeNode::Empty(info) => Node::Leaf(Cube {
            edges: [0x00; 12],
            textures: info.textures,
            material: info.surfaces.material().unwrap_or_default(),
        }),
        OctreeNode::Solid(info) => Node::Leaf(Cube {
            edges: [0x80; 12],
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

pub fn node_to_ogz(node: &Node) -> OctreeNode {
    match node {
        Node::Branch(children) => {
            let converted = std::array::from_fn(|i| node_to_ogz(&children[i]));
            OctreeNode::Children(Box::new(converted))
        }
        Node::Leaf(cube) => leaf_to_ogz(cube, None, None),
    }
}

pub(crate) fn leaf_to_ogz(
    cube: &Cube,
    merged: Option<u8>,
    surfaces: Option<ModernSurfaces>,
) -> OctreeNode {
    let surfaces = Surfaces::Modern(ModernSurfaceData {
        material: if cube.material != Default::default() {
            Some(cube.material)
        } else {
            None
        },
        merged,
        surfaces,
    });
    let info = CubeInfo {
        textures: cube.textures,
        surfaces,
    };
    if cube.edges == [0x00; 12] {
        OctreeNode::Empty(info)
    } else if cube.edges == [0x80; 12] {
        OctreeNode::Solid(info)
    } else {
        OctreeNode::Normal {
            info,
            edges: cube.edges,
        }
    }
}
