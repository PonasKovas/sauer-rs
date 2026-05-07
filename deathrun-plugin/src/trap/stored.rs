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
    pub trigger_payload: Option<sauer_proto::data::ClipboardPayload>,
    pub trigger_entities: Vec<StoredEntity>,
    pub clip_pos: [i32; 3],
    pub clip_s: [i32; 3],
    pub clip_grid: i32,
    pub time: u32,
    pub timeout: u32,
}

impl StoredTrap {
    pub fn from_trap(t: &Trap) -> Self {
        Self {
            button_pos: t.button_pos,
            button_size: t.button_size,
            button_orient: t.button_orient,
            trigger_payload: t.trigger_payload.clone(),
            trigger_entities: t.trigger_entities.iter().map(StoredEntity::from).collect(),
            clip_pos: t.clip_pos,
            clip_s: t.clip_s,
            clip_grid: t.clip_grid,
            time: t.time,
            timeout: t.timeout,
        }
    }

    pub fn into_trap(self) -> Option<Trap> {
        Some(Trap {
            button_pos: self.button_pos,
            button_size: self.button_size,
            button_orient: self.button_orient,
            trigger_payload: self.trigger_payload,
            trigger_entities: self.trigger_entities.into_iter().map(|e| e.into_entity()).collect(),
            clip_pos: self.clip_pos,
            clip_s: self.clip_s,
            clip_grid: self.clip_grid,
            time: self.time,
            timeout: self.timeout,
            is_active: false,
        })
    }

    /// Safely synchronizes all stored VSlot definitions back to the live server map
    /// and resolves any index shifts caused by engine garbage collection.
    pub fn remap_vslots(&mut self, map: &mut sauer_map::Map) {
        let mut remapping = std::collections::HashMap::new();

        if let Some(payload) = &self.trigger_payload {
            for vslot_data in &payload.vslots {
                let ds = sauer_proto::data::VSlotDelta {
                    params: vslot_data.vslot.params.iter().map(|p| sauer_proto::data::VSlotDeltaParam {
                        name: p.name.clone(),
                        values: p.values,
                    }).collect(),
                    scale: vslot_data.vslot.scale,
                    rotation: vslot_data.vslot.rotation,
                    offset: vslot_data.vslot.offset,
                    scroll: vslot_data.vslot.scroll,
                    layer: vslot_data.vslot.layer,
                    alpha: vslot_data.vslot.alpha,
                    color: vslot_data.vslot.color,
                };
                let new_idx = sauer_map::vslot::resolve_vslot(&mut map.vslots, vslot_data.slot, &ds, false);
                remapping.insert(vslot_data.index, new_idx);
            }
        }

        if remapping.is_empty() { return; }

        fn remap_node_textures(node: &mut sauer_map::Node, remap: &std::collections::HashMap<u16, u16>) {
            match node {
                sauer_map::Node::Leaf(cube) => {
                    for t in &mut cube.textures {
                        if let Some(&new_idx) = remap.get(t) {
                            *t = new_idx;
                        }
                    }
                }
                sauer_map::Node::Branch(children) => {
                    for child in children.iter_mut() {
                        remap_node_textures(child, remap);
                    }
                }
            }
        }

        if let Some(payload) = &mut self.trigger_payload {
            for node in &mut payload.nodes {
                remap_node_textures(node, &remapping);
            }
        }
    }
}

pub fn generate_trap_vslot_packets(trap: &StoredTrap) -> Vec<sauer_proto::channel1::EditVSlot> {
    let sel = crate::trap::placement::get_payload_selection(trap.button_pos, trap.button_size, trap.button_orient);
    let mut packets = Vec::new();

    let data_blocks = sauer_map::Map::db_encode(trap).unwrap_or_default();

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
                map.apply_vslots(&pkt.sel, pkt.all_faces, pkt.delta != 0, &pkt.extra_data);
                packets_to_send.push(pkt);
            }
        }
    }

    drop(state); // Drop write lock before acquiring read lock in tracker

    let mut total_bytes = 0;
    for pkt in &packets_to_send {
        let delta = &pkt.extra_data;
        for p in &delta.params {
            total_bytes += 1 + p.name.len() + 1 + 16;
        }
        if delta.scale.is_some() { total_bytes += 5; }
        if delta.rotation.is_some() { total_bytes += 5; }
        if delta.offset.is_some() { total_bytes += 9; }
        if delta.scroll.is_some() { total_bytes += 9; }
        if delta.layer.is_some() { total_bytes += 5; }
        if delta.alpha.is_some() { total_bytes += 9; }
        if delta.color.is_some() { total_bytes += 13; }
        total_bytes += 1; // 0xFF
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
