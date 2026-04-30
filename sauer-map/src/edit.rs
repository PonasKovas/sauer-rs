use crate::{Cube, Node};
use ogz_rs::data::Material;

// Change the closure signature to include the `[i32; 3]` pos:
pub fn apply_region<F>(
    node: &mut Node,
    node_size: i32,
    origin: [i32; 3],
    sel_min: &[i32; 3],
    sel_max: &[i32; 3],
    grid: i32,
    f: &mut F,
) where
    F: FnMut(&mut Node, [i32; 3]), // Added [i32; 3] here
{
    // AABB Bounds Check
    if origin[0] >= sel_max[0]
        || origin[0] + node_size <= sel_min[0]
        || origin[1] >= sel_max[1]
        || origin[1] + node_size <= sel_min[1]
        || origin[2] >= sel_max[2]
        || origin[2] + node_size <= sel_min[2]
    {
        return;
    }

    if node_size > grid {
        if let Node::Leaf(cube) = node {
            let children = crate::geom::subdivide(cube);
            *node = Node::Branch(Box::new(children.map(Node::Leaf)));
        }

        if let Node::Branch(children) = node {
            let half = node_size / 2;
            for i in 0..8 {
                let cx = (i & 1) as i32;
                let cy = ((i >> 1) & 1) as i32;
                let cz = ((i >> 2) & 1) as i32;
                let child_origin = [
                    origin[0] + cx * half,
                    origin[1] + cy * half,
                    origin[2] + cz * half,
                ];
                apply_region(
                    &mut children[i as usize],
                    half,
                    child_origin,
                    sel_min,
                    sel_max,
                    grid,
                    f,
                );
            }
        }
    } else {
        f(node, origin);
    }
}

pub fn set_material(node: &mut Node, mat: Material) {
    match node {
        Node::Leaf(c) => c.material = mat,
        Node::Branch(children) => {
            for child in children.iter_mut() {
                set_material(child, mat);
            }
        }
    }
}

pub fn set_texture(node: &mut Node, tex: u16, orient: Option<usize>) {
    match node {
        Node::Leaf(c) => {
            if let Some(o) = orient {
                c.textures[o] = tex;
            } else {
                c.textures = [tex; 6];
            }
        }
        Node::Branch(children) => {
            for child in children.iter_mut() {
                set_texture(child, tex, orient);
            }
        }
    }
}

pub fn fill(node: &mut Node, is_solid: bool) {
    let edges = if is_solid { [0x80; 12] } else { [0x00; 12] };

    // In a map editor, filled sections inherit properties from their parent hierarchy.
    let (textures, material) = match node {
        Node::Leaf(c) => (c.textures, c.material),
        Node::Branch(children) => {
            let mut curr = &children[0];
            while let Node::Branch(c) = curr {
                curr = &c[0];
            }
            if let Node::Leaf(c) = curr {
                (c.textures, c.material)
            } else {
                ([0; 6], Material::default())
            }
        }
    };

    *node = Node::Leaf(Cube {
        edges,
        textures,
        material,
    });
}
