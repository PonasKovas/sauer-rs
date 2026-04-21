use crate::node::{Cube, Node};
use ogz_rs::data::{CubeInfo, ModernSurfaceData, OctreeNode, Surfaces};

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
            let converted: Vec<OctreeNode> = children.iter().map(node_to_ogz).collect();
            OctreeNode::Children(Box::new(converted.try_into().unwrap()))
        }
        Node::Leaf(cube) => {
            let surfaces = Surfaces::Modern(ModernSurfaceData {
                material: if cube.material != Default::default() {
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
    }
}
