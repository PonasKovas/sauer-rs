use crate::edit::{apply_region, fill, set_material, set_texture};
use crate::{Cube, Node, Selection};
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

use ogz_rs::data::VSlot;
use std::collections::HashSet;

impl World {
    pub fn compact_vslots(&mut self, vslots: &mut Vec<Option<ogz_rs::data::VSlot>>) {
        let mut used = std::collections::HashSet::new();
        fn collect(node: &Node, used: &mut std::collections::HashSet<u16>) {
            match node {
                Node::Leaf(c) => {
                    for &t in &c.textures {
                        used.insert(t);
                    }
                }
                Node::Branch(ch) => {
                    for c in ch.as_ref() {
                        collect(c, used);
                    }
                }
            }
        }
        collect(&self.root, &mut used);

        let mut to_check: Vec<u16> = used.iter().copied().collect();
        while let Some(idx) = to_check.pop() {
            if let Some(Some(vs)) = vslots.get(idx as usize) {
                if let Some(layer) = vs.layer {
                    if used.insert(layer as u16) {
                        to_check.push(layer as u16);
                    }
                }
                if vs.prev >= 0 {
                    if used.insert(vs.prev as u16) {
                        to_check.push(vs.prev as u16);
                    }
                }
            }
        }

        // Wipe unreferenced VSlots. We DO NOT remap indices, to preserve base textures.
        for i in 0..vslots.len() {
            if !used.contains(&(i as u16)) {
                vslots[i] = None;
            }
        }

        // Trim trailing empty slots
        while vslots.last() == Some(&None) {
            vslots.pop();
        }
    }

    pub fn new_empty_root() -> Node {
        let solid = Node::Leaf(Cube {
            edges: [0x80; 12],
            textures: [1; 6],
            material: Default::default(),
        });
        let air = Node::Leaf(Cube {
            edges: [0x00; 12],
            textures: [0; 6],
            material: Default::default(),
        });
        Node::Branch(Box::new([
            solid.clone(),
            solid.clone(),
            solid.clone(),
            solid,
            air.clone(),
            air.clone(),
            air.clone(),
            air,
        ]))
    }

    pub fn new_empty(size: i32) -> Self {
        World {
            size,
            root: Self::new_empty_root(),
            entities: vec![],
        }
    }

    pub fn enlarge_empty(&mut self) {
        let new_size = self.size * 2;
        let air = Node::Leaf(Cube {
            edges: [0x00; 12],
            textures: [0; 6],
            material: Default::default(),
        });
        let old_root = std::mem::replace(&mut self.root, air.clone());
        self.root = Node::Branch(Box::new([
            old_root,
            air.clone(),
            air.clone(),
            air.clone(),
            air.clone(),
            air.clone(),
            air.clone(),
            air,
        ]));
        self.size = new_size;
    }

    pub fn enlarge(&mut self) {
        let new_size = self.size * 2;
        let solid = Node::Leaf(Cube {
            edges: [0x80; 12],
            textures: [1; 6],
            material: Default::default(),
        });
        let air = Node::Leaf(Cube {
            edges: [0x00; 12],
            textures: [0; 6],
            material: Default::default(),
        });
        let old_root = std::mem::replace(&mut self.root, air.clone());
        self.root = Node::Branch(Box::new([
            old_root,
            solid.clone(),
            solid.clone(),
            solid,
            air.clone(),
            air.clone(),
            air.clone(),
            air,
        ]));
        self.size = new_size;
    }

    pub fn apply_material(
        &mut self,
        sel: &Selection,
        matid: u16,
        matmask: u16,
        filtermat: u16,
        filtermask: u16,
        filtergeom: i32,
    ) {
        const EDITMATF_EMPTY: i32 = 0x10000;
        const EDITMATF_NOTEMPTY: i32 = 0x20000;
        const EDITMATF_SOLID: i32 = 0x30000;
        const EDITMATF_NOTSOLID: i32 = 0x40000;
        const MAT_AIR: u16 = 0;
        fn apply(
            node: &mut Node,
            matid: u16,
            matmask: u16,
            filtermat: u16,
            filtermask: u16,
            filtergeom: i32,
        ) {
            match node {
                Node::Leaf(cube) => {
                    let current = cube.material.to_bits();
                    if (current & filtermask) == filtermat {
                        let skip = match filtergeom {
                            EDITMATF_EMPTY => !cube.is_empty(),
                            EDITMATF_NOTEMPTY => cube.is_empty(),
                            EDITMATF_SOLID => !cube.is_entirely_solid(),
                            EDITMATF_NOTSOLID => cube.is_entirely_solid(),
                            _ => false,
                        };
                        if !skip {
                            let new_bits = if matid != MAT_AIR {
                                (current & matmask) | matid
                            } else {
                                MAT_AIR
                            };
                            cube.material = Material::from_bits(new_bits);
                        }
                    }
                }
                Node::Branch(children) => {
                    for child in children.iter_mut() {
                        apply(child, matid, matmask, filtermat, filtermask, filtergeom);
                    }
                }
            }
        }
        self.edit_sel(sel, |node, _| {
            apply(node, matid, matmask, filtermat, filtermask, filtergeom)
        });
    }

    pub fn replace_texture(&mut self, sel: Option<&Selection>, old_tex: u16, new_tex: u16) {
        fn replace(node: &mut Node, old_tex: u16, new_tex: u16) {
            match node {
                Node::Leaf(cube) => {
                    for t in &mut cube.textures {
                        if *t == old_tex {
                            *t = new_tex;
                        }
                    }
                }
                Node::Branch(children) => {
                    for child in children.iter_mut() {
                        replace(child, old_tex, new_tex);
                    }
                }
            }
        }
        if let Some(s) = sel {
            self.edit_sel(s, |node, _| replace(node, old_tex, new_tex));
        } else {
            replace(&mut self.root, old_tex, new_tex);
        }
    }

    pub fn apply_vslots(
        &mut self,
        sel: &Selection,
        all_faces: bool,
        delta: bool,
        extra_data: &[u8],
        vslots: &mut Vec<Option<VSlot>>,
    ) {
        let Some(ds) = crate::vslot::parse_packed_vslot(extra_data) else {
            return;
        };
        let orient = sel.orient as usize;

        fn crushededge(e: u8, dc: usize) -> bool {
            if dc != 0 { e == 0 } else { e == 0x88 }
        }
        fn touchingface(cube: &Cube, orient: usize) -> bool {
            let d = orient >> 1;
            let dc = orient & 1;
            for x in 0..2 {
                for y in 0..2 {
                    let edge = cube.edges[(d << 2) + (y << 1) + x];
                    let val = if dc == 1 { edge >> 4 } else { edge & 0x0F };
                    if val != (if dc == 1 { 8 } else { 0 }) {
                        return false;
                    }
                }
            }
            true
        }
        fn visible_orient(cube: &Cube, orient: usize) -> usize {
            const FACEEDGESIDX: [[usize; 4]; 6] = [
                [4, 5, 8, 10],
                [6, 7, 9, 11],
                [8, 9, 0, 2],
                [10, 11, 1, 3],
                [0, 1, 4, 6],
                [2, 3, 5, 7],
            ];
            for i in 0..2 {
                let a = FACEEDGESIDX[orient][i * 2 + 0];
                let b = FACEEDGESIDX[orient][i * 2 + 1];
                for j in 0..2 {
                    if crushededge(cube.edges[a], j)
                        && crushededge(cube.edges[b], j)
                        && touchingface(cube, orient)
                    {
                        return ((a >> 2) << 1) + j;
                    }
                }
            }
            orient
        }
        fn collect_vslots(
            node: &Node,
            orient: usize,
            all_faces: bool,
            seen: &mut HashSet<u16>,
            ordered: &mut Vec<u16>,
        ) {
            match node {
                Node::Branch(children) => {
                    for child in children.as_ref() {
                        collect_vslots(child, orient, all_faces, seen, ordered);
                    }
                }
                Node::Leaf(cube) => {
                    if all_faces {
                        for face in 0..6 {
                            let t = cube.textures[face];
                            if seen.insert(t) {
                                ordered.push(t);
                            }
                        }
                    } else {
                        let i = visible_orient(cube, orient);
                        let t = cube.textures[i];
                        if seen.insert(t) {
                            ordered.push(t);
                        }
                    }
                }
            }
        }
        fn apply_vslots_recursive(
            node: &mut Node,
            orient: usize,
            all_faces: bool,
            remap: &HashMap<u16, u16>,
        ) {
            match node {
                Node::Branch(children) => {
                    for child in children.iter_mut() {
                        apply_vslots_recursive(child, orient, all_faces, remap);
                    }
                }
                Node::Leaf(cube) => {
                    if all_faces {
                        for face in 0..6 {
                            if let Some(&new_t) = remap.get(&cube.textures[face]) {
                                cube.textures[face] = new_t;
                            }
                        }
                    } else {
                        let i = visible_orient(cube, orient);
                        if let Some(&new_t) = remap.get(&cube.textures[i]) {
                            cube.textures[i] = new_t;
                        }
                    }
                }
            }
        }

        let mut seen = HashSet::new();
        let mut ordered_textures = Vec::new();
        let orient_dim = sel.orient as usize >> 1;
        let dc = sel.orient as usize & 1;
        let r_dim = [1, 2, 0][orient_dim];
        let c_dim = [2, 0, 1][orient_dim];
        let d_dim = [0, 1, 2][orient_dim];
        let s_r = sel.s[r_dim];
        let s_c = sel.s[c_dim];
        let s_d = sel.s[d_dim];

        for z in 0..s_d {
            for y in 0..s_c {
                for x in 0..s_r {
                    let mut pos = [0; 3];
                    pos[r_dim] = x * sel.grid;
                    pos[c_dim] = y * sel.grid;
                    pos[d_dim] = dc as i32 * (s_d - 1) * sel.grid;
                    pos[0] += sel.o[0];
                    pos[1] += sel.o[1];
                    pos[2] += sel.o[2];
                    if dc == 1 {
                        pos[d_dim] -= z * sel.grid;
                    } else {
                        pos[d_dim] += z * sel.grid;
                    }
                    let node = crate::octree::extract_node_at(
                        &self.root,
                        [0, 0, 0],
                        self.size,
                        pos,
                        sel.grid,
                    );
                    collect_vslots(&node, orient, all_faces, &mut seen, &mut ordered_textures);
                }
            }
        }

        let tex_remap: HashMap<u16, u16> = ordered_textures
            .iter()
            .map(|&t| (t, crate::vslot::resolve_vslot(vslots, t, &ds, delta)))
            .collect();

        self.edit_sel(sel, |node, _| {
            apply_vslots_recursive(node, orient, all_faces, &tex_remap)
        });
    }

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
        if sel.grid <= 0 || [sel.s.x, sel.s.y, sel.s.z].into_iter().any(|n| n <= 0) {
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

    /// Directly sets the octree node at a power-of-2 aligned (pos, size) coordinate.
    /// Subdivides any Leaf nodes encountered on the way down as needed.
    pub fn set_node_at(&mut self, pos: [i32; 3], size: i32, node: Node) {
        fn recurse(
            cur: &mut Node,
            cur_pos: [i32; 3],
            cur_size: i32,
            target_pos: [i32; 3],
            target_size: i32,
            new_node: Node,
        ) {
            if cur_size == target_size {
                *cur = new_node;
                return;
            }
            if let Node::Leaf(cube) = cur {
                let children = crate::geom::subdivide(cube);
                *cur = Node::Branch(Box::new(children.map(Node::Leaf)));
            }
            if let Node::Branch(children) = cur {
                let half = cur_size / 2;
                let cx = usize::from(target_pos[0] >= cur_pos[0] + half);
                let cy = usize::from(target_pos[1] >= cur_pos[1] + half);
                let cz = usize::from(target_pos[2] >= cur_pos[2] + half);
                let child_idx = cx | (cy << 1) | (cz << 2);
                let child_pos = [
                    cur_pos[0] + cx as i32 * half,
                    cur_pos[1] + cy as i32 * half,
                    cur_pos[2] + cz as i32 * half,
                ];
                recurse(
                    &mut children[child_idx],
                    child_pos,
                    half,
                    target_pos,
                    target_size,
                    new_node,
                );
            }
        }
        recurse(&mut self.root, [0, 0, 0], self.size, pos, size, node);
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
