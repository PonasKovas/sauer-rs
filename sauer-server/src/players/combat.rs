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
                .try_get(actor_id)
                .map_or(1, |p| if p.quad_millis > 0 { 4 } else { 1 });

        let Some(mut p) = self.players.try_get_mut(target_id) else {
            return false;
        };
        if p.client_state != ClientState::Alive {
            return false;
        }
        // Inner block: drops the &mut PlayerState borrow before we modify client_state.
        let (health, armour, killed) = {
            let Some(state) = p.state.as_mut() else {
                return false;
            };
            // Armour absorbs: blue=25%, green=50%, yellow=75%
            let absorbed = ((damage * (state.armour_type as i32 + 1) * 25) / 100).min(state.armour);
            state.armour -= absorbed;
            state.health -= damage - absorbed;
            (state.health, state.armour, state.health <= 0)
        };
        if killed {
            p.client_state = ClientState::Dead;
        }
        drop(p);

        if let (Some(t_cn), Some(a_cn)) = (self.get_cn(target_id), self.get_cn(actor_id)) {
            self.broadcast_chan1(&Damage {
                target_cn: t_cn as i32,
                actor_cn: a_cn as i32,
                damage,
                armour,
                health,
            });
            self.send_chan1(
                target_id,
                &HitPush {
                    target_cn: t_cn as i32,
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
        let Some(mut p) = self.players.try_get_mut(id) else {
            return false;
        };
        if p.client_state != ClientState::Alive {
            return false;
        }
        p.client_state = ClientState::Dead;
        drop(p);
        self.do_kill(id, id);
        true
    }

    fn do_kill(&self, victim_id: u64, actor_id: u64) {
        let actor_frags = if actor_id != victim_id {
            self.players
                .try_get_mut(actor_id)
                .map(|mut p| {
                    p.frags += 1;
                    p.frags
                })
                .unwrap_or(0)
        } else {
            // suicide: costs a frag
            self.players
                .try_get_mut(victim_id)
                .map(|mut p| {
                    p.frags -= 1;
                    p.frags
                })
                .unwrap_or(0)
        };

        if let Some(mut p) = self.players.try_get_mut(victim_id) {
            p.deaths += 1;
        }

        if let (Some(v_cn), Some(a_cn)) = (self.get_cn(victim_id), self.get_cn(actor_id)) {
            self.broadcast_chan1(&Died {
                victim_cn: v_cn as i32,
                actor_cn: a_cn as i32,
                actor_frags,
                team_frags: 0,
            });
        }
    }
}
