use sauer_proto::{
    channel1::{Damage, Died, HitPush},
    data::{ClientState, Gun, Vec3},
};
use sauer_server::Server;

use super::get_players;

pub fn gun_damage_per_ray(gun: Gun) -> i32 {
    match gun {
        Gun::Fist => 50,
        Gun::Sg => 10,
        Gun::Cg => 30,
        Gun::Rl => 120,
        Gun::Rifle => 100,
        Gun::Gl => 90,
        Gun::Pistol => 35,
    }
}

fn gun_exprad(gun: Gun) -> f32 {
    match gun {
        Gun::Rl => 40.0,
        Gun::Gl => 45.0,
        _ => 0.0,
    }
}

const EXP_DISTSCALE: f32 = 1.5;
const EXP_SELFDAMDIV: i32 = 2;

/// Damage dealt by an explosion at `dist` from the centre.
/// `is_self` halves the damage (EXP_SELFDAMDIV).
pub fn explosion_damage(gun: Gun, dist: f32, is_self: bool) -> i32 {
    let base = gun_damage_per_ray(gun) as f32;
    let exprad = gun_exprad(gun);
    let damage = if exprad > 0.0 {
        (base * (1.0 - dist / EXP_DISTSCALE / exprad)) as i32
    } else {
        base as i32
    };
    let damage = damage.max(0);
    if is_self { damage / EXP_SELFDAMDIV } else { damage }
}

/// Apply `damage` from `actor_cn` to `target_cn`. Handles armour absorption,
/// broadcasts N_DAMAGE and N_HITPUSH, and kills the target if health drops to 0.
pub fn apply_damage(
    server: &Server,
    target_cn: u32,
    actor_cn: u32,
    gun: Gun,
    damage: i32,
    dir: Vec3<f32>,
) {
    let (health, armour, killed) = {
        let mut p = match get_players(server).try_get_mut(target_cn) {
            Some(p) => p,
            None => return,
        };
        if p.client_state != ClientState::Alive {
            return;
        }
        // Nested block so the &mut PlayerState borrow drops before we write client_state
        let (health, armour, killed) = {
            let state = match p.state.as_mut() {
                Some(s) => s,
                None => return,
            };
            // Armour absorbs: blue=25%, green=50%, yellow=75%
            let absorbed =
                ((damage * (state.armour_type as i32 + 1) * 25) / 100).min(state.armour);
            state.armour -= absorbed;
            state.health -= damage - absorbed;
            (state.health, state.armour, state.health <= 0)
        };
        if killed {
            p.client_state = ClientState::Dead;
        }
        (health, armour, killed)
    };

    server.broadcast_chan1(&Damage {
        target_cn: target_cn as i32,
        actor_cn: actor_cn as i32,
        damage,
        armour,
        health,
    });
    server.broadcast_chan1(&HitPush {
        target_cn: target_cn as i32,
        gun,
        damage,
        dir,
    });

    if killed {
        do_kill(server, target_cn, actor_cn);
    }
}

/// Kill a player by suicide (frags -=1).
pub fn kill_player(server: &Server, cn: u32) {
    {
        let mut p = match get_players(server).try_get_mut(cn) {
            Some(p) => p,
            None => return,
        };
        if p.client_state != ClientState::Alive {
            return;
        }
        p.client_state = ClientState::Dead;
    }
    do_kill(server, cn, cn);
}

fn do_kill(server: &Server, victim_cn: u32, actor_cn: u32) {
    let actor_frags = if actor_cn != victim_cn {
        get_players(server)
            .try_get_mut(actor_cn)
            .map(|mut p| {
                p.frags += 1;
                p.frags
            })
            .unwrap_or(0)
    } else {
        // suicide: costs a frag
        get_players(server)
            .try_get_mut(victim_cn)
            .map(|mut p| {
                p.frags -= 1;
                p.frags
            })
            .unwrap_or(0)
    };

    if let Some(mut p) = get_players(server).try_get_mut(victim_cn) {
        p.deaths += 1;
    }

    server.broadcast_chan1(&Died {
        victim_cn: victim_cn as i32,
        actor_cn: actor_cn as i32,
        actor_frags,
        team_frags: 0,
    });
}
