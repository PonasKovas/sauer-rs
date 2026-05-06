use cube_string::IntoCubeStringLossy;
use ogz_rs::{
    OgzMap,
    data::{MapConfig, Variable},
};
use sauer_map::World;
use sauer_proto::{
    channel1, channel2,
    data::{EditVarValue, SelInfo},
};
use sauer_server::plugin::Context;
use sauer_server::{Server, ServerItem, delay_spawn, spawntime};
use std::sync::RwLock;
use tracing::error;

#[derive(Clone)]
pub struct MapState {
    pub world: World,
    pub meta: OgzMap,
}

impl MapState {
    pub fn default_empty() -> Self {
        Self::new_empty(1024)
    }

    /// Create a fresh empty map matching Sauerbraten's `emptymap(scale)`:
    /// bottom 4 root octants solid (texture 1), top 4 empty air.
    /// `world_size` is the actual size (e.g. 1024), not the log2 scale.
    pub fn new_empty(world_size: i32) -> Self {
        let world = World::new_empty(world_size);
        let meta = OgzMap {
            version: 33,
            world_size,
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

    pub fn enlarge(&mut self) {
        self.world.enlarge();
        self.meta.world_size = self.world.size;
        self.meta.octree = self.world.to_ogz_octree();
    }

    pub fn enlarge_empty(&mut self) {
        self.world.enlarge_empty();
        self.meta.world_size = self.world.size;
        self.meta.octree = self.world.to_ogz_octree();
    }

    pub fn from_bytes(data: &[u8]) -> Result<Self, ogz_rs::OgzError> {
        let ogz = OgzMap::parse(data)?;
        let world = World::from_ogz(&ogz);
        let mut state = Self { world, meta: ogz };

        // Aggressive garbage collection wipes out all ghost VSlots from undo/mru buffers
        state.world.compact_vslots(&mut state.meta.vslots);

        while state.world.size < 1024 {
            state.enlarge_empty();
        }

        Ok(state)
    }

    pub fn to_ogz_bytes(&self) -> Result<Vec<u8>, ogz_rs::OgzError> {
        let mut ogz = self.meta.clone();
        self.world.compact_vslots(&mut ogz.vslots);
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
                if let Ok(item_type) = sauer_proto::data::ItemType::try_from(proto_type) {
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
        self.world
            .set_texture(&packet.sel, packet.tex as u16, packet.all_faces);
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

        self.world.apply_material(
            &packet.sel,
            matid,
            matmask,
            filtermat,
            filtermask,
            filtergeom,
        );
        self.invalidate_cache();
    }

    pub fn handle_del_cube(&mut self, packet: &channel1::DelCube) {
        self.world.fill(&packet.sel, false);
        self.invalidate_cache();
    }

    pub fn handle_replace(&mut self, packet: &channel1::Replace) {
        let sel = if packet.in_sel != 0 {
            Some(packet.sel.clone())
        } else {
            None
        };
        self.world
            .replace_texture(sel.as_ref(), packet.old_tex as u16, packet.new_tex as u16);
        self.invalidate_cache();
    }

    pub fn handle_edit_face(&mut self, packet: &channel1::EditFace) {
        self.world
            .edit_face(&packet.sel, packet.dir, packet.mode as i32);
        self.invalidate_cache();
    }

    pub fn handle_edit_ent(&mut self, packet: &channel1::EditEnt) {
        self.world.edit_ent(
            packet.ent_id as usize,
            [packet.pos.x, packet.pos.y, packet.pos.z],
            packet.type_,
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

        let s_r = sel.s[r_dim];
        let s_c = sel.s[c_dim];
        let s_d = sel.s[d_dim];

        self.world.edit_sel(sel, |dest_node, pos| {
            let dx = (pos[r_dim] - sel.o[r_dim]) / sel.grid;
            let dy = (pos[c_dim] - sel.o[c_dim]) / sel.grid;

            // Sauerbraten maps the Z-axis backwards in the array when dimcoord(orient) != 0
            let dc = orient & 1;
            let pos_d = (pos[d_dim] - sel.o[d_dim]) / sel.grid;
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

    pub fn handle_paste(&mut self, packet: &channel1::PasteClient, payload: &sauer_proto::data::ClipboardPayload) {
        let mut paste_sel = packet.sel.clone();
        paste_sel.s = payload.sel.s;
        paste_sel.orient = payload.sel.orient;
        self.apply_block(&paste_sel, payload.nodes.clone(), true);
    }

    pub fn handle_undo_redo(&mut self, payload: &sauer_proto::data::UndoPayload) {
        self.apply_block(&payload.sel, payload.nodes.clone(), false);
    }

    pub fn handle_edit_v_slot(&mut self, packet: &channel1::EditVSlot) {
        if packet.extra_data.is_empty() {
            return;
        }
        self.world.apply_vslots(
            &packet.sel,
            packet.all_faces,
            packet.delta != 0,
            &packet.extra_data,
            &mut self.meta.vslots,
        );
        self.invalidate_cache();
    }
}

#[derive(Clone)]
pub struct Trap {
    pub button_pos: [i32; 3],
    pub button_size: i32,
    pub button_orient: u8,

    pub trigger_payload: Option<sauer_proto::data::ClipboardPayload>,

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
    pub reset_payload: Option<sauer_proto::data::ClipboardPayload>,
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
    pub trap_edit_particle_ent: Option<usize>,
    pub active_traps: Vec<crate::trap::ActiveTrap>,
    pub client_vslot_bloat: dashmap::DashMap<u64, usize>,
}

impl Default for DeathrunState {
    fn default() -> Self {
        Self {
            map: None,
            waiting_for_map: false,
            traps: Vec::new(),
            trap_edit: None,
            trap_edit_particle_ent: None,
            active_traps: Vec::new(),
            client_vslot_bloat: dashmap::DashMap::new(),
        }
    }
}

pub fn track_vslot_bloat(server: &std::sync::Arc<Server>, bytes: usize) {
    let state = get_server_state(server).read().unwrap();
    let mut to_resend = Vec::new();
    for mut entry in state.client_vslot_bloat.iter_mut() {
        let id = *entry.key();
        let new_size = *entry + bytes;
        *entry = new_size;
        if new_size > 3_000_000 {
            *entry = 0;
            to_resend.push(id);
        }
    }
    drop(state);

    for id in to_resend {
        crate::ui::send_warn(
            server,
            id,
            "\x0C3[INFO] Automatically reloading map to clear video memory bloat...",
        );
        crate::map_state::send_map_to_async(server, id);
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

pub(crate) fn send_map_to_async(server: &std::sync::Arc<Server>, id: u64) {
    let server_clone = std::sync::Arc::clone(server);
    tokio::spawn(async move {
        let state_opt = {
            let lock = get_server_state(&server_clone).read().unwrap();
            lock.map.clone()
        };

        if let Some(state) = state_opt {
            let out_bytes = tokio::task::spawn_blocking(move || {
                state.to_ogz_bytes()
            }).await.expect("to_ogz_bytes panicked");

            match out_bytes {
                Ok(bytes) => {
                    server_clone.send_chan2(id, &channel2::SendMap { data: bytes });
                }
                Err(e) => {
                    error!("failed to serialize map state: {e}");
                }
            }
        }
    });
}
