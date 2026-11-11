//! Fake entities only some players see: highlights, frames, progress bars.
//!
//! The protocol has no per-player entities, so each one borrows an entity
//! slot past the map's own. Everyone gets an inert placeholder in the slot,
//! and the players meant to see the entity get `N_EDITENT` with it.

use ogz_rs::data::Entity;
use sauer_proto::channel1::EditEnt;
use sauer_proto::data::{Cn, EntityType, Vec3};
use sauer_server::Server;
use std::collections::{HashMap, HashSet};

/// Stamped into the 5th attribute of every virtual entity, which none of the
/// entity types used for them read. Clients save virtual entities into the
/// `.ogz` like any other, so maps are cleaned of them on load.
pub const VIRT_MARK: i32 = 7331;

const PLACEHOLDER_ATTRS: [i16; 5] = [0, 0, 0, 0, VIRT_MARK as i16];

#[derive(Debug, Clone, PartialEq)]
pub struct FakeEnt {
    pub ent_type: EntityType,
    pub position: [f32; 3],
    pub attrs: [i32; 5],
}

/// What to send after a change: placeholders everyone gets, and what the
/// player sees.
#[derive(Debug, Clone, Default)]
#[must_use = "the updates have to be sent"]
pub struct VirtEntUpdates {
    pub global_broadcast: Vec<EditEnt>,
    pub personal: Vec<EditEnt>,
}

impl VirtEntUpdates {
    /// Sends the updates, as edits by `from`, to `id` and everyone.
    pub fn send(self, server: &Server, id: u64, from: Cn) {
        let wrap = |pkts: Vec<EditEnt>| {
            pkts.into_iter()
                .map(|packet| sauer_proto::Client {
                    client_num: from,
                    packet,
                })
                .collect::<Vec<_>>()
        };
        if !self.global_broadcast.is_empty() {
            server.broadcast(wrap(self.global_broadcast));
        }
        if !self.personal.is_empty() {
            server.send(id, wrap(self.personal));
        }
    }
}

#[derive(Debug, Clone, Default)]
struct Namespace {
    ents: Vec<FakeEnt>,
    /// Indexes into `VirtEnts::pool`, one per entity.
    pool_indices: Vec<usize>,
}

/// Which entity slots are virtual, and what each player sees in them,
/// grouped by namespace so unrelated sets don't disturb each other.
#[derive(Debug, Clone, Default)]
pub struct VirtEnts {
    /// Entity slots taken for virtual entities.
    pool: Vec<usize>,
    players: HashMap<u64, HashMap<String, Namespace>>,
}

/// What an unused virtual slot holds: an inert box at the origin.
fn placeholder(ent_id: usize) -> EditEnt {
    EditEnt {
        ent_id: ent_id as i32,
        pos: Vec3 {
            x: 0.0,
            y: 0.0,
            z: 0.0,
        },
        type_: EntityType::Box,
        attrs: PLACEHOLDER_ATTRS.map(i32::from),
    }
}

fn shown(ent_id: usize, fake: &FakeEnt) -> EditEnt {
    EditEnt {
        ent_id: ent_id as i32,
        pos: Vec3 {
            x: fake.position[0],
            y: fake.position[1],
            z: fake.position[2],
        },
        type_: fake.ent_type,
        attrs: fake.attrs,
    }
}

fn placeholder_entity() -> Entity {
    Entity {
        position: [0.0; 3],
        attrs: PLACEHOLDER_ATTRS,
        ent_type: EntityType::Box,
        extra_data: Vec::new(),
    }
}

impl VirtEnts {
    /// Whether entity slot `ent_id` is a virtual one.
    pub fn is_virtual(&self, ent_id: usize) -> bool {
        self.pool.contains(&ent_id)
    }

    /// The entity slots taken for virtual entities.
    pub fn slots(&self) -> &[usize] {
        &self.pool
    }

    /// Puts placeholders back in virtual slots the map lost (after undo,
    /// or a map copy taken before the slot was).
    pub fn reconcile(&self, entities: &mut Vec<Entity>) {
        for &id in &self.pool {
            if id >= entities.len() {
                entities.resize_with(id + 1, || Entity {
                    ent_type: EntityType::Empty,
                    attrs: [0; 5],
                    ..placeholder_entity()
                });
            }
            if entities[id].ent_type == EntityType::Empty {
                entities[id] = placeholder_entity();
            }
        }
    }

    /// Shows `player` the entities `ents` under `namespace`, replacing what
    /// that namespace showed them before. Slots stay with their entity, so
    /// changing one namespace doesn't resend the others.
    pub fn update(
        &mut self,
        entities: &mut Vec<Entity>,
        player: u64,
        namespace: &str,
        mut ents: Vec<FakeEnt>,
    ) -> VirtEntUpdates {
        for e in &mut ents {
            e.attrs[4] = VIRT_MARK;
        }
        let mut updates = VirtEntUpdates::default();
        let mut spaces = self.players.remove(&player).unwrap_or_default();
        let mut ns = spaces.remove(namespace).unwrap_or_default();

        // Hand back slots no longer needed.
        while ns.pool_indices.len() > ents.len() {
            let pool_idx = ns.pool_indices.pop().unwrap();
            updates.personal.push(placeholder(self.pool[pool_idx]));
        }

        // Take more if needed: first ones this player isn't using, then new ones.
        if ns.pool_indices.len() < ents.len() {
            let mut used: HashSet<usize> = spaces
                .values()
                .flat_map(|s| s.pool_indices.iter().copied())
                .chain(ns.pool_indices.iter().copied())
                .collect();
            while ns.pool_indices.len() < ents.len() {
                let pool_idx = match (0..self.pool.len()).find(|i| !used.contains(i)) {
                    Some(i) => i,
                    None => {
                        // Always a brand new slot at the end. An empty slot may be
                        // empty only for now (e.g. an entity a trap removes while
                        // active), and taking it would make a real entity virtual
                        // once the slot is filled again.
                        let ent_id = entities.len();
                        entities.push(placeholder_entity());
                        self.pool.push(ent_id);
                        updates.global_broadcast.push(placeholder(ent_id));
                        self.pool.len() - 1
                    }
                };
                ns.pool_indices.push(pool_idx);
                used.insert(pool_idx);
            }
        }

        for (i, ent) in ents.iter().enumerate() {
            if ns.ents.get(i) != Some(ent) {
                updates
                    .personal
                    .push(shown(self.pool[ns.pool_indices[i]], ent));
            }
        }
        ns.ents = ents;

        if !ns.ents.is_empty() {
            spaces.insert(namespace.to_string(), ns);
        }
        if !spaces.is_empty() {
            self.players.insert(player, spaces);
        }
        updates
    }

    /// Hides what `namespace` shows `player`.
    pub fn clear(&mut self, player: u64, namespace: &str) -> VirtEntUpdates {
        let mut updates = VirtEntUpdates::default();
        if let Some(spaces) = self.players.get_mut(&player)
            && let Some(ns) = spaces.remove(namespace)
        {
            for &pool_idx in &ns.pool_indices {
                updates.personal.push(placeholder(self.pool[pool_idx]));
            }
            if spaces.is_empty() {
                self.players.remove(&player);
            }
        }
        updates
    }

    /// Hides everything `player` was shown, as when they leave.
    pub fn clear_player(&mut self, player: u64) -> VirtEntUpdates {
        let mut updates = VirtEntUpdates::default();
        for ns in self
            .players
            .remove(&player)
            .into_iter()
            .flat_map(|s| s.into_values())
        {
            for pool_idx in ns.pool_indices {
                updates.personal.push(placeholder(self.pool[pool_idx]));
            }
        }
        updates
    }

    /// What `player` should see in entity slot `ent_id`, or `None` if it's
    /// not a virtual slot. For undoing a player grabbing one.
    pub fn view(&self, player: u64, ent_id: usize) -> Option<EditEnt> {
        let pool_idx = self.pool.iter().position(|&e| e == ent_id)?;
        let fake = self.players.get(&player).and_then(|spaces| {
            spaces.values().find_map(|ns| {
                let i = ns.pool_indices.iter().position(|&k| k == pool_idx)?;
                ns.ents.get(i)
            })
        });
        Some(match fake {
            Some(fake) => shown(ent_id, fake),
            None => placeholder(ent_id),
        })
    }
}

/// Empties virtual entities a client saved into a map: anything marked with
/// [`VIRT_MARK`], plus the unmarked origin boxes older servers used as
/// placeholders. Returns how many were removed.
pub fn strip(entities: &mut [Entity]) -> usize {
    let mut removed = 0;
    for ent in entities {
        let marked = ent.attrs[4] == VIRT_MARK as i16
            && matches!(ent.ent_type, EntityType::Particles | EntityType::Box);
        let old_placeholder =
            ent.ent_type == EntityType::Box && ent.position == [0.0; 3] && ent.attrs == [0; 5];
        if marked || old_placeholder {
            ent.ent_type = EntityType::Empty;
            ent.attrs = [0; 5];
            removed += 1;
        }
    }
    removed
}
