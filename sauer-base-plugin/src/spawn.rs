use rand::random;
use sauer_proto::data::{ArmourType, ClientState, GameMode, Gun, PlayerAmmo};

use crate::{
    items::tactics_ammo,
    state::{GameState, Player},
};

// ── Spawn state per mode ──────────────────────────────────────────────────────

/// Reset a player's combat state for a new life, applying mode-specific gear.
///
/// Sets `state = Alive` and increments `life_sequence`.
pub(crate) fn apply_spawn(mode: GameMode, player: &mut Player) {
    player.state = ClientState::Alive;
    player.life_sequence = (player.life_sequence + 1) & 127;
    player.health = 100;
    player.max_health = 100;
    player.armour = 0;
    player.armour_type = ArmourType::Blue;

    match mode_category(mode) {
        ModeCategory::Instagib => spawn_instagib(player),
        ModeCategory::Efficiency => spawn_efficiency(player),
        ModeCategory::Tactics => spawn_tactics(player),
        ModeCategory::Normal => spawn_ffa(player),
    }
}

fn spawn_ffa(p: &mut Player) {
    p.gun = Gun::Pistol;
    p.ammo = PlayerAmmo {
        shotgun: 0,
        chaingun: 0,
        rocket_launcher: 0,
        rifle: 0,
        grenade_launcher: 0,
        pistol: 40,
    };
}

fn spawn_instagib(p: &mut Player) {
    p.health = 1; // one hit = instant kill
    p.gun = Gun::Rifle;
    p.ammo = PlayerAmmo {
        shotgun: 0,
        chaingun: 0,
        rocket_launcher: 0,
        rifle: 1,
        grenade_launcher: 0,
        pistol: 0,
    };
}

fn spawn_efficiency(p: &mut Player) {
    // Start with all weapons, generous ammo.
    p.armour = 200;
    p.armour_type = ArmourType::Yellow;
    p.gun = Gun::Cg;
    p.ammo = PlayerAmmo {
        shotgun: 20,
        chaingun: 60,
        rocket_launcher: 10,
        rifle: 10,
        grenade_launcher: 20,
        pistol: 0,
    };
}

fn spawn_tactics(p: &mut Player) {
    // Pick a random weapon, different from the last spawn's weapon.
    const GUNS: &[Gun] = &[Gun::Sg, Gun::Cg, Gun::Rl, Gun::Rifle, Gun::Gl, Gun::Pistol];
    let gun = loop {
        let g = GUNS[random::<usize>() % GUNS.len()];
        if Some(g) != p.last_gun {
            break g;
        }
    };
    p.last_gun = Some(gun);
    p.gun = gun;
    p.ammo = tactics_ammo(gun);
}

// ── Teams ─────────────────────────────────────────────────────────────────────

/// Assign a team to a newly connected player.
///
/// Returns an empty string for non-team modes.
pub(crate) fn pick_team(mode: GameMode, state: &GameState) -> String {
    if !is_team_mode(mode) {
        return String::new();
    }
    let good = state.players.values().filter(|p| p.team == "good").count();
    let evil = state.players.values().filter(|p| p.team == "evil").count();
    if good <= evil { "good".to_string() } else { "evil".to_string() }
}

pub(crate) fn is_team_mode(mode: GameMode) -> bool {
    use GameMode::*;
    matches!(
        mode,
        TeamPlay
            | InstaTeam
            | EfficTeam
            | TacTeam
            | Capture
            | RegenCapture
            | Ctf
            | InstaCTF
            | Protect
            | InstaProtect
            | Hold
            | InstaHold
            | EfficCTF
            | EfficProtect
            | EfficHold
            | Collect
            | InstaCollect
            | EfficCollect
    )
}

// ── Internal ──────────────────────────────────────────────────────────────────

enum ModeCategory {
    Normal,
    Instagib,
    Efficiency,
    Tactics,
}

fn mode_category(mode: GameMode) -> ModeCategory {
    use GameMode::*;
    match mode {
        Instagib | InstaTeam | InstaCTF | InstaProtect | InstaHold | InstaCollect => {
            ModeCategory::Instagib
        }
        Efficiency | EfficTeam | EfficCTF | EfficProtect | EfficHold | EfficCollect => {
            ModeCategory::Efficiency
        }
        Tactics | TacTeam => ModeCategory::Tactics,
        _ => ModeCategory::Normal,
    }
}
