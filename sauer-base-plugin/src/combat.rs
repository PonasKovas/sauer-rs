use sauer_proto::{
    Encode,
    channel1::{Damage, Died},
    data::{ArmourType, ClientState, Gun},
    route::{Chan0, Chan1, S2C},
};
use sauer_server::Server;

use crate::BasePlugin;

// ── Weapon tables ─────────────────────────────────────────────────────────────

/// Base damage for one ray/pellet of a weapon.
///
/// For hitscan weapons (N_SHOOT), multiply by `hit.rays` (shotgun sends 10 rays).
/// For splash weapons (N_EXPLODE), use as full damage and apply distance falloff.
pub(crate) fn gun_damage(gun: Gun) -> i32 {
    match gun {
        Gun::Fist => 50,
        Gun::Sg => 10,     // × rays (up to 10 pellets)
        Gun::Cg => 30,
        Gun::Rl => 120,
        Gun::Rifle => 100,
        Gun::Gl => 90,
        Gun::Pistol => 35,
    }
}

/// Blast radius for splash weapons (used for distance falloff).
pub(crate) fn gun_damrad(gun: Gun) -> f32 {
    match gun {
        Gun::Rl => 40.0,
        Gun::Gl => 9.5,
        _ => 8.0,
    }
}

/// Apply damage into armour.  Returns `(actual_damage, absorbed_by_armour)`.
pub(crate) fn absorb_armour(armour: &mut i32, armour_type: ArmourType, damage: i32) -> (i32, i32) {
    if *armour <= 0 {
        return (damage, 0);
    }
    let pct = match armour_type {
        ArmourType::Blue => 25,
        ArmourType::Green => 50,
        ArmourType::Yellow => 75,
    };
    let absorbed = (*armour).min(damage) * pct / 100;
    *armour = (*armour - absorbed).max(0);
    (damage - absorbed, absorbed)
}

// ── BasePlugin helpers ────────────────────────────────────────────────────────

impl BasePlugin {
    /// Send a channel-1 packet to every registered player, optionally skipping one.
    pub(crate) fn broadcast1<T: Encode<S2C<Chan1>>>(
        &self,
        server: &Server,
        except: Option<u32>,
        pkt: &T,
    ) {
        let state = self.state.lock().unwrap();
        for &cn in state.players.keys() {
            if Some(cn) != except {
                server.send_chan1(cn, pkt);
            }
        }
    }

    /// Send a channel-0 packet to every registered player, optionally skipping one.
    pub(crate) fn broadcast0<T: Encode<S2C<Chan0>>>(
        &self,
        server: &Server,
        except: Option<u32>,
        pkt: &T,
    ) {
        let state = self.state.lock().unwrap();
        for &cn in state.players.keys() {
            if Some(cn) != except {
                server.send_chan0(cn, pkt);
            }
        }
    }

    /// Broadcast the current spawn state for `cn` to all registered players.
    pub(crate) fn broadcast_spawn_state(&self, server: &Server, cn: u32) {
        use sauer_proto::channel1::SpawnState;
        use sauer_proto::data::PlayerState;

        let state = self.state.lock().unwrap();
        let Some(p) = state.players.get(&cn) else { return };
        let pkt = SpawnState {
            client_num: cn as i32,
            state: PlayerState {
                life_sequence: p.life_sequence,
                health: p.health,
                max_health: p.max_health,
                armour: p.armour,
                armour_type: p.armour_type,
                gun_select: p.gun,
                ammo: p.ammo.clone(),
            },
        };
        drop(state);
        self.broadcast1(server, None, &pkt);
    }

    /// Apply damage to a player and broadcast `N_DAMAGE`.  Kills if health reaches 0.
    ///
    /// Silently ignored if the victim does not exist or is not in `Alive` state
    /// (dead, spectating, or in edit mode).
    pub(crate) fn do_damage(&self, server: &Server, victim_cn: u32, actor_cn: u32, damage: i32) {
        let (final_health, died, absorbed) = {
            let mut state = self.state.lock().unwrap();
            let victim = match state.players.get_mut(&victim_cn) {
                Some(p) if p.state == ClientState::Alive => p,
                _ => return,
            };
            let (actual, absorbed) = absorb_armour(&mut victim.armour, victim.armour_type, damage);
            victim.health -= actual;
            (victim.health, victim.health <= 0, absorbed)
        };

        self.broadcast1(
            server,
            None,
            &Damage {
                target_cn: victim_cn as i32,
                actor_cn: actor_cn as i32,
                damage,
                armour: absorbed,
                health: final_health,
            },
        );

        if died {
            self.kill(server, victim_cn, actor_cn);
        }
    }

    /// Mark a player as dead, update frags, and broadcast `N_DIED`.
    pub(crate) fn kill(&self, server: &Server, victim_cn: u32, actor_cn: u32) {
        let (actor_frags, team_frags) = {
            let mut state = self.state.lock().unwrap();

            // Update victim — block so the &mut borrow is released before the next get().
            {
                let victim = match state.players.get_mut(&victim_cn) {
                    Some(p) => p,
                    None => return,
                };
                victim.state = ClientState::Dead;
                victim.life_sequence = (victim.life_sequence + 1) & 127;
                victim.deaths += 1;
            }

            // Determine team kill (same non-empty team).
            let victim_team = state.players[&victim_cn].team.clone();
            let actor_team = state.players.get(&actor_cn).map(|p| p.team.clone()).unwrap_or_default();
            let team_kill =
                victim_cn != actor_cn && !victim_team.is_empty() && victim_team == actor_team;

            // Update actor frags.
            if victim_cn != actor_cn {
                if let Some(actor) = state.players.get_mut(&actor_cn) {
                    if team_kill {
                        actor.frags -= 1;
                    } else {
                        actor.frags += 1;
                    }
                }
            }

            // Update team frags.
            if !actor_team.is_empty() {
                let tf = state.team_frags.entry(actor_team.clone()).or_insert(0);
                if team_kill {
                    *tf -= 1;
                } else {
                    *tf += 1;
                }
            }

            let actor_frags = state.players.get(&actor_cn).map_or(0, |p| p.frags);
            let team_frags = if actor_team.is_empty() {
                0
            } else {
                state.team_frags.get(&actor_team).copied().unwrap_or(0)
            };
            (actor_frags, team_frags)
        };

        self.broadcast1(
            server,
            None,
            &Died {
                victim_cn: victim_cn as i32,
                actor_cn: actor_cn as i32,
                actor_frags,
                team_frags,
            },
        );
    }
}
