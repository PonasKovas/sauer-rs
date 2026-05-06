use crate::map_state::{Trap, get_server_state};
use sauer_proto::Client;
use sauer_server::Server;
use serde::{Deserialize, Serialize};

#[derive(Serialize, Deserialize, Clone)]
pub struct StoredEntity {
    pub position: [f32; 3],
    pub attrs: [i16; 5],
    pub ent_type: u8,
    pub extra_data: Vec<u8>,
}

impl From<&ogz_rs::data::Entity> for StoredEntity {
    fn from(ent: &ogz_rs::data::Entity) -> Self {
        Self {
            position: ent.position,
            attrs: ent.attrs,
            ent_type: i32::from(ent.ent_type) as u8,
            extra_data: ent.extra_data.clone(),
        }
    }
}

impl StoredEntity {
    pub fn into_entity(self) -> ogz_rs::data::Entity {
        ogz_rs::data::Entity {
            position: self.position,
            attrs: self.attrs,
            ent_type: ogz_rs::data::EntityType::try_from(self.ent_type as i32)
                .unwrap_or(ogz_rs::data::EntityType::Empty),
            extra_data: self.extra_data,
        }
    }
}

#[derive(Serialize, Deserialize, Clone)]
pub struct StoredTrap {
    pub button_pos: [i32; 3],
    pub button_size: i32,
    pub button_orient: u8,
    pub trigger_clipboard: Vec<u8>,
    pub trigger_unpack_len: i32,
    pub trigger_cells: Vec<([i32; 3], i32, Vec<u8>)>,
    pub trigger_entities_mod: Vec<(usize, StoredEntity)>,
    pub trigger_entities_add: Vec<StoredEntity>,
    pub clip_pos: [i32; 3],
    pub clip_s: [i32; 3],
    pub clip_grid: i32,
    pub time: u32,
    pub timeout: u32,
    /// Base-state geometry — always applied on load to guarantee the map starts
    /// in a clean (pre-trigger) state regardless of whether a trap was active
    /// when the map was saved.
    pub reset_clipboard: Vec<u8>,
    pub reset_unpack_len: i32,
    pub reset_cells: Vec<([i32; 3], i32, Vec<u8>)>,
    pub reset_entities_mod: Vec<(usize, StoredEntity)>,
}

impl StoredTrap {
    pub fn from_trap(t: &Trap) -> Self {
        let pack_cells = |cells: &Vec<([i32; 3], i32, sauer_map::Node)>| {
            cells
                .iter()
                .map(|(pos, size, node)| {
                    let mut packed = Vec::new();
                    sauer_map::clipboard::pack_cube(node, &mut packed);
                    (*pos, *size, packed)
                })
                .collect::<Vec<_>>()
        };
        Self {
            button_pos: t.button_pos,
            button_size: t.button_size,
            button_orient: t.button_orient,
            trigger_clipboard: t.trigger_clipboard.clone(),
            trigger_unpack_len: t.trigger_unpack_len,
            trigger_cells: pack_cells(&t.trigger_cells),
            trigger_entities_mod: t
                .trigger_entities_mod
                .iter()
                .map(|(id, e)| (*id, StoredEntity::from(e)))
                .collect(),
            trigger_entities_add: t
                .trigger_entities_add
                .iter()
                .map(|e| StoredEntity::from(e))
                .collect(),
            clip_pos: t.clip_pos,
            clip_s: t.clip_s,
            clip_grid: t.clip_grid,
            time: t.time,
            timeout: t.timeout,
            reset_clipboard: t.reset_clipboard.clone(),
            reset_unpack_len: t.reset_unpack_len,
            reset_cells: pack_cells(&t.reset_cells),
            reset_entities_mod: t
                .reset_entities_mod
                .iter()
                .map(|(id, e)| (*id, StoredEntity::from(e)))
                .collect(),
        }
    }

    pub fn into_trap(self) -> Option<Trap> {
        let unpack_cells =
            |packed_cells: Vec<([i32; 3], i32, Vec<u8>)>|
             -> Option<Vec<([i32; 3], i32, sauer_map::Node)>> {
                let mut out = Vec::new();
                for (pos, size, packed) in packed_cells {
                    let mut ptr = 0;
                    let node = sauer_map::clipboard::unpack_cube(&packed, &mut ptr)?;
                    out.push((pos, size, node));
                }
                Some(out)
            };
        Some(Trap {
            button_pos: self.button_pos,
            button_size: self.button_size,
            button_orient: self.button_orient,
            trigger_clipboard: self.trigger_clipboard,
            trigger_unpack_len: self.trigger_unpack_len,
            trigger_cells: unpack_cells(self.trigger_cells)?,
            trigger_entities_mod: self
                .trigger_entities_mod
                .into_iter()
                .map(|(id, e)| (id, e.into_entity()))
                .collect(),
            trigger_entities_add: self
                .trigger_entities_add
                .into_iter()
                .map(|e| e.into_entity())
                .collect(),
            clip_pos: self.clip_pos,
            clip_s: self.clip_s,
            clip_grid: self.clip_grid,
            time: self.time,
            timeout: self.timeout,
            reset_clipboard: self.reset_clipboard,
            reset_unpack_len: self.reset_unpack_len,
            reset_cells: unpack_cells(self.reset_cells).unwrap_or_default(),
            reset_entities_mod: self
                .reset_entities_mod
                .into_iter()
                .map(|(id, e)| (id, e.into_entity()))
                .collect(),
            is_active: false,
        })
    }

    /// Apply the stored reset state directly to `world`, restoring the base
    /// (pre-trigger) geometry and entity values. Safe to call even when the
    /// trap was not active — the reset state equals the base state, so the
    /// operation is idempotent.
    pub fn apply_reset_to_world(&self, world: &mut sauer_map::World) {
        for (pos, size, packed) in &self.reset_cells {
            let mut ptr = 0;
            if let Some(node) = sauer_map::clipboard::unpack_cube(packed, &mut ptr) {
                world.set_node_at(*pos, *size, node);
            }
        }
        for (id, stored_ent) in &self.reset_entities_mod {
            if *id < world.entities.len() {
                world.entities[*id] = stored_ent.clone().into_entity();
            }
        }
        for added_ent in &self.trigger_entities_add {
            let ent = added_ent.clone().into_entity();
            if let Some(idx) = world.entities.iter().position(|e| {
                e.ent_type == ent.ent_type && e.position == ent.position && e.attrs == ent.attrs
            }) {
                world.entities[idx] = ogz_rs::data::Entity {
                    position: [0.0; 3],
                    attrs: [0; 5],
                    ent_type: ogz_rs::data::EntityType::Empty,
                    extra_data: vec![],
                };
            }
        }
    }
}

pub fn generate_trap_vslot_packets(trap: &StoredTrap) -> Vec<sauer_proto::channel1::EditVSlot> {
    let sel = crate::trap::placement::get_payload_selection(trap.button_pos, trap.button_orient);
    let mut packets = Vec::new();

    let data_blocks = crate::trap::vslot_store::encode_payload(
        trap,
        crate::trap::TRAP_STORE_VERSION,
        crate::trap::TRAP_DOMAIN,
    )
    .unwrap_or_default();

    for block in data_blocks {
        packets.push(sauer_proto::channel1::EditVSlot {
            sel: sel.clone(),
            delta: 1,
            all_faces: false,
            extra_data: block,
        });
    }

    packets
}

pub fn sync_traps_to_map(server: &std::sync::Arc<Server>, _owner_id: u64) {
    let mut state = get_server_state(server).write().unwrap();
    let owner_cn = crate::SERVER_BOT_CN as i32;
    let stored_traps: Vec<StoredTrap> = state.traps.iter().map(StoredTrap::from_trap).collect();

    let mut packets_to_send = Vec::new();

    if let Some(map) = state.map.as_mut() {
        for trap in stored_traps {
            let packets = generate_trap_vslot_packets(&trap);
            for pkt in packets {
                map.handle_edit_v_slot(&pkt);
                packets_to_send.push(pkt);
            }
        }
    }

    drop(state); // Drop write lock before acquiring read lock in tracker

    let mut total_bytes = 0;
    for pkt in &packets_to_send {
        total_bytes += pkt.extra_data.len();
    }

    if total_bytes > 0 {
        crate::map_state::track_vslot_bloat(server, total_bytes);
        for pkt in packets_to_send {
            server.broadcast_chan1(&Client {
                client_num: owner_cn,
                packet: pkt,
            });
        }
    }
}
