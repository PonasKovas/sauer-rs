use crate::edit::{apply_region, fill, set_material, set_texture};
use crate::node::{Cube, Node};
use crate::selection::Selection;
use ogz_rs::OgzMap;
use ogz_rs::data::Material;
use ogz_rs::data::{Entity, EntityType};
use std::collections::HashMap;

/// Represents the spatial map structure, tracking geometric cubes, textures, and materials.
#[derive(Debug, Clone)]
pub struct World {
    pub size: i32,
    pub root: Node,
    pub entities: Vec<Entity>,
}

impl World {
    /// Constructs the spatial tree from raw OGZ data.
    pub fn from_ogz(map: &OgzMap) -> Self {
        World {
            size: map.world_size,
            root: crate::octree::build_node(&map.octree),
            entities: map.entities.clone(),
        }
    }

    /// Converts the tracked `World` back into an `OctreeNode` tree for serialization.
    pub fn to_ogz_octree(&self) -> ogz_rs::data::OctreeNode {
        crate::octree::node_to_ogz(&self.root)
    }

    /// Updates or inserts a new map entity.
    pub fn edit_ent(&mut self, id: usize, pos: [f32; 3], ent_type: EntityType, attrs: [i16; 5]) {
        if id < self.entities.len() {
            let ent = &mut self.entities[id];
            ent.position = pos;
            ent.ent_type = ent_type;
            ent.attrs = attrs;
        } else if id == self.entities.len() {
            self.entities.push(Entity {
                position: pos,
                attrs,
                ent_type,
                extra_data: Vec::new(),
            });
        }
    }

    /// Queries the octree at a specific 3D coordinate.
    /// Returns the Cube, its spatial size, and its origin (bottom-left-front corner).
    pub fn get_cube(&self, x: f32, y: f32, z: f32) -> Option<(&Cube, i32, [i32; 3])> {
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

            let cx = if ix >= ox + current_size { 1 } else { 0 };
            let cy = if iy >= oy + current_size { 1 } else { 0 };
            let cz = if iz >= oz + current_size { 1 } else { 0 };

            let child_idx = cx | (cy << 1) | (cz << 2);
            current_node = &children[child_idx as usize];

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

    /// Walk the octree within a defined selection block and apply a custom edit closure.
    /// Safely handles automatic subdivision if cubes are coarser than the selection grid.
    pub fn edit_sel<F>(&mut self, sel: &Selection, mut f: F)
    where
        F: FnMut(&mut Node, [i32; 3]),
    {
        if sel.grid <= 0 || sel.s.iter().any(|&n| n <= 0) {
            return;
        }
        let (min, max) = sel.bounds();
        apply_region(
            &mut self.root,
            self.size,
            [0, 0, 0],
            &min,
            &max,
            sel.grid,
            &mut f,
        );
    }

    /// Applies a material completely replacing the volumes within the selection.
    pub fn set_material(&mut self, sel: &Selection, mat: Material) {
        self.edit_sel(sel, |node, _| set_material(node, mat));
    }

    /// Applies a texture to the selected region. If `all_faces` is false, the
    /// texture only applies to the side matching the selection's orientation.
    pub fn set_texture(&mut self, sel: &Selection, tex: u16, all_faces: bool) {
        let orient = if all_faces {
            None
        } else {
            Some(sel.orient as usize)
        };
        self.edit_sel(sel, |node, _| set_texture(node, tex, orient));
    }

    /// Extrudes (solid) or erases (empty) the volumes bounded by the selection.
    pub fn fill(&mut self, sel: &Selection, is_solid: bool) {
        self.edit_sel(sel, |node, _| fill(node, is_solid));
    }

    /// Extrudes, Pushes, or Deforms faces or corners of the geometry based on the selection.
    pub fn edit_face(&mut self, sel: &Selection, dir: i32, mode: i32) {
        use crate::geom::{C, R, dimcoord, dimension, linked_push, optiface};

        let d = dimension(sel.orient as usize);
        let dc = dimcoord(sel.orient as usize);
        let seldir = if dc == 1 { -dir } else { dir };

        let mut actual_mode = mode;
        if actual_mode == 1
            && (sel.cx > 0 || sel.cy > 0 || (sel.cxs & 1) != 0 || (sel.cys & 1) != 0)
        {
            actual_mode = 0; // Partial face extrusion degrades back to standard pushing
        }

        let mut local_sel = sel.clone();

        if actual_mode == 1 {
            let h = local_sel.o[d] + (dc as i32) * local_sel.grid;
            if ((dir > 0) == (dc == 1) && h <= 0) || ((dir < 0) == (dc == 1) && h >= self.size) {
                return;
            }
            if dir < 0 {
                local_sel.o[d] += local_sel.grid * seldir;
            }
        }

        if dc == 1 {
            local_sel.o[d] += local_sel.s[d] * local_sel.grid - local_sel.grid;
        }
        local_sel.s[d] = 1;

        // If extruding a new block (mode 1, dir < 0), we must inherit textures from the
        // adjacent block we pulled from. We sample these before mutating the tree.
        let mut adj_textures = HashMap::new();
        if actual_mode == 1 && dir < 0 {
            let r = R[d];
            let c = C[d];
            for x in 0..local_sel.s[r] {
                for y in 0..local_sel.s[c] {
                    let mut pos = local_sel.o;
                    pos[r] += x * local_sel.grid;
                    pos[c] += y * local_sel.grid;
                    pos[d] -= seldir * local_sel.grid; // Step back one coordinate to sample

                    if let Some((cube, _, _)) = self.get_cube(
                        pos[0] as f32 + (local_sel.grid as f32 * 0.5),
                        pos[1] as f32 + (local_sel.grid as f32 * 0.5),
                        pos[2] as f32 + (local_sel.grid as f32 * 0.5),
                    ) {
                        adj_textures.insert([pos[r], pos[c]], cube.textures);
                    }
                }
            }
        }

        self.edit_sel(&local_sel, |node, pos| {
            // Safely inherit existing textures/materials if we need to collapse a detailed branch.
            let (inherited_tex, inherited_mat) = match node {
                Node::Leaf(c) => (c.textures, c.material),
                Node::Branch(children) => {
                    let mut curr = &children[0];
                    while let Node::Branch(c) = curr {
                        curr = &c[0];
                    }
                    if let Node::Leaf(c) = curr {
                        (c.textures, c.material)
                    } else {
                        ([0; 6], Default::default())
                    }
                }
            };

            if let Node::Branch(_) = node {
                // When collapsing a branch into a face edit, Sauerbraten explicitly assumes
                // it as a solid block before pushing the geometry into it.
                *node = Node::Leaf(Cube {
                    edges: [0x80; 12],
                    textures: inherited_tex,
                    material: inherited_mat,
                });
            }

            if let Node::Leaf(c) = node {
                if actual_mode == 1 {
                    // Fill / Delete Mode
                    if dir < 0 {
                        c.edges = [0x80; 12];
                        let rx = pos[R[d]];
                        let ry = pos[C[d]];
                        // Inherit the exact texture of the block we pulled from.
                        if let Some(tex) = adj_textures.get(&[rx, ry]) {
                            c.textures = *tex;
                        }
                    } else {
                        c.edges = [0x00; 12];
                    }
                } else {
                    // Standard Push / Corner
                    let bak = [
                        c.edges[d * 4],
                        c.edges[d * 4 + 1],
                        c.edges[d * 4 + 2],
                        c.edges[d * 4 + 3],
                    ];

                    if actual_mode == 2 {
                        // Corner Push
                        linked_push(
                            c,
                            d,
                            (sel.corner & 1) as usize,
                            (sel.corner >> 1) as usize,
                            dc,
                            seldir,
                        );
                    } else {
                        // Normal Face Push
                        for mx in 0..2 {
                            for my in 0..2 {
                                let rx = (pos[R[d]] - local_sel.o[R[d]]) / local_sel.grid;
                                let ry = (pos[C[d]] - local_sel.o[C[d]]) / local_sel.grid;

                                if rx == 0 && mx == 0 && sel.cx > 0 {
                                    continue;
                                }
                                if ry == 0 && my == 0 && sel.cy > 0 {
                                    continue;
                                }
                                if rx == local_sel.s[R[d]] - 1
                                    && mx == 1
                                    && ((sel.cx + sel.cxs) & 1) != 0
                                {
                                    continue;
                                }
                                if ry == local_sel.s[C[d]] - 1
                                    && my == 1
                                    && ((sel.cy + sel.cys) & 1) != 0
                                {
                                    continue;
                                }

                                if c.edges[d * 4 + mx + my * 2] != bak[mx + my * 2] {
                                    continue;
                                }

                                linked_push(c, d, mx, my, dc, seldir);
                            }
                        }
                    }
                    optiface(c, d);
                }
            }
        });
    }
}
