use cube_string::{CubeString, IntoCubeStringLossy};
use ogz_rs::{
    OgzMap,
    data::{MapConfig, VSlot, VSlotParam, Variable},
};
use sauer_map::{Cube, Node, Selection, World};
use sauer_proto::{
    channel1, channel2,
    data::{EditVarValue, SelInfo},
};
use sauer_server::plugin::Context;
use sauer_server::{Server, ServerItem, delay_spawn, entity_to_item_type, spawntime};
use std::collections::{HashMap, HashSet};
use std::sync::RwLock;
use tracing::error;

/// Converts `sauer_proto` network bounds into `sauer_map` selection areas.
pub fn to_selection(sel: &SelInfo) -> Selection {
    Selection {
        o: [sel.o.x, sel.o.y, sel.o.z],
        s: [sel.s.x, sel.s.y, sel.s.z],
        grid: sel.grid,
        orient: sel.orient as u8,
        cx: sel.cx as i32,
        cxs: sel.cxs as i32,
        cy: sel.cy as i32,
        cys: sel.cys as i32,
        corner: sel.corner as i32,
    }
}

#[derive(Clone)]
pub struct MapState {
    pub world: World,
    pub meta: OgzMap,
}

impl MapState {
    pub fn default_empty() -> Self {
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
        let world = World {
            size: 512,
            root,
            entities: vec![],
        };
        let meta = OgzMap {
            version: 33,
            world_size: 512,
            config: MapConfig::Modern { variables: vec![] },
            game_type: Some("fps".lossy_cubestring()),
            eif: 0,
            extra_game_data: vec![],
            mru_textures: vec![],
            entities: vec![],
            vslots: vec![None, None],
            octree: world.to_ogz_octree(),
            lightmaps: vec![],
            pvs: vec![],
            blendmap_resolution: 0,
            blendmap_data: None,
        };
        Self { world, meta }
    }

    pub fn from_bytes(data: &[u8]) -> Result<Self, ogz_rs::OgzError> {
        let ogz = OgzMap::parse(data)?;
        let world = World::from_ogz(&ogz);
        Ok(Self { world, meta: ogz })
    }

    pub fn to_ogz_bytes(&self) -> Result<Vec<u8>, ogz_rs::OgzError> {
        let mut ogz = self.meta.clone();
        ogz.octree = self.world.to_ogz_octree();
        ogz.entities = self.world.entities.clone();
        ogz.serialize()
    }

    pub fn invalidate_cache(&mut self) {
        self.meta.lightmaps.clear();
        self.meta.pvs.clear();
    }

    pub fn update_server_items(&self, server: &Server) {
        let num_players = server.client_count();
        let mut items = server.items.write().unwrap();
        items.reset();

        for (i, ent) in self.world.entities.iter().enumerate() {
            if let ogz_rs::data::EntityType::GameSpecific(n) = ent.ent_type {
                let Ok(proto_type) = sauer_proto::data::EntityType::try_from(n as i32) else {
                    continue;
                };
                if let Some(item_type) = entity_to_item_type(proto_type) {
                    if items.list.len() <= i {
                        items.list.resize_with(i + 1, || ServerItem {
                            item_type,
                            spawned: false,
                            spawn_time_ms: 0,
                        });
                    }
                    items.list[i].item_type = item_type;
                    if delay_spawn(item_type) {
                        items.list[i].spawned = false;
                        items.list[i].spawn_time_ms = spawntime(item_type, num_players);
                    } else {
                        items.list[i].spawned = true;
                        items.list[i].spawn_time_ms = 0;
                    }
                }
            }
        }
        items.notgotitems = false;
    }

    pub fn handle_edit_var(&mut self, packet: &channel1::EditVar) {
        let ogz_var = match &packet.val {
            EditVarValue::Int(v) => Variable::Int {
                name: packet.name.clone(),
                value: *v,
            },
            EditVarValue::Float(v) => Variable::Float {
                name: packet.name.clone(),
                value: *v,
            },
            EditVarValue::String(v) => Variable::String {
                name: packet.name.clone(),
                value: v.clone(),
            },
        };

        if let ogz_rs::data::MapConfig::Modern { variables } = &mut self.meta.config {
            if let Some(existing) = variables.iter_mut().find(|v| match v {
                Variable::Int { name: n, .. } => n == &packet.name,
                Variable::Float { name: n, .. } => n == &packet.name,
                Variable::String { name: n, .. } => n == &packet.name,
            }) {
                *existing = ogz_var;
            } else {
                variables.push(ogz_var);
            }
        }
    }

    pub fn handle_edit_tex(&mut self, packet: &channel1::EditTex) {
        self.world.set_texture(
            &to_selection(&packet.sel),
            packet.tex as u16,
            packet.all_faces,
        );
        self.invalidate_cache();
    }

    pub fn handle_edit_mat(&mut self, packet: &channel1::EditMat) {
        let packet_mat = packet.mat as i32;
        let packet_filter = packet.filter as i32;

        let mut filtermat = 0u16;
        let mut filtermask = 0u16;
        let mut filtergeom = 0i32;

        if packet_filter >= 0 {
            filtermat = (packet_filter & 0xFFFF) as u16;
            filtergeom = packet_filter & !0xFFFF;
            filtermask = if (filtermat & 0x1F) != 0 {
                0x1F
            } else if (filtermat & 0xE0) != 0 {
                0xE0
            } else {
                filtermat
            };
        }

        let (matid, matmask) = if packet_mat < 0 {
            let matmask = if (filtermat & 0x1C) == 0x0C {
                filtermask & !0xE0u16
            } else if (filtermat & 0x1C) == 0x08 {
                filtermask & !0x0100u16
            } else {
                filtermask
            };
            (0u16, matmask)
        } else {
            let mut matid = (packet_mat & 0xFFFF) as u16;
            let matmask = if (matid & 0x1F) != 0 {
                0
            } else if (matid & 0xE0) != 0 {
                !0xE0u16
            } else {
                !matid
            };
            if (matid & 0x1C) == 0x0C {
                matid |= 0x40;
            }
            if (matid & 0x1C) == 0x08 {
                matid |= 0x0100;
            }
            (matid, matmask)
        };

        self.world.edit_sel(&to_selection(&packet.sel), |node, _| {
            Self::apply_mat(node, matid, matmask, filtermat, filtermask, filtergeom);
        });
        self.invalidate_cache();
    }

    fn apply_mat(
        node: &mut Node,
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
                        cube.material = ogz_rs::data::Material::from_bits(new_bits);
                    }
                }
            }
            Node::Branch(children) => {
                for child in children.iter_mut() {
                    Self::apply_mat(child, matid, matmask, filtermat, filtermask, filtergeom);
                }
            }
        }
    }

    pub fn handle_del_cube(&mut self, packet: &channel1::DelCube) {
        self.world.fill(&to_selection(&packet.sel), false);
        self.invalidate_cache();
    }

    pub fn handle_replace(&mut self, packet: &channel1::Replace) {
        let old_tex = packet.old_tex as u16;
        let new_tex = packet.new_tex as u16;
        if packet.in_sel != 0 {
            self.world.edit_sel(&to_selection(&packet.sel), |node, _| {
                Self::replace_tex(node, old_tex, new_tex);
            });
        } else {
            Self::replace_tex(&mut self.world.root, old_tex, new_tex);
        }
        self.invalidate_cache();
    }

    fn replace_tex(node: &mut Node, old_tex: u16, new_tex: u16) {
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
                    Self::replace_tex(child, old_tex, new_tex);
                }
            }
        }
    }

    pub fn handle_edit_face(&mut self, packet: &channel1::EditFace) {
        self.world
            .edit_face(&to_selection(&packet.sel), packet.dir, packet.mode as i32);
        self.invalidate_cache();
    }

    pub fn handle_edit_ent(&mut self, packet: &channel1::EditEnt) {
        self.world.edit_ent(
            packet.ent_id as usize,
            [packet.pos.x, packet.pos.y, packet.pos.z],
            (packet.type_ as u8).into(),
            packet.attrs.map(|x| x as i16),
        );
    }

    pub fn apply_block(&mut self, sel: &SelInfo, nodes: Vec<sauer_map::Node>, transparent: bool) {
        if nodes.is_empty() {
            return;
        }

        let orient = sel.orient as usize;
        let d = orient >> 1;
        let r_dim = [1usize, 2, 0][d];
        let c_dim = [2usize, 0, 1][d];
        let d_dim = [0usize, 1, 2][d];

        let s_arr = [sel.s.x, sel.s.y, sel.s.z];
        let s_r = s_arr[r_dim];
        let s_c = s_arr[c_dim];
        let s_d = s_arr[d_dim];

        let sel_o = [sel.o.x, sel.o.y, sel.o.z];

        self.world.edit_sel(&to_selection(sel), |dest_node, pos| {
            let dx = (pos[r_dim] - sel_o[r_dim]) / sel.grid;
            let dy = (pos[c_dim] - sel_o[c_dim]) / sel.grid;

            // Sauerbraten maps the Z-axis backwards in the array when dimcoord(orient) != 0
            let dc = orient & 1;
            let pos_d = (pos[d_dim] - sel_o[d_dim]) / sel.grid;
            let dz = if dc != 0 { s_d - 1 - pos_d } else { pos_d };

            if dx >= 0 && dx < s_r && dy >= 0 && dy < s_c && dz >= 0 && dz < s_d {
                let idx = (dz * s_c * s_r + dy * s_r + dx) as usize;
                if idx < nodes.len() {
                    let src_node = &nodes[idx];
                    let should_paste = if transparent {
                        match src_node {
                            sauer_map::Node::Leaf(cube) => {
                                !cube.is_empty() || cube.material != Default::default()
                            }
                            sauer_map::Node::Branch(_) => true,
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

    pub fn handle_paste(&mut self, packet: &channel1::PasteClient, clipboard_data: &[u8]) {
        if let Some((cb_sel, nodes)) = crate::block_reader::decode_clipboard(clipboard_data) {
            let mut paste_sel = packet.sel.clone();
            paste_sel.s = cb_sel.s; // Paste dynamically replaces bounds...
            paste_sel.orient = cb_sel.orient; // ...and mapping orientation.
            self.apply_block(&paste_sel, nodes, true); // Paste is natively transparent
        }
    }

    pub fn handle_undo_redo(&mut self, data: &[u8]) {
        if let Some((sel, nodes)) = crate::block_reader::decode_undo(data) {
            self.apply_block(&sel, nodes, false);
        }
    }

    pub fn handle_edit_v_slot(&mut self, packet: &channel1::EditVSlot) {
        if packet.extra_data.is_empty() {
            return;
        }
        let Some(ds) = parse_packed_vslot(&packet.extra_data) else {
            return;
        };

        let sel = to_selection(&packet.sel);
        let orient = sel.orient as usize;
        let all_faces = packet.all_faces;

        // Collect unique texture indices from the selection.
        let mut unique_textures: HashSet<u16> = HashSet::new();
        self.world.edit_sel(&sel, |node, _| {
            if let Node::Leaf(cube) = node {
                for face in 0usize..6 {
                    if all_faces || face == orient {
                        unique_textures.insert(cube.textures[face]);
                    }
                }
            }
        });

        // Resolve (or create) a vslot for each unique base texture.
        let vslots = &mut self.meta.vslots;
        let tex_remap: HashMap<u16, u16> = unique_textures
            .into_iter()
            .map(|t| (t, resolve_vslot(vslots, t, &ds)))
            .collect();

        // Apply the remapped texture indices.
        self.world.edit_sel(&sel, |node, _| {
            if let Node::Leaf(cube) = node {
                for face in 0usize..6 {
                    if all_faces || face == orient {
                        if let Some(&new_t) = tex_remap.get(&cube.textures[face]) {
                            cube.textures[face] = new_t;
                        }
                    }
                }
            }
        });
        self.invalidate_cache();
    }
}

// ── VSlot tracking ───────────────────────────────────────────────────────────

struct PackedVSlot {
    params: Vec<(CubeString, [f32; 4])>,
    scale: Option<f32>,
    rotation: Option<i32>,
    offset: Option<(i32, i32)>,
    scroll: Option<(f32, f32)>,
    layer: Option<i32>,
    alpha: Option<(f32, f32)>,
    color: Option<(f32, f32, f32)>,
}

/// Reads a Sauerbraten varint (putint encoding) from a byte slice.
fn read_varint(data: &[u8], pos: &mut usize) -> Option<i32> {
    if *pos >= data.len() {
        return None;
    }
    let c = data[*pos] as i8;
    *pos += 1;
    match c {
        -128 => {
            if *pos + 2 > data.len() {
                return None;
            }
            let v = i16::from_le_bytes([data[*pos], data[*pos + 1]]) as i32;
            *pos += 2;
            Some(v)
        }
        -127 => {
            if *pos + 4 > data.len() {
                return None;
            }
            let v = i32::from_le_bytes(data[*pos..*pos + 4].try_into().ok()?);
            *pos += 4;
            Some(v)
        }
        _ => Some(c as i32),
    }
}

fn read_f32_le(data: &[u8], pos: &mut usize) -> Option<f32> {
    if *pos + 4 > data.len() {
        return None;
    }
    let v = f32::from_le_bytes(data[*pos..*pos + 4].try_into().ok()?);
    *pos += 4;
    Some(v)
}

/// Reads a sendstring-encoded name: each char is a putint, null-terminated.
fn read_sendstring(data: &[u8], pos: &mut usize) -> Option<CubeString> {
    let mut bytes = Vec::new();
    loop {
        let c = read_varint(data, pos)? as u8;
        if c == 0 {
            break;
        }
        bytes.push(c);
    }
    Some(CubeString::from_bytes(bytes))
}

/// Reads a Sauerbraten getuint (putuint encoding) from a byte slice.
fn read_getuint(data: &[u8], pos: &mut usize) -> Option<i32> {
    let b0 = *data.get(*pos)? as i32;
    *pos += 1;
    if b0 & 0x80 == 0 {
        return Some(b0);
    }
    let b1 = *data.get(*pos)? as i32;
    *pos += 1;
    let mut n = b0 + (b1 << 7) - 0x80;
    if n & (1 << 14) != 0 {
        let b2 = *data.get(*pos)? as i32;
        *pos += 1;
        n += (b2 << 14) - (1 << 14);
    }
    if n & (1 << 21) != 0 {
        let b3 = *data.get(*pos)? as i32;
        *pos += 1;
        n += (b3 << 21) - (1 << 21);
    }
    if n & (1 << 28) != 0 {
        n |= !0i32 << 28;
    }
    Some(n)
}

/// Parses a packvslot-encoded blob from an N_EDITVSLOT extra_data buffer.
///
/// Format: repeated (type_byte, data...) entries terminated by 0xFF.
/// Type bytes: 0=SHPARAM, 1=SCALE, 2=ROTATION, 3=OFFSET, 4=SCROLL,
///             5=LAYER, 6=ALPHA, 7=COLOR.
fn parse_packed_vslot(data: &[u8]) -> Option<PackedVSlot> {
    let mut pos = 0;
    let mut params = Vec::new();
    let mut scale = None;
    let mut rotation = None;
    let mut offset = None;
    let mut scroll = None;
    let mut layer = None;
    let mut alpha = None;
    let mut color = None;

    loop {
        if pos >= data.len() {
            break;
        }
        let type_byte = data[pos];
        pos += 1;
        if type_byte >= 0x80 {
            break;
        }
        match type_byte {
            0 => {
                // VSLOT_SHPARAM
                let name = read_sendstring(data, &mut pos)?;
                let vals = [
                    read_f32_le(data, &mut pos)?,
                    read_f32_le(data, &mut pos)?,
                    read_f32_le(data, &mut pos)?,
                    read_f32_le(data, &mut pos)?,
                ];
                params.push((name, vals));
            }
            1 => scale = Some(read_f32_le(data, &mut pos)?),
            2 => rotation = Some(read_varint(data, &mut pos)?),
            3 => offset = Some((read_varint(data, &mut pos)?, read_varint(data, &mut pos)?)),
            4 => {
                // VSLOT_SCROLL: values already stored as arg/1000 (raw float)
                scroll = Some((read_f32_le(data, &mut pos)?, read_f32_le(data, &mut pos)?));
            }
            5 => layer = Some(read_getuint(data, &mut pos)?),
            6 => alpha = Some((read_f32_le(data, &mut pos)?, read_f32_le(data, &mut pos)?)),
            7 => {
                color = Some((
                    read_f32_le(data, &mut pos)?,
                    read_f32_le(data, &mut pos)?,
                    read_f32_le(data, &mut pos)?,
                ))
            }
            _ => return None,
        }
    }

    Some(PackedVSlot {
        params,
        scale,
        rotation,
        offset,
        scroll,
        layer,
        alpha,
        color,
    })
}

/// Walks the prev chain from `idx` to the root base (None) slot index.
fn root_base_of(vslots: &[Option<VSlot>], idx: u16) -> i32 {
    let mut current = idx as i32;
    loop {
        match vslots.get(current as usize) {
            Some(Some(vs)) => {
                let p = vs.prev;
                if p < 0 || (p as usize) >= vslots.len() {
                    return current;
                }
                match &vslots[p as usize] {
                    None => return p,
                    Some(_) => current = p,
                }
            }
            _ => return current,
        }
    }
}

/// Finds the last vslot in the chain rooted at `root` (linear chain tail).
/// New vslots should use this as `prev` so all vslots remain reachable after OGZ load.
fn find_chain_tail(vslots: &[Option<VSlot>], root: i32) -> i32 {
    let mut tail = root;
    for i in 0..vslots.len() {
        if let Some(Some(_)) = vslots.get(i) {
            if root_base_of(vslots, i as u16) == root {
                tail = i as i32;
            }
        }
    }
    tail
}

/// Creates a merged vslot by accumulating `base_tex`'s existing properties
/// then overriding with `ds`. Sets `prev` to the chain tail to keep all vslots
/// reachable during OGZ load (avoids orphaning when multiple vslots share a root).
fn resolve_vslot(vslots: &mut Vec<Option<VSlot>>, base_tex: u16, ds: &PackedVSlot) -> u16 {
    if vslots.len() <= base_tex as usize {
        vslots.resize(base_tex as usize + 1, None);
    }
    let root = root_base_of(vslots, base_tex);
    let chain_tail = find_chain_tail(vslots, root);

    let (acc_params, acc_scale, acc_rotation, acc_offset, acc_scroll, acc_layer, acc_alpha, acc_color) =
        match vslots.get(base_tex as usize) {
            Some(Some(existing)) => (
                existing.params.clone(),
                existing.scale,
                existing.rotation,
                existing.offset,
                existing.scroll,
                existing.layer,
                existing.alpha,
                existing.color,
            ),
            _ => (vec![], None, None, None, None, None, None, None),
        };

    let new_vs = VSlot {
        prev: chain_tail,
        params: if !ds.params.is_empty() {
            ds.params.iter().map(|(n, v)| VSlotParam { name: n.clone(), values: *v }).collect()
        } else {
            acc_params
        },
        scale: ds.scale.or(acc_scale),
        rotation: ds.rotation.or(acc_rotation),
        offset: ds.offset.or(acc_offset),
        scroll: ds.scroll.or(acc_scroll),
        layer: ds.layer.or(acc_layer),
        alpha: ds.alpha.or(acc_alpha),
        color: ds.color.or(acc_color),
    };
    let idx = vslots.len();
    vslots.push(Some(new_vs));
    idx as u16
}

#[derive(Clone)]
pub struct Trap {
    pub button_pos: [i32; 3],
    pub button_size: i32,
    pub button_orient: u8,

    /// Pre-compressed clipboard for the triggered geometry.
    pub trigger_clipboard: Vec<u8>,
    pub trigger_unpack_len: i32,

    /// Per-cell changes — used for accurate server-map updates (no region collateral).
    pub trigger_cells: Vec<([i32; 3], i32, sauer_map::Node)>,

    /// Modified existing entities when the trap is active.
    pub trigger_entities_mod: Vec<(usize, ogz_rs::data::Entity)>,

    /// Newly added entities when the trap is active.
    pub trigger_entities_add: Vec<ogz_rs::data::Entity>,

    /// DelCube/paste region — tight diff bounds, grid-aligned.
    pub clip_pos: [i32; 3],
    pub clip_s: [i32; 3],
    pub clip_grid: i32,

    pub time: u32,
    pub timeout: u32,

    /// Base-state geometry — what to restore when resetting from an active trap.
    /// Populated by `cmd_endtrap`; mirrors the original map's cells/entities for
    /// the trap region so a save-while-active can be correctly recovered on load.
    pub reset_clipboard: Vec<u8>,
    pub reset_unpack_len: i32,
    pub reset_cells: Vec<([i32; 3], i32, sauer_map::Node)>,
    pub reset_entities_mod: Vec<(usize, ogz_rs::data::Entity)>,

    pub is_active: bool,
}

pub struct DeathrunState {
    pub map: Option<MapState>,
    /// True while waiting for a player to upload a map after a map vote.
    pub waiting_for_map: bool,
    pub traps: Vec<Trap>,
    pub trap_edit: Option<crate::trap::TrapEditState>,
}

impl Default for DeathrunState {
    fn default() -> Self {
        Self {
            map: None,
            waiting_for_map: false,
            traps: Vec::new(),
            trap_edit: None,
        }
    }
}

pub(crate) fn get_server_state(server: &Server) -> &RwLock<DeathrunState> {
    server.extra.get().expect("DeathrunState not initialised")
}

pub(crate) fn is_waiting_for_map(server: &Server) -> bool {
    get_server_state(server).read().unwrap().waiting_for_map
}

pub(crate) fn with_map_state_mut<F>(ctx: &Context, f: F)
where
    F: FnOnce(&mut MapState),
{
    if let Some(map) = get_server_state(ctx.server()).write().unwrap().map.as_mut() {
        f(map);
    }
}

pub(crate) fn send_map_to(server: &Server, cn: u32) -> bool {
    let bytes = {
        let lock = get_server_state(server).read().unwrap();
        match lock.map.as_ref() {
            None => return false,
            Some(state) => match state.to_ogz_bytes() {
                Ok(b) => b,
                Err(e) => {
                    error!("failed to serialize map state: {e}");
                    return false;
                }
            },
        }
    };
    server.send_chan2(cn, &channel2::SendMap { data: bytes });
    true
}
