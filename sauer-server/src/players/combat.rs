use crate::Server;
use sauer_proto::{
    channel1::{Damage, Died, HitPush},
    data::{ClientState, Gun, Vec3},
};

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

impl Server {
    /// Apply `damage` from `actor_cn` to `target_cn`. Handles armour absorption,
    /// broadcasts N_DAMAGE and N_HITPUSH, and kills the target if health drops to 0.
    pub fn apply_damage(
        &self,
        target_cn: u32,
        actor_cn: u32,
        gun: Gun,
        damage: i32,
        dir: Vec3<f32>,
    ) {
        // Apply quad damage multiplier from the attacker.
        let damage = damage
            * self
                .players
                .try_get(actor_cn)
                .map_or(1, |p| if p.quad_millis > 0 { 4 } else { 1 });

        let Some(mut p) = self.players.try_get_mut(target_cn) else { return };
        if p.client_state != ClientState::Alive {
            return;
        }
        // Inner block: drops the &mut PlayerState borrow before we modify client_state.
        let (health, armour, killed) = {
            let Some(state) = p.state.as_mut() else { return };
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
        drop(p);

        self.broadcast_chan1(&Damage {
            target_cn: target_cn as i32,
            actor_cn: actor_cn as i32,
            damage,
            armour,
            health,
        });
        self.send_chan1(
            target_cn,
            &HitPush { target_cn: target_cn as i32, gun, damage, dir },
        );

        if killed {
            self.do_kill(target_cn, actor_cn);
        }
    }

    /// Kill a player by suicide (frags -= 1).
    pub fn kill_player(&self, cn: u32) {
        let Some(mut p) = self.players.try_get_mut(cn) else { return };
        if p.client_state != ClientState::Alive {
            return;
        }
        p.client_state = ClientState::Dead;
        drop(p);
        self.do_kill(cn, cn);
    }

    fn do_kill(&self, victim_cn: u32, actor_cn: u32) {
        let actor_frags = if actor_cn != victim_cn {
            self.players
                .try_get_mut(actor_cn)
                .map(|mut p| {
                    p.frags += 1;
                    p.frags
                })
                .unwrap_or(0)
        } else {
            // suicide: costs a frag
            self.players
                .try_get_mut(victim_cn)
                .map(|mut p| {
                    p.frags -= 1;
                    p.frags
                })
                .unwrap_or(0)
        };

        if let Some(mut p) = self.players.try_get_mut(victim_cn) {
            p.deaths += 1;
        }

        self.broadcast_chan1(&Died {
            victim_cn: victim_cn as i32,
            actor_cn: actor_cn as i32,
            actor_frags,
            team_frags: 0,
        });
    }
}
