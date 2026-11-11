use crate::Server;
use crate::Status;
use sauer_proto::{
    channel1::{Damage, Died, HitPush},
    data::{Gun, Vec3},
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
    if is_self {
        damage / EXP_SELFDAMDIV
    } else {
        damage
    }
}

impl Server {
    /// Apply `damage` from `actor_id` to `target_id`. Handles armour absorption,
    /// broadcasts N_DAMAGE and N_HITPUSH, and kills the target if health drops to 0.
    /// Returns true if the player died from this damage event.
    pub fn apply_damage(
        &self,
        target_id: u64,
        actor_id: u64,
        gun: Gun,
        damage: i32,
        dir: Vec3<f32>,
    ) -> bool {
        // Apply quad damage multiplier from the attacker.
        let damage = damage
            * self
                .players
                .with(actor_id, |p| if p.quad_millis > 0 { 4 } else { 1 })
                .unwrap_or(1);

        let hit = self.players.with_mut(target_id, |p| {
            if p.status != Status::Alive {
                return None;
            }
            let state = p.state.as_mut()?;
            // Armour absorbs: blue=25%, green=50%, yellow=75%
            let absorbed = ((damage * (state.armour_type as i32 + 1) * 25) / 100).min(state.armour);
            state.armour -= absorbed;
            state.health -= damage - absorbed;
            let (health, armour, killed) = (state.health, state.armour, state.health <= 0);
            if killed {
                p.status = Status::Dead;
            }
            Some((health, armour, killed))
        });
        let Some((health, armour, killed)) = hit.flatten() else {
            return false;
        };

        if let (Some(t_cn), Some(a_cn)) = (self.get_cn(target_id), self.get_cn(actor_id)) {
            self.broadcast_chan1(&Damage {
                target_cn: t_cn,
                actor_cn: a_cn,
                damage,
                armour,
                health,
            });
            self.send_chan1(
                target_id,
                &HitPush {
                    target_cn: t_cn,
                    gun,
                    damage,
                    dir,
                },
            );
        }

        if killed {
            self.do_kill(target_id, actor_id);
        }
        killed
    }

    /// Kill a player by suicide (frags -= 1). Returns true if successful.
    pub fn kill_player(&self, id: u64) -> bool {
        let killed = self.players.with_mut(id, |p| {
            let alive = p.status == Status::Alive;
            if alive {
                p.status = Status::Dead;
            }
            alive
        });
        if killed != Some(true) {
            return false;
        }
        self.do_kill(id, id);
        true
    }

    fn do_kill(&self, victim_id: u64, actor_id: u64) {
        // A suicide costs a frag.
        let frag_change = if actor_id != victim_id { 1 } else { -1 };
        let actor_frags = self
            .players
            .with_mut(actor_id, |p| {
                p.frags += frag_change;
                p.frags
            })
            .unwrap_or(0);
        self.players.with_mut(victim_id, |p| p.deaths += 1);

        if let (Some(v_cn), Some(a_cn)) = (self.get_cn(victim_id), self.get_cn(actor_id)) {
            self.broadcast_chan1(&Died {
                victim_cn: v_cn,
                actor_cn: a_cn,
                actor_frags,
                team_frags: 0,
            });
        }
    }
}
