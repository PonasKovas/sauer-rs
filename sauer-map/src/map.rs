use crate::edit::{apply_region, fill, set_material, set_texture};
use crate::{Cube, Node, Selection, ValidSel};
use ogz_rs::OgzMap;
use ogz_rs::data::VSlot;
use ogz_rs::data::{Entity, Lightmap, MapConfig, Variable};
use sauer_core::Aabb;
use sauer_proto::data::VSlotProps;
use std::collections::{HashMap, HashSet};

/// Where [`Map::normalize_vslots`] puts variants: past the texture slots of
/// any client (Sauerbraten installs have about 1700).
/// Engine `MAXENTS`: clients ignore entities past it.
pub const MAX_ENTS: usize = 10_000;

pub const MIN_VARIANT_INDEX: usize = 4096;

/// Represents the complete, live state of a Sauerbraten map.
///
/// This structure encapsulates both the geometric octree (`Node`s and `Cube`s) and
/// the metadata required to render and interact with it (textures/vslots, entities, variables).
#[derive(Debug, Clone)]
pub struct Map {
    pub size: i32,
    pub root: Node,

    /// The map's texture slot (VSlot) registry. Contains material configurations and delta parameters.
    pub vslots: Vec<Option<VSlot>>,
    /// The map's list of interactive game entities.
    pub entities: Vec<Entity>,
    /// Global map variables controlling environment behavior.
    pub vars: Vec<Variable>,

    // Preservation of original OGZ metadata for lossless saving
    pub version: i32,
    pub game_type: Option<cube_string::CubeString>,
    pub eif: u16,
    pub extra_game_data: Vec<u8>,
    pub mru_textures: Vec<u16>,
    pub lightmaps: Vec<Lightmap>,
    pub pvs: Vec<u8>,
    pub blendmap_resolution: i32,
    pub blendmap_data: Option<Vec<u8>>,
    pub config: MapConfig,
}

impl Map {
    /// Creates a default empty map matching Sauerbraten's `emptymap(10)`
    pub fn default_empty() -> Self {
        Self::new_empty(1024)
    }

    /// Creates a fresh empty map of the specified world size.
    pub fn new_empty(size: i32) -> Self {
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
        let root = Node::Branch(Box::new([
            solid.clone(),
            solid.clone(),
            solid.clone(),
            solid,
            air.clone(),
            air.clone(),
            air.clone(),
            air,
        ]));

        Map {
            size,
            root,
            vslots: vec![None, None],
            entities: vec![],
            vars: vec![],
            version: 33,
            game_type: Some(cube_string::CubeString::from_bytes(b"fps".to_vec())),
            eif: 0,
            extra_game_data: vec![],
            mru_textures: vec![],
            lightmaps: vec![],
            pvs: vec![],
            blendmap_resolution: 0,
            blendmap_data: None,
            config: MapConfig::Modern { variables: vec![] },
        }
    }

    /// Parses a live map state from raw `.ogz` file bytes.
    pub fn from_bytes(data: &[u8]) -> Result<Self, ogz_rs::OgzError> {
        let ogz = OgzMap::parse(data)?;
        let vars = match &ogz.config {
            MapConfig::Modern { variables } => variables.clone(),
            _ => vec![],
        };

        let mut map = Map {
            size: ogz.world_size,
            root: crate::octree::build_node(&ogz.octree),
            vslots: ogz.vslots,
            entities: ogz.entities,
            vars,
            version: ogz.version,
            game_type: ogz.game_type,
            eif: ogz.eif,
            extra_game_data: ogz.extra_game_data,
            mru_textures: ogz.mru_textures,
            lightmaps: ogz.lightmaps,
            pvs: ogz.pvs,
            blendmap_resolution: ogz.blendmap_resolution,
            blendmap_data: ogz.blendmap_data,
            config: ogz.config,
        };

        map.compact_vslots();
        map.normalize_vslots();
        while map.size < 1024 {
            map.enlarge();
        }
        Ok(map)
    }

    /// Serializes the live map state back into an `.ogz` byte format.
    pub fn to_ogz_bytes(&self) -> Result<Vec<u8>, ogz_rs::OgzError> {
        self.to_ogz_bytes_with(flate2::Compression::default())
    }

    pub fn to_ogz_bytes_with(
        &self,
        compression: flate2::Compression,
    ) -> Result<Vec<u8>, ogz_rs::OgzError> {
        self.serialize(compression, crate::octree::node_to_ogz(&self.root))
    }

    /// [`Map::to_ogz_bytes_with`], with faces merged like `remip` does, so
    /// clients load it faster. Costs a pass over the octree. Falls back to
    /// the plain map if merging fails or makes the map too big to send.
    pub fn to_ogz_bytes_merged(
        &self,
        compression: flate2::Compression,
    ) -> Result<Vec<u8>, ogz_rs::OgzError> {
        /// The engine's file transfer limit (`sendfile` in engine/server.cpp).
        const MAX_SENT_MAP: usize = 16 << 20;
        let merged = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
            crate::facemerge::node_to_ogz_merged(&self.root, self.size)
        }));
        if let Ok(octree) = merged {
            if let Ok(bytes) = self.serialize(compression, octree) {
                if bytes.len() <= MAX_SENT_MAP {
                    return Ok(bytes);
                }
            }
        }
        self.to_ogz_bytes_with(compression)
    }

    fn serialize(
        &self,
        compression: flate2::Compression,
        octree: ogz_rs::data::OctreeNode,
    ) -> Result<Vec<u8>, ogz_rs::OgzError> {
        let compacted_vslots = self.get_compacted_vslots();

        let config = MapConfig::Modern {
            variables: self.vars.clone(),
        };

        let ogz = OgzMap {
            version: self.version,
            world_size: self.size,
            config,
            game_type: self.game_type.clone(),
            eif: self.eif,
            extra_game_data: self.extra_game_data.clone(),
            mru_textures: self.mru_textures.clone(),
            entities: self.entities.clone(),
            vslots: compacted_vslots,
            octree,
            lightmaps: self.lightmaps.clone(),
            pvs: self.pvs.clone(),
            blendmap_resolution: self.blendmap_resolution,
            blendmap_data: self.blendmap_data.clone(),
        };
        ogz.serialize_with(compression)
    }

    pub fn invalidate_cache(&mut self) {
        self.lightmaps.clear();
        self.pvs.clear();
    }

    /// The vslot table without its trailing empty entries. Unused variants
    /// stay: emptying one would turn it into a slot base and shift the slot
    /// of every base after it (see `vslot::slot_of_root`). Clients drop them
    /// when they compact.
    pub fn get_compacted_vslots(&self) -> Vec<Option<VSlot>> {
        let mut vslots = self.vslots.clone();
        while vslots.last() == Some(&None) {
            vslots.pop();
        }
        vslots
    }

    pub fn compact_vslots(&mut self) {
        self.vslots = self.get_compacted_vslots();
    }

    /// Renumbers the vslots so every texture slot's base is at its slot
    /// number, when a texture the map uses has variants stored below its
    /// base (see `vslot::slot_of_root`). Clients number bases by slot once
    /// they compact, and send plain textures as raw indices, so otherwise
    /// those would mean other textures here. Variants move to
    /// [`MIN_VARIANT_INDEX`] and up, past any client's slots. Returns whether
    /// anything changed.
    pub fn normalize_vslots(&mut self) -> bool {
        let len = self.vslots.len();
        // Variants stored below each index.
        let mut below = vec![0usize; len + 1];
        for i in 0..len {
            below[i + 1] = below[i] + self.vslots[i].is_some() as usize;
        }
        let variants_below = |i: usize| below[i.min(len)];

        let mut used = vec![false; 0x10000];
        fn collect(node: &Node, used: &mut [bool]) {
            match node {
                Node::Leaf(c) => c.textures.iter().for_each(|&t| used[t as usize] = true),
                Node::Branch(ch) => ch.iter().for_each(|c| collect(c, used)),
            }
        }
        collect(&self.root, &mut used);
        // Only real slots count: indices past any client's slots (junk some
        // maps have) draw as the default texture either way.
        let offset = MIN_VARIANT_INDEX;
        let needs = (0..used.len()).filter(|&t| used[t]).any(|t| {
            let base = match self.vslots.get(t) {
                Some(Some(_)) => crate::vslot::root_base_of(&self.vslots, t as u16) as usize,
                _ => t,
            };
            let below = variants_below(base);
            below > 0 && base - below < offset
        });
        let nvar = below[len];
        if !needs || offset + nvar > 0x10000 {
            return false;
        }
        // Keeps every entry's slot: base k goes to k, or past the moved
        // variants if k is beyond them.
        let remap = |i: usize| -> usize {
            match self.vslots.get(i) {
                Some(Some(_)) => offset + below[i],
                _ => {
                    let slot = i - variants_below(i);
                    if slot < offset { slot } else { slot + nvar }
                }
            }
        };
        let mut vslots: Vec<Option<VSlot>> = vec![None; offset + nvar];
        for (i, vs) in self.vslots.iter().enumerate() {
            let Some(vs) = vs else { continue };
            let mut vs = vs.clone();
            if vs.prev >= 0 {
                vs.prev = remap(vs.prev as usize) as i32;
            }
            if let Some(layer) = vs.props.layer {
                vs.props.layer = Some(remap(layer.max(0) as usize) as i32);
            }
            vslots[remap(i)] = Some(vs);
        }
        let table: Vec<u16> = (0..0x10000).map(|i| remap(i) as u16).collect();
        fn apply(node: &mut Node, table: &[u16]) {
            match node {
                Node::Leaf(c) => c.textures.iter_mut().for_each(|t| *t = table[*t as usize]),
                Node::Branch(ch) => ch.iter_mut().for_each(|c| apply(c, table)),
            }
        }
        apply(&mut self.root, &table);
        for t in &mut self.mru_textures {
            *t = table[*t as usize];
        }
        self.vslots = vslots;
        true
    }

    /// Doubles the world, keeping the old one in the origin octant. Unlike the
    /// engine's `enlargemap`, which fills the three new bottom octants with
    /// rock, every new octant is air, so callers that sync clients must send
    /// [`Map::enlarged_octants`] after the client-side enlarge.
    pub fn enlarge(&mut self) {
        let air = Node::Leaf(Cube {
            edges: [0x00; 12],
            textures: [0; 6],
            material: Default::default(),
        });
        let old_root = std::mem::replace(&mut self.root, air.clone());
        let mut children: [Node; 8] = std::array::from_fn(|_| air.clone());
        children[0] = old_root;
        self.root = Node::Branch(Box::new(children));
        self.size *= 2;
        if let Some(data) = &self.blendmap_data {
            self.blendmap_data = Some(ogz_rs::blendmap::enlarged(data));
        }
    }

    /// The bottom octants the engine fills with rock when enlarging to the
    /// current size (the ones [`Map::enlarge`] leaves as air).
    pub fn enlarged_octants(&self) -> [Aabb; 3] {
        let h = self.size / 2;
        [[h, 0, 0], [0, h, 0], [h, h, 0]].map(|o| Aabb::cube(o, h))
    }

    /// True when the box is untouched: all air, or all plain rock, with no materials.
    pub fn is_untouched_region(&self, b: Aabb) -> bool {
        let Aabb { min, max } = b;
        // `want`: None until the first leaf decides between rock (true) and air (false).
        fn check(
            node: &Node,
            pos: [i32; 3],
            size: i32,
            min: &[i32; 3],
            max: &[i32; 3],
            want: &mut Option<bool>,
        ) -> bool {
            if (0..3).any(|i| pos[i] >= max[i] || pos[i] + size <= min[i]) {
                return true;
            }
            match node {
                Node::Leaf(c) => {
                    if c.material != Default::default() {
                        return false;
                    }
                    let solid = if c.is_entirely_solid() {
                        true
                    } else if c.is_empty() {
                        false
                    } else {
                        return false;
                    };
                    *want.get_or_insert(solid) == solid
                }
                Node::Branch(ch) => {
                    let half = size / 2;
                    (0..8).all(|i| {
                        let p = [
                            pos[0] + (i & 1) as i32 * half,
                            pos[1] + ((i >> 1) & 1) as i32 * half,
                            pos[2] + ((i >> 2) & 1) as i32 * half,
                        ];
                        check(&ch[i], p, half, min, max, want)
                    })
                }
            }
        }
        check(&self.root, [0, 0, 0], self.size, &min, &max, &mut None)
    }

    /// Gives the selected faces a variant of texture `slot` carrying exactly the
    /// props in `ds` (like `N_EDITT` with `tex = 0x10000 | slot`). Returns the vslot index.
    pub fn assign_vslot(
        &mut self,
        sel: &ValidSel,
        all_faces: bool,
        slot: u16,
        ds: &VSlotProps,
    ) -> Option<u16> {
        let root = crate::vslot::root_of_slot(&self.vslots, slot);
        let idx = crate::vslot::resolve_vslot(&mut self.vslots, root, ds, false)?;
        self.set_texture(sel, idx, all_faces);
        Some(idx)
    }

    /// Extracts geometry within the given selection to create a payload for a clipboard copy.
    pub fn copy_selection(&mut self, sel: &ValidSel) -> sauer_proto::data::ClipboardPayload {
        let d = sel.orient as usize >> 1;
        let r_dim = [1usize, 2, 0][d];
        let c_dim = [2usize, 0, 1][d];
        let d_dim = [0usize, 1, 2][d];

        let s_r = sel.s[r_dim];
        let s_c = sel.s[c_dim];
        let s_d = sel.s[d_dim];
        let dc = sel.orient as usize & 1;

        let total_nodes = (s_r * s_c * s_d) as usize;
        let mut extracted_nodes = vec![
            Node::Leaf(Cube {
                edges: [0; 12],
                textures: [0; 6],
                material: Default::default()
            });
            total_nodes
        ];

        self.edit_sel(sel, |node, pos| {
            let dx = (pos[r_dim] - sel.o[r_dim]) / sel.grid;
            let dy = (pos[c_dim] - sel.o[c_dim]) / sel.grid;
            let pos_d = (pos[d_dim] - sel.o[d_dim]) / sel.grid;
            let dz = if dc != 0 { s_d - 1 - pos_d } else { pos_d };

            if dx >= 0 && dx < s_r && dy >= 0 && dy < s_c && dz >= 0 && dz < s_d {
                let idx = (dz * s_c * s_r + dy * s_r + dx) as usize;
                if idx < extracted_nodes.len() {
                    extracted_nodes[idx] = node.clone();
                }
            }
        });

        let vslots = crate::block::extract_vslots(&extracted_nodes, &self.vslots);

        sauer_proto::data::ClipboardPayload {
            sel: (**sel).clone(),
            nodes: extracted_nodes,
            vslots,
        }
    }

    // --- Geometry Base Methods ---

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
        let (mut ox, mut oy, mut oz) = (0, 0, 0);
        let (ix, iy, iz) = (x as i32, y as i32, z as i32);

        while let Node::Branch(children) = current_node {
            current_size /= 2;
            let cx = if ix >= ox + current_size { 1 } else { 0 };
            let cy = if iy >= oy + current_size { 1 } else { 0 };
            let cz = if iz >= oz + current_size { 1 } else { 0 };
            current_node = &children[(cx | (cy << 1) | (cz << 2)) as usize];
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

    pub fn edit_sel<F>(&mut self, sel: &ValidSel, mut f: F)
    where
        F: FnMut(&mut Node, [i32; 3]),
    {
        let Aabb { min, max } = sel.bounds();
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

    pub fn set_material(&mut self, sel: &ValidSel, mat: ogz_rs::data::Material) {
        self.edit_sel(sel, |node, _| set_material(node, mat));
    }

    pub fn set_texture(&mut self, sel: &ValidSel, tex: u16, all_faces: bool) {
        let orient = if all_faces {
            None
        } else {
            Some(sel.orient as usize)
        };
        self.edit_sel(sel, |node, _| set_texture(node, tex, orient));
    }

    pub fn fill(&mut self, sel: &ValidSel, is_solid: bool) {
        self.edit_sel(sel, |node, _| fill(node, is_solid));
    }

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
                *cur = Node::Branch(Box::new(crate::geom::subdivide(cube).map(Node::Leaf)));
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

    pub fn replace_texture(&mut self, sel: Option<&ValidSel>, old_tex: u16, new_tex: u16) {
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

    pub fn apply_vslots(&mut self, sel: &ValidSel, all_faces: bool, delta: bool, ds: &VSlotProps) {
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

        // Textures with no room for a new variant stay as they are.
        let tex_remap: HashMap<u16, u16> = ordered_textures
            .iter()
            .filter_map(|&t| {
                Some((
                    t,
                    crate::vslot::resolve_vslot(&mut self.vslots, t, ds, delta)?,
                ))
            })
            .collect();
        self.edit_sel(sel, |node, _| {
            apply_vslots_recursive(node, orient, all_faces, &tex_remap)
        });
    }

    pub fn edit_face(&mut self, sel: &ValidSel, dir: i32, mode: i32) {
        use crate::geom::{C, R, dimcoord, dimension, linked_push, optiface};
        let d = dimension(sel.orient as usize);
        let dc = dimcoord(sel.orient as usize);
        let seldir = if dc == 1 { -dir } else { dir };
        let mut actual_mode = mode;
        if actual_mode == 1
            && (sel.cx > 0 || sel.cy > 0 || (sel.cxs & 1) != 0 || (sel.cys & 1) != 0)
        {
            actual_mode = 0;
        }
        let mut local_sel: Selection = (**sel).clone();

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

        let mut adj_textures = HashMap::new();
        if actual_mode == 1 && dir < 0 {
            let r = R[d];
            let c = C[d];
            for x in 0..local_sel.s[r] {
                for y in 0..local_sel.s[c] {
                    let mut pos = local_sel.o;
                    pos[r] += x * local_sel.grid;
                    pos[c] += y * local_sel.grid;
                    pos[d] -= seldir * local_sel.grid;
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

        // The face slice of a valid selection still lies in the world.
        let Some(face_sel) = local_sel.validate(self.size) else {
            return;
        };
        self.edit_sel(&face_sel, |node, pos| {
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
                *node = Node::Leaf(Cube {
                    edges: [0x80; 12],
                    textures: inherited_tex,
                    material: inherited_mat,
                });
            }

            if let Node::Leaf(c) = node {
                if actual_mode == 1 {
                    if dir < 0 {
                        c.edges = [0x80; 12];
                        let rx = pos[R[d]];
                        let ry = pos[C[d]];
                        if let Some(tex) = adj_textures.get(&[rx, ry]) {
                            c.textures = *tex;
                        }
                    } else {
                        c.edges = [0x00; 12];
                    }
                } else {
                    let bak = [
                        c.edges[d * 4],
                        c.edges[d * 4 + 1],
                        c.edges[d * 4 + 2],
                        c.edges[d * 4 + 3],
                    ];
                    if actual_mode == 2 {
                        linked_push(
                            c,
                            d,
                            (sel.corner & 1) as usize,
                            (sel.corner >> 1) as usize,
                            dc,
                            seldir,
                        );
                    } else {
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

    pub fn apply_block(&mut self, sel: &ValidSel, nodes: Vec<sauer_core::Node>, transparent: bool) {
        if nodes.is_empty() {
            return;
        }
        let orient = sel.orient as usize;
        let d = orient >> 1;
        let r_dim = [1usize, 2, 0][d];
        let c_dim = [2usize, 0, 1][d];
        let d_dim = [0usize, 1, 2][d];

        let s_r = sel.s[r_dim];
        let s_c = sel.s[c_dim];
        let s_d = sel.s[d_dim];
        let dc = orient & 1;

        self.edit_sel(sel, |dest_node, pos| {
            let dx = (pos[r_dim] - sel.o[r_dim]) / sel.grid;
            let dy = (pos[c_dim] - sel.o[c_dim]) / sel.grid;
            let pos_d = (pos[d_dim] - sel.o[d_dim]) / sel.grid;
            let dz = if dc != 0 { s_d - 1 - pos_d } else { pos_d };

            if dx >= 0 && dx < s_r && dy >= 0 && dy < s_c && dz >= 0 && dz < s_d {
                let idx = (dz * s_c * s_r + dy * s_r + dx) as usize;
                if idx < nodes.len() {
                    let src_node = &nodes[idx];
                    let should_paste = if transparent {
                        match src_node {
                            Node::Leaf(cube) => {
                                !cube.is_empty() || cube.material != Default::default()
                            }
                            Node::Branch(_) => true,
                        }
                    } else {
                        true
                    };
                    if should_paste {
                        *dest_node = src_node.clone();
                    }
                }
            }
        });
        self.invalidate_cache();
    }
}
