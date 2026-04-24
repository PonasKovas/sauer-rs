use ogz_rs::{OgzMap, data::Variable};
use sauer_map::{Node, Selection, World};
use sauer_proto::{
    channel1,
    data::{EditVarValue, SelInfo},
};
use sauer_server::{Server, ServerItem, delay_spawn, entity_to_item_type, spawntime};
use std::sync::RwLock;

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
        let num_players = server.client_cns().len();
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
}

pub struct DeathrunState {
    pub map: Option<MapState>,
    /// True while waiting for a player to upload a map after a map vote.
    pub waiting_for_map: bool,
}

impl Default for DeathrunState {
    fn default() -> Self {
        Self {
            map: None,
            waiting_for_map: false,
        }
    }
}

pub(crate) fn get_server_state(server: &Server) -> &RwLock<DeathrunState> {
    server.extra.get().expect("DeathrunState not initialised")
}
