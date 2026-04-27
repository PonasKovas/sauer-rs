use ogz_rs::{OgzMap, data::Variable};
use sauer_map::{Node, Selection, World};
use sauer_proto::{
    channel1, channel2,
    data::{EditVarValue, SelInfo},
};
use sauer_server::plugin::Context;
use sauer_server::{Server, ServerItem, delay_spawn, entity_to_item_type, spawntime};
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

pub struct MapState {
    pub world: World,
    pub meta: OgzMap,
}

impl MapState {
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
            self.apply_block(&sel, nodes, false); // Undo uses absolute overwrites
        }
    }
}

/// A placed trap button in the map.
pub struct Trap {
    pub button_pos: [i32; 3],
    pub button_size: i32,
    pub button_orient: u8,
}

pub struct DeathrunState {
    pub map: Option<MapState>,
    /// True while waiting for a player to upload a map after a map vote.
    pub waiting_for_map: bool,
    pub traps: Vec<Trap>,
}

impl Default for DeathrunState {
    fn default() -> Self {
        Self {
            map: None,
            waiting_for_map: false,
            traps: Vec::new(),
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

fn unpack_cube(bytes: &[u8], pos: &mut usize) -> Option<Node> {
    if *pos >= bytes.len() {
        return None;
    }
    let b = bytes[*pos];
    *pos += 1;
    if b == 255 {
        // OCTSAV_CHILDREN
        let mut children = Vec::with_capacity(8);
        for _ in 0..8 {
            children.push(unpack_cube(bytes, pos)?);
        }
        Some(Node::Branch(Box::new(children.try_into().unwrap())))
    } else {
        if *pos >= bytes.len() {
            return None;
        }
        let mat_hi = bytes[*pos];
        *pos += 1;
        let mat = (b as u16) | ((mat_hi as u16) << 8);

        let mut edges = [0u8; 12];
        if *pos + 12 > bytes.len() {
            return None;
        }
        edges.copy_from_slice(&bytes[*pos..*pos + 12]);
        *pos += 12;

        let mut textures = [0u16; 6];
        for i in 0..6 {
            if *pos + 2 > bytes.len() {
                return None;
            }
            textures[i] = u16::from_le_bytes([bytes[*pos], bytes[*pos + 1]]);
            *pos += 2;
        }

        Some(Node::Leaf(sauer_map::Cube {
            edges,
            textures,
            material: ogz_rs::data::Material::from_bits(mat),
        }))
    }
}
