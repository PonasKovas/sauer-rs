use std::collections::HashMap;

use cube_string::CubeString;
use sauer_proto::data::{ArmourType, ClientState, Gun, ItemType, PlayerAmmo, PlayerModel};

// ── Game state ────────────────────────────────────────────────────────────────

pub(crate) struct GameState {
    pub players: HashMap<u32, Player>,
    pub items: Vec<ItemState>,
    /// Per-team frag totals for team modes. Empty in FFA.
    pub team_frags: HashMap<String, i32>,
}

impl GameState {
    pub fn new() -> Self {
        Self {
            players: HashMap::new(),
            items: Vec::new(),
            team_frags: HashMap::new(),
        }
    }
}

// ── Player ────────────────────────────────────────────────────────────────────

#[derive(Clone)]
pub(crate) struct Player {
    pub name: CubeString,
    pub model: PlayerModel,
    pub state: ClientState,
    pub frags: i32,
    pub deaths: i32,
    pub flags: i32,
    pub life_sequence: i32,
    pub gun: Gun,
    pub health: i32,
    pub max_health: i32,
    pub armour: i32,
    pub armour_type: ArmourType,
    pub ammo: PlayerAmmo,
    /// Team name ("good" / "evil" in team modes, empty in FFA).
    pub team: String,
    /// The gun the player had last spawn — used by Tactics mode.
    pub last_gun: Option<Gun>,
}

pub(crate) fn fresh_player(name: CubeString, model: PlayerModel) -> Player {
    Player {
        name,
        model,
        state: ClientState::Dead,
        frags: 0,
        deaths: 0,
        flags: 0,
        life_sequence: 0,
        gun: Gun::Pistol,
        health: 100,
        max_health: 100,
        armour: 0,
        armour_type: ArmourType::Blue,
        ammo: PlayerAmmo {
            shotgun: 0,
            chaingun: 0,
            rocket_launcher: 0,
            rifle: 0,
            grenade_launcher: 0,
            pistol: 40,
        },
        team: String::new(),
        last_gun: None,
    }
}

// ── Map item ──────────────────────────────────────────────────────────────────

pub(crate) struct ItemState {
    pub item_type: ItemType,
    pub taken: bool,
}
