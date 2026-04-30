use sauer_proto::data::{ArmourType, ItemSpawn, ItemType, PlayerState};

pub struct ServerItem {
    pub item_type: ItemType,
    pub spawned: bool,
    /// Countdown in milliseconds until this item respawns. 0 = not counting.
    pub spawn_time_ms: i32,
}

pub struct Items {
    pub list: Vec<ServerItem>,
    /// True while waiting for a client to send the initial N_ITEMLIST.
    pub notgotitems: bool,
}

impl Default for Items {
    fn default() -> Self {
        Self {
            list: Vec::new(),
            notgotitems: true,
        }
    }
}

impl Items {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn reset(&mut self) {
        self.list.clear();
        self.notgotitems = true;
    }

    pub fn spawned_items(&self) -> Vec<ItemSpawn> {
        self.list
            .iter()
            .enumerate()
            .filter(|(_, item)| item.spawned)
            .map(|(i, item)| ItemSpawn {
                item_index: i as i32,
                item_type: item.item_type,
            })
            .collect()
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
