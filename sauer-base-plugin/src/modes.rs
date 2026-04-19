use sauer_proto::data::{ArmourType, GameMode, Gun, PlayerAmmo, PlayerState};

// ── Mode predicates ───────────────────────────────────────────────────────────

/// True for all networked multiplayer modes (excludes SP / demo / coop-edit).
pub fn is_multiplayer(mode: GameMode) -> bool {
    !matches!(mode, GameMode::Sp | GameMode::Dmsp | GameMode::Demo | GameMode::CoopEdit)
}

/// True for modes that use the item pickup system.
pub fn has_items(mode: GameMode) -> bool {
    matches!(
        mode,
        GameMode::Ffa
            | GameMode::TeamPlay
            | GameMode::Capture
            | GameMode::RegenCapture
            | GameMode::Ctf
            | GameMode::Protect
            | GameMode::Hold
            | GameMode::Collect
    )
}

pub fn is_instagib(mode: GameMode) -> bool {
    matches!(
        mode,
        GameMode::Instagib
            | GameMode::InstaTeam
            | GameMode::InstaCTF
            | GameMode::InstaProtect
            | GameMode::InstaHold
            | GameMode::InstaCollect
    )
}

pub fn is_efficiency(mode: GameMode) -> bool {
    matches!(
        mode,
        GameMode::Efficiency
            | GameMode::EfficTeam
            | GameMode::EfficCTF
            | GameMode::EfficProtect
            | GameMode::EfficHold
            | GameMode::EfficCollect
    )
}

// ── Spawn state ───────────────────────────────────────────────────────────────

/// Default PlayerState for a newly spawned client, determined by game mode.
pub fn default_spawn_state(mode: GameMode) -> PlayerState {
    if is_instagib(mode) {
        return PlayerState {
            life_sequence: 1,
            health: 1,
            max_health: 1,
            armour: 0,
            armour_type: ArmourType::Blue,
            gun_select: Gun::Rifle,
            ammo: PlayerAmmo {
                shotgun: 0,
                chaingun: 0,
                rocket_launcher: 0,
                rifle: -1, // infinite
                grenade_launcher: 0,
                pistol: 0,
            },
        };
    }

    if is_efficiency(mode) {
        return PlayerState {
            life_sequence: 1,
            health: 100,
            max_health: 100,
            armour: 0,
            armour_type: ArmourType::Blue,
            gun_select: Gun::Fist,
            ammo: PlayerAmmo {
                shotgun: 10,
                chaingun: 20,
                rocket_launcher: 5,
                rifle: 5,
                grenade_launcher: 5,
                pistol: 10,
            },
        };
    }

    PlayerState {
        life_sequence: 1,
        health: 100,
        max_health: 100,
        armour: 0,
        armour_type: ArmourType::Blue,
        gun_select: Gun::Fist,
        ammo: PlayerAmmo {
            shotgun: 0,
            chaingun: 0,
            rocket_launcher: 0,
            rifle: 0,
            grenade_launcher: 0,
            pistol: 30,
        },
    }
}
