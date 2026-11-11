use crate::{Cube, Node};
use ogz_rs::data::{CubeInfo, ModernSurfaceData, ModernSurfaces, OctreeNode, Surfaces};

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

pub fn diff_bounds(
    a: &Node,
    b: &Node,
    pos: [i32; 3],
    size: i32,
    out_min: &mut [i32; 3],
    out_max: &mut [i32; 3],
) {
    if a == b {
        return;
    }
    let half = size / 2;
    match (a, b) {
        (Node::Branch(ac), Node::Branch(bc)) => {
            for i in 0..8 {
                let cx = (i & 1) as i32;
                let cy = ((i >> 1) & 1) as i32;
                let cz = ((i >> 2) & 1) as i32;
                let child_pos = [pos[0] + cx * half, pos[1] + cy * half, pos[2] + cz * half];
                diff_bounds(&ac[i], &bc[i], child_pos, half, out_min, out_max);
            }
        }
        (Node::Branch(ac), Node::Leaf(bl)) => {
            let b_children = crate::geom::subdivide(bl);
            for i in 0..8 {
                let cx = (i & 1) as i32;
                let cy = ((i >> 1) & 1) as i32;
                let cz = ((i >> 2) & 1) as i32;
                let child_pos = [pos[0] + cx * half, pos[1] + cy * half, pos[2] + cz * half];
                diff_bounds(
                    &ac[i],
                    &Node::Leaf(b_children[i]),
                    child_pos,
                    half,
                    out_min,
                    out_max,
                );
            }
        }
        (Node::Leaf(al), Node::Branch(bc)) => {
            let a_children = crate::geom::subdivide(al);
            for i in 0..8 {
                let cx = (i & 1) as i32;
                let cy = ((i >> 1) & 1) as i32;
                let cz = ((i >> 2) & 1) as i32;
                let child_pos = [pos[0] + cx * half, pos[1] + cy * half, pos[2] + cz * half];
                diff_bounds(
                    &Node::Leaf(a_children[i]),
                    &bc[i],
                    child_pos,
                    half,
                    out_min,
                    out_max,
                );
            }
        }
        (Node::Leaf(_), Node::Leaf(_)) => {
            for i in 0..3 {
                if pos[i] < out_min[i] {
                    out_min[i] = pos[i];
                }
                if pos[i] + size > out_max[i] {
                    out_max[i] = pos[i] + size;
                }
            }
        }
    }
}

pub fn collect_diff_cells(
    a: &Node,
    b: &Node,
    pos: [i32; 3],
    size: i32,
    out: &mut Vec<([i32; 3], i32, Node)>,
) {
    if a == b {
        return;
    }
    let half = size / 2;
    match (a, b) {
        (Node::Branch(ac), Node::Branch(bc)) => {
            for i in 0..8 {
                let cx = (i & 1) as i32;
                let cy = ((i >> 1) & 1) as i32;
                let cz = ((i >> 2) & 1) as i32;
                let child_pos = [pos[0] + cx * half, pos[1] + cy * half, pos[2] + cz * half];
                collect_diff_cells(&ac[i], &bc[i], child_pos, half, out);
            }
        }
        (Node::Branch(ac), Node::Leaf(bl)) => {
            let b_children = crate::geom::subdivide(bl);
            for i in 0..8 {
                let cx = (i & 1) as i32;
                let cy = ((i >> 1) & 1) as i32;
                let cz = ((i >> 2) & 1) as i32;
                let child_pos = [pos[0] + cx * half, pos[1] + cy * half, pos[2] + cz * half];
                collect_diff_cells(&ac[i], &Node::Leaf(b_children[i]), child_pos, half, out);
            }
        }
        (Node::Leaf(al), Node::Branch(bc)) => {
            let a_children = crate::geom::subdivide(al);
            for i in 0..8 {
                let cx = (i & 1) as i32;
                let cy = ((i >> 1) & 1) as i32;
                let cz = ((i >> 2) & 1) as i32;
                let child_pos = [pos[0] + cx * half, pos[1] + cy * half, pos[2] + cz * half];
                collect_diff_cells(&Node::Leaf(a_children[i]), &bc[i], child_pos, half, out);
            }
        }
        (Node::Leaf(_), Node::Leaf(_)) => {
            out.push((pos, size, b.clone()));
        }
    }
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
