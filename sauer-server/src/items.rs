use sauer_proto::data::{ArmourType, ItemSpawn, ItemType, PlayerState};

/// Engine `MAXENTS`: no map has more entities, so no item index is past it.
pub const MAX_ENTS: usize = 10000;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ItemState {
    Spawned,
    /// Picked up or not out yet; comes back once the countdown runs out.
    Respawning {
        ms_left: i32,
    },
}

#[derive(Clone, Copy, Debug)]
pub struct ServerItem {
    pub item_type: ItemType,
    pub state: ItemState,
}

impl ServerItem {
    /// An item as it first appears on a map: armour, boost and quad wait a
    /// while, the rest is out right away (engine `delayspawn`).
    fn fresh(item_type: ItemType, num_players: usize) -> Self {
        let state = if delay_spawn(item_type) {
            ItemState::Respawning {
                ms_left: spawntime(item_type, num_players),
            }
        } else {
            ItemState::Spawned
        };
        Self { item_type, state }
    }
}

/// The current map's items, indexed by entity id.
pub struct Items {
    /// `None` for entities that aren't items.
    slots: Vec<Option<ServerItem>>,
    /// Waiting for a client to send the map's `N_ITEMLIST`.
    awaiting_list: bool,
}

impl Default for Items {
    fn default() -> Self {
        Self {
            slots: Vec::new(),
            awaiting_list: true,
        }
    }
}

impl Items {
    pub fn new() -> Self {
        Self::default()
    }

    /// Forgets all items and waits for a client to send the new map's list.
    pub fn await_list(&mut self) {
        self.slots.clear();
        self.awaiting_list = true;
    }

    /// Forgets all items: the map is empty (engine `N_NEWMAP`).
    pub fn clear(&mut self) {
        self.slots.clear();
        self.awaiting_list = false;
    }

    pub fn is_awaiting_list(&self) -> bool {
        self.awaiting_list
    }

    /// Replaces all items with a new map's. Indexes past [`MAX_ENTS`] are dropped.
    pub fn load(&mut self, items: impl IntoIterator<Item = (usize, ItemType)>, num_players: usize) {
        self.clear();
        for (idx, item_type) in items {
            if let Some(slot) = self.slot_mut(idx) {
                *slot = Some(ServerItem::fresh(item_type, num_players));
            }
        }
    }

    /// Sets entity `idx` to `item_type`, or to no item, after a coop edit.
    /// Returns true if a new item appeared there, which goes out as spawned.
    pub fn edit(&mut self, idx: usize, item_type: Option<ItemType>) -> bool {
        let Some(slot) = self.slot_mut(idx) else {
            return false;
        };
        match item_type {
            Some(t) if !matches!(slot, Some(item) if item.item_type == t) => {
                *slot = Some(ServerItem {
                    item_type: t,
                    state: ItemState::Spawned,
                });
                true
            }
            Some(_) => false,
            None => {
                *slot = None;
                false
            }
        }
    }

    /// Takes the spawned item at `idx` if `can_take` allows its type, and
    /// starts its respawn countdown. Returns its type.
    pub fn take(
        &mut self,
        idx: usize,
        num_players: usize,
        can_take: impl FnOnce(ItemType) -> bool,
    ) -> Option<ItemType> {
        let item = self.slots.get_mut(idx)?.as_mut()?;
        if item.state != ItemState::Spawned || !can_take(item.item_type) {
            return None;
        }
        item.state = ItemState::Respawning {
            ms_left: spawntime(item.item_type, num_players),
        };
        Some(item.item_type)
    }

    /// Runs the respawn countdowns forward. Returns the items that came back.
    pub fn tick(&mut self, ms: i32) -> Vec<usize> {
        if self.awaiting_list {
            return Vec::new();
        }
        let mut spawned = Vec::new();
        for (idx, item) in self.slots.iter_mut().enumerate() {
            if let Some(item) = item
                && let ItemState::Respawning { ms_left } = &mut item.state
            {
                *ms_left -= ms;
                if *ms_left <= 0 {
                    item.state = ItemState::Spawned;
                    spawned.push(idx);
                }
            }
        }
        spawned
    }

    pub fn spawned_items(&self) -> Vec<ItemSpawn> {
        self.slots
            .iter()
            .enumerate()
            .filter_map(|(i, slot)| {
                let item = slot.as_ref()?;
                (item.state == ItemState::Spawned).then_some(ItemSpawn {
                    item_index: i as i32,
                    item_type: item.item_type,
                })
            })
            .collect()
    }

    /// The slot for entity `idx`, growing the list to reach it. `None` past [`MAX_ENTS`].
    fn slot_mut(&mut self, idx: usize) -> Option<&mut Option<ServerItem>> {
        if idx >= MAX_ENTS {
            return None;
        }
        if self.slots.len() <= idx {
            self.slots.resize(idx + 1, None);
        }
        Some(&mut self.slots[idx])
    }
}

/// Returns the respawn delay in milliseconds for an item, given the current number of players.
/// Matches the C++ `spawntime()` function.
pub fn spawntime(item_type: ItemType, num_players: usize) -> i32 {
    let np: i32 = if num_players < 3 {
        4
    } else if num_players > 4 {
        2
    } else {
        3
    };
    let sec: i32 = match item_type {
        ItemType::Shells
        | ItemType::Bullets
        | ItemType::Rockets
        | ItemType::Rounds
        | ItemType::Grenades
        | ItemType::Cartridges => np * 4,
        ItemType::Health => np * 5,
        ItemType::GreenArmour => 20,
        ItemType::YellowArmour => 30,
        ItemType::Boost => 60,
        ItemType::Quad => 70,
    };
    sec * 1000
}

/// Returns true for items that should start with a delayed spawn on map load.
/// Matches the C++ `delayspawn()` function.
pub fn delay_spawn(item_type: ItemType) -> bool {
    matches!(
        item_type,
        ItemType::GreenArmour | ItemType::YellowArmour | ItemType::Boost | ItemType::Quad
    )
}

/// Returns (add, max) ammo stats for ammo-type items.
pub fn ammo_stats(item_type: ItemType) -> (i32, i32) {
    match item_type {
        ItemType::Shells => (10, 30),
        ItemType::Bullets => (20, 60),
        ItemType::Rockets => (5, 15),
        ItemType::Rounds => (5, 15),
        ItemType::Grenades => (10, 30),
        ItemType::Cartridges => (30, 120),
        _ => (0, 0),
    }
}

pub fn ammo_current(state: &PlayerState, item_type: ItemType) -> i32 {
    match item_type {
        ItemType::Shells => state.ammo.shotgun,
        ItemType::Bullets => state.ammo.chaingun,
        ItemType::Rockets => state.ammo.rocket_launcher,
        ItemType::Rounds => state.ammo.rifle,
        ItemType::Grenades => state.ammo.grenade_launcher,
        ItemType::Cartridges => state.ammo.pistol,
        _ => 0,
    }
}

pub fn ammo_current_mut(state: &mut PlayerState, item_type: ItemType) -> &mut i32 {
    match item_type {
        ItemType::Shells => &mut state.ammo.shotgun,
        ItemType::Bullets => &mut state.ammo.chaingun,
        ItemType::Rockets => &mut state.ammo.rocket_launcher,
        ItemType::Rounds => &mut state.ammo.rifle,
        ItemType::Grenades => &mut state.ammo.grenade_launcher,
        ItemType::Cartridges => &mut state.ammo.pistol,
        _ => unreachable!("ammo_current_mut called for non-ammo item"),
    }
}

/// Mirrors C++ `fpsstate::canpickup` — returns whether the player's current
/// state allows picking up the given item.
pub fn canpickup(state: &PlayerState, quad_millis: i32, item_type: ItemType) -> bool {
    match item_type {
        ItemType::Boost => state.max_health < 200 || state.health < state.max_health,
        ItemType::Health => state.health < state.max_health,
        ItemType::GreenArmour => {
            if state.armour_type == ArmourType::Yellow && state.armour >= 100 {
                return false;
            }
            state.armour_type == ArmourType::Blue || state.armour < 100
        }
        ItemType::YellowArmour => state.armour_type == ArmourType::Blue || state.armour < 200,
        ItemType::Quad => quad_millis < 30000,
        item => ammo_current(state, item) < ammo_stats(item).1,
    }
}

/// Mirrors C++ `fpsstate::pickup` — applies the stat changes from picking up
/// the given item to the player's state and quad timer.
pub fn apply_pickup(state: &mut PlayerState, quad_millis: &mut i32, item_type: ItemType) {
    match item_type {
        ItemType::Boost => {
            state.max_health = (state.max_health + 50).min(200);
            state.health = (state.health + 100).min(state.max_health);
        }
        ItemType::Health => {
            state.health = (state.health + 25).min(state.max_health);
        }
        ItemType::GreenArmour => {
            state.armour = (state.armour + 100).min(100);
            state.armour_type = ArmourType::Green;
        }
        ItemType::YellowArmour => {
            state.armour = (state.armour + 200).min(200);
            state.armour_type = ArmourType::Yellow;
        }
        ItemType::Quad => {
            *quad_millis = (*quad_millis + 20000).min(30000);
        }
        item => {
            let (add, max) = ammo_stats(item);
            *ammo_current_mut(state, item) = (ammo_current(state, item) + add).min(max);
        }
    }
}
