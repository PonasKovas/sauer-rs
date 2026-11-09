use crate::Map;
use sauer_proto::channel1::EditEnt;
use sauer_proto::data::{EntityType, Vec3};
use std::collections::{HashMap, HashSet};

#[derive(Debug, Clone, PartialEq)]
pub struct FakeEnt {
    pub ent_type: EntityType,
    pub position: [f32; 3],
    pub attrs: [i32; 5],
}

#[derive(Debug, Clone, Default)]
pub struct VirtEntUpdates {
    pub global_broadcast: Vec<EditEnt>,
    pub personal: Vec<EditEnt>,
}

#[derive(Debug, Clone, Default)]
pub struct NamespaceState {
    pub ents: Vec<FakeEnt>,
    pub pool_indices: Vec<usize>,
}

#[derive(Debug, Clone, Default)]
pub struct PlayerVirtState {
    pub namespaces: HashMap<String, NamespaceState>,
}

#[derive(Debug, Clone, Default)]
pub struct VirtEntState {
    pub global_pool: Vec<usize>,
    pub players: HashMap<u64, PlayerVirtState>,
}

impl Map {
    /// Reconciles the physical entity list with the virtual entity pool.
    /// This ensures that every virtual entity ID managed by the global pool is backed by
    /// a genuine, reserved ET_BOX entity at `[0.0, 0.0, 0.0]` in the shared map state.
    /// This guarantees that when map states are cloned, restored, or networked, the virtual
    /// anchors remain stable and perfectly synchronized for both the server and all clients.
    pub fn reconcile_virtual_entities(&mut self) {
        for &id in &self.virt_state.global_pool {
            if id >= self.entities.len() {
                self.entities.resize_with(id + 1, || ogz_rs::data::Entity {
                    position: [0.0; 3],
                    attrs: [0; 5],
                    ent_type: ogz_rs::data::EntityType::Empty,
                    extra_data: Vec::new(),
                });
            }

            let ent = &mut self.entities[id];
            if ent.ent_type == ogz_rs::data::EntityType::Empty {
                ent.ent_type = ogz_rs::data::EntityType::Box;
                ent.position = [0.0; 3];
                ent.attrs = [0; 5];
            }
        }
    }

    /// Safely assigns virtual entities for a player under a specific namespace (e.g. "highlight", "progress").
    /// Uses stable ID allocations to prevent unrelated namespaces from shifting and flooding network updates.
    pub fn update_virt_ents(
        &mut self,
        player_id: u64,
        namespace: &str,
        new_ents: Vec<FakeEnt>,
    ) -> VirtEntUpdates {
        let mut updates = VirtEntUpdates::default();

        let mut player_state = self
            .virt_state
            .players
            .remove(&player_id)
            .unwrap_or_default();
        let mut ns_state = player_state
            .namespaces
            .remove(namespace)
            .unwrap_or_default();

        // 1. Reclaim excess pool indices
        while ns_state.pool_indices.len() > new_ents.len() {
            let pool_idx = ns_state.pool_indices.pop().unwrap();
            let ent_id = self.virt_state.global_pool[pool_idx] as i32;
            updates.personal.push(EditEnt {
                ent_id,
                pos: Vec3 {
                    x: 0.0,
                    y: 0.0,
                    z: 0.0,
                },
                type_: EntityType::Box,
                attrs: [0; 5],
            });
        }

        // 2. Allocate new pool indices if needed
        if ns_state.pool_indices.len() < new_ents.len() {
            let mut used_indices = HashSet::new();
            for ns in player_state.namespaces.values() {
                used_indices.extend(ns.pool_indices.iter().copied());
            }
            used_indices.extend(ns_state.pool_indices.iter().copied());

            while ns_state.pool_indices.len() < new_ents.len() {
                let mut found_idx = None;
                for i in 0..self.virt_state.global_pool.len() {
                    if !used_indices.contains(&i) {
                        found_idx = Some(i);
                        break;
                    }
                }

                let pool_idx = if let Some(idx) = found_idx {
                    idx
                } else {
                    // Always a brand new slot at the end. An empty slot may be
                    // empty only for now (e.g. an entity a trap removes while
                    // active), and taking it would make a real entity virtual
                    // once the slot is filled again.
                    let empty_id = self.entities.len();
                    self.entities.push(ogz_rs::data::Entity {
                        position: [0.0; 3],
                        attrs: [0; 5],
                        ent_type: ogz_rs::data::EntityType::Box,
                        extra_data: vec![],
                    });

                    let idx = self.virt_state.global_pool.len();
                    self.virt_state.global_pool.push(empty_id);

                    updates.global_broadcast.push(EditEnt {
                        ent_id: empty_id as i32,
                        pos: Vec3 {
                            x: 0.0,
                            y: 0.0,
                            z: 0.0,
                        },
                        type_: EntityType::Box,
                        attrs: [0; 5],
                    });
                    idx
                };

                ns_state.pool_indices.push(pool_idx);
                used_indices.insert(pool_idx);
            }
        }

        // 3. Diff and update
        for i in 0..new_ents.len() {
            if ns_state.ents.get(i) != Some(&new_ents[i]) {
                let pool_idx = ns_state.pool_indices[i];
                let ent_id = self.virt_state.global_pool[pool_idx] as i32;
                updates.personal.push(EditEnt {
                    ent_id,
                    pos: Vec3 {
                        x: new_ents[i].position[0],
                        y: new_ents[i].position[1],
                        z: new_ents[i].position[2],
                    },
                    type_: new_ents[i].ent_type,
                    attrs: new_ents[i].attrs,
                });
            }
        }

        ns_state.ents = new_ents;

        if !ns_state.ents.is_empty() {
            player_state
                .namespaces
                .insert(namespace.to_string(), ns_state);
        }
        if !player_state.namespaces.is_empty() {
            self.virt_state.players.insert(player_id, player_state);
        }

        updates
    }

    /// What `player_id` should currently see in entity slot `ent_id`, or `None`
    /// if that slot is not a virtual entity. Used to undo a player grabbing one.
    pub fn virt_ent_view(&self, player_id: u64, ent_id: usize) -> Option<EditEnt> {
        let pool_idx = self
            .virt_state
            .global_pool
            .iter()
            .position(|&e| e == ent_id)?;
        let shown = self.virt_state.players.get(&player_id).and_then(|p| {
            p.namespaces.values().find_map(|ns| {
                let i = ns.pool_indices.iter().position(|&k| k == pool_idx)?;
                ns.ents.get(i)
            })
        });
        Some(match shown {
            Some(fake) => EditEnt {
                ent_id: ent_id as i32,
                pos: Vec3 {
                    x: fake.position[0],
                    y: fake.position[1],
                    z: fake.position[2],
                },
                type_: fake.ent_type,
                attrs: fake.attrs,
            },
            None => EditEnt {
                ent_id: ent_id as i32,
                pos: Vec3 {
                    x: 0.0,
                    y: 0.0,
                    z: 0.0,
                },
                type_: EntityType::Box,
                attrs: [0; 5],
            },
        })
    }

    pub fn clear_virt_ents(&mut self, player_id: u64, namespace: &str) -> VirtEntUpdates {
        self.update_virt_ents(player_id, namespace, Vec::new())
    }

    pub fn clear_all_virt_ents_for_player(&mut self, player_id: u64) -> VirtEntUpdates {
        let mut updates = VirtEntUpdates::default();
        if let Some(player_state) = self.virt_state.players.remove(&player_id) {
            for ns_state in player_state.namespaces.values() {
                for &pool_idx in &ns_state.pool_indices {
                    let ent_id = self.virt_state.global_pool[pool_idx] as i32;
                    updates.personal.push(EditEnt {
                        ent_id,
                        pos: Vec3 {
                            x: 0.0,
                            y: 0.0,
                            z: 0.0,
                        },
                        type_: EntityType::Box,
                        attrs: [0; 5],
                    });
                }
            }
        }
        updates
    }
}
