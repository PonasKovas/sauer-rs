use sauer_proto::data::{ArmourType, ItemType, PlayerAmmo};

use crate::state::Player;

/// How long (in milliseconds) before a picked-up item respawns.
pub(crate) fn item_respawn_ms(item_type: ItemType) -> u64 {
    match item_type {
        ItemType::Boost | ItemType::Quad => 60_000,
        _ => 15_000,
    }
}

/// Apply a pickup's effect to a player's stats.
pub(crate) fn apply_item(p: &mut Player, item_type: ItemType) {
    match item_type {
        ItemType::Health => {
            p.health = (p.health + 25).min(p.max_health);
        }
        ItemType::Boost => {
            p.max_health = (p.max_health + 10).min(200);
            p.health = (p.health + 25).min(p.max_health);
        }
        ItemType::GreenArmour => {
            // Only upgrade if not already wearing yellow armour.
            if p.armour_type != ArmourType::Yellow {
                p.armour = p.armour.max(100);
                p.armour_type = ArmourType::Green;
            }
        }
        ItemType::YellowArmour => {
            p.armour = p.armour.max(200);
            p.armour_type = ArmourType::Yellow;
        }
        ItemType::Quad => {
            // Quad damage is handled client-side; server just acknowledges the pickup.
        }
        ItemType::Shells => p.ammo.shotgun = (p.ammo.shotgun + 10).min(30),
        ItemType::Bullets => p.ammo.chaingun = (p.ammo.chaingun + 20).min(60),
        ItemType::Rockets => p.ammo.rocket_launcher = (p.ammo.rocket_launcher + 5).min(15),
        ItemType::Rounds => p.ammo.rifle = (p.ammo.rifle + 5).min(15),
        ItemType::Grenades => p.ammo.grenade_launcher = (p.ammo.grenade_launcher + 10).min(30),
        ItemType::Cartridges => p.ammo.pistol = (p.ammo.pistol + 20).min(120),
    }
}

/// Whether this game mode has pickup items on the map (weapons, health, armour).
///
/// Instagib, Efficiency, and Tactics variants do not use pickups.
pub(crate) fn mode_has_items(mode: sauer_proto::data::GameMode) -> bool {
    use sauer_proto::data::GameMode::*;
    !matches!(
        mode,
        Instagib
            | InstaTeam
            | InstaCTF
            | InstaProtect
            | InstaHold
            | InstaCollect
            | Efficiency
            | EfficTeam
            | EfficCTF
            | EfficProtect
            | EfficHold
            | EfficCollect
            | Tactics
            | TacTeam
    )
}

/// Ammo for a specific weapon when assigned by Tactics mode.
pub(crate) fn tactics_ammo(gun: sauer_proto::data::Gun) -> PlayerAmmo {
    use sauer_proto::data::Gun::*;
    let mut ammo = PlayerAmmo {
        shotgun: 0,
        chaingun: 0,
        rocket_launcher: 0,
        rifle: 0,
        grenade_launcher: 0,
        pistol: 0,
    };
    match gun {
        Sg => ammo.shotgun = 20,
        Cg => ammo.chaingun = 60,
        Rl => ammo.rocket_launcher = 10,
        Rifle => ammo.rifle = 10,
        Gl => ammo.grenade_launcher = 20,
        Pistol => ammo.pistol = 40,
        Fist => {} // fist needs no ammo
    }
    ammo
}
