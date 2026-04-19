use crate::{game::GameState, registry::ClientRegistry};
use dashmap::DashMap;
use sauer_proto::{
    channel0,
    channel1::{self, Explode, GunSelect, Shoot, Suicide},
    data::{ArmourType, ClientState, Gun, HitInfo, PlayerAmmo},
};
use sauer_server::{
    Server,
    plugin::{Control, PASS},
};
use std::sync::Arc;

// ── Gun tables ────────────────────────────────────────────────────────────────
// Indexed by Gun discriminant: Fist=0, Sg=1, Cg=2, Rl=3, Rifle=4, Gl=5, Pistol=6

/// Minimum milliseconds between shots per gun.
const GUN_ATTACK_DELAY: [u64; 7] = [600, 1400, 100, 800, 1500, 600, 500];
/// Base damage per ray.
const GUN_DAMAGE: [i32; 7] = [50, 10, 30, 120, 100, 100, 35];
/// Number of rays per shot (shotgun has 20).
const GUN_RAYS: [i32; 7] = [1, 20, 1, 1, 1, 1, 1];

fn gun_idx(gun: Gun) -> usize {
    gun as usize
}

fn is_projectile(gun: Gun) -> bool {
    matches!(gun, Gun::Rl | Gun::Gl)
}

/// Return mutable ref to the ammo slot for `gun`, or None for Fist (no ammo).
fn ammo_slot_mut(ammo: &mut PlayerAmmo, gun: Gun) -> Option<&mut i32> {
    match gun {
        Gun::Fist => None,
        Gun::Sg => Some(&mut ammo.shotgun),
        Gun::Cg => Some(&mut ammo.chaingun),
        Gun::Rl => Some(&mut ammo.rocket_launcher),
        Gun::Rifle => Some(&mut ammo.rifle),
        Gun::Gl => Some(&mut ammo.grenade_launcher),
        Gun::Pistol => Some(&mut ammo.pistol),
    }
}

// ── CombatState ───────────────────────────────────────────────────────────────

/// Tracks projectiles currently in flight, keyed by (shooter_cn, shot_id).
pub struct CombatState {
    projectiles: DashMap<(u32, i32), Gun>,
}

impl CombatState {
    pub(crate) fn new() -> Self {
        Self { projectiles: DashMap::new() }
    }
}

// ── Damage helpers ────────────────────────────────────────────────────────────

/// Apply a list of hits from a weapon shot.
/// Broadcasts N_DAMAGE and N_DIED for each valid hit.
fn apply_hits(
    shooter_cn: u32,
    gun: Gun,
    hits: &[HitInfo],
    game: &GameState,
    registry: &ClientRegistry,
    server: &Server,
) {
    let gidx = gun_idx(gun);
    let base_damage = GUN_DAMAGE[gidx];
    let max_rays = GUN_RAYS[gidx];

    for hit in hits {
        let target_cn = hit.target_cn as u32;
        let rays = hit.rays.min(max_rays);
        if rays <= 0 {
            continue;
        }
        let damage = rays * base_damage;

        // Apply damage; release the borrow before touching the shooter.
        let killed = {
            let Some(mut target) = registry.get_mut(target_cn) else { continue };
            if target.state != ClientState::Alive {
                continue;
            }
            if hit.life_sequence != target.life_sequence {
                continue;
            }

            {
                let pct = match target.armour_type {
                    ArmourType::Blue => 25,
                    ArmourType::Green => 30,
                    ArmourType::Yellow => 40,
                };
                let absorbed = (damage * pct / 100).min(target.armour);
                target.armour -= absorbed;
                target.health -= damage - absorbed;
            }
            let dead = target.health <= 0;
            if dead {
                target.state = ClientState::Dead;
                target.death_ms = Some(game.gamemillis());
                target.deaths += 1;
            }

            server.broadcast_chan1(&channel1::Damage {
                target_cn: target_cn as i32,
                actor_cn: shooter_cn as i32,
                damage,
                armour: target.armour,
                health: target.health,
            });
            server.broadcast_chan1(&channel1::HitPush {
                target_cn: target_cn as i32,
                gun,
                damage,
                dir: hit.dir,
            });

            dead
        };

        if killed {
            let actor_frags = if shooter_cn != target_cn {
                if let Some(mut shooter) = registry.get_mut(shooter_cn) {
                    shooter.frags += 1;
                    shooter.frags
                } else {
                    0
                }
            } else {
                registry.get(shooter_cn).map(|s| s.frags).unwrap_or(0)
            };

            server.broadcast_chan1(&channel1::Died {
                victim_cn: target_cn as i32,
                actor_cn: shooter_cn as i32,
                actor_frags,
                team_frags: 0,
            });
        }
    }
}

/// Kill a client immediately (e.g. suicide). Broadcasts N_DIED.
fn kill_client(cn: u32, game: &GameState, registry: &ClientRegistry, server: &Server) {
    let Some(mut client) = registry.get_mut(cn) else { return };
    if client.state != ClientState::Alive {
        return;
    }
    client.state = ClientState::Dead;
    client.health = 0;
    client.death_ms = Some(game.gamemillis());
    client.deaths += 1;
    let frags = client.frags;
    drop(client);

    server.broadcast_chan1(&channel1::Died {
        victim_cn: cn as i32,
        actor_cn: cn as i32,
        actor_frags: frags,
        team_frags: 0,
    });
}

// ── Handlers ──────────────────────────────────────────────────────────────────

pub async fn on_pos(server: &Arc<Server>, cn: u32, packet: &channel0::Position) -> Control {
    if packet.client_num != cn {
        return PASS;
    }

    let registry = server.extra.get::<ClientRegistry>().unwrap();
    if let Some(client) = registry.get(cn) {
        if client.state != ClientState::Alive {
            return PASS;
        }
        // Reject packets from a previous life.
        if packet.life_seq_parity != ((client.life_sequence & 1) != 0) {
            return PASS;
        }
    }

    server.broadcast_except_chan0(cn, packet);
    PASS
}

pub async fn on_gun_select(server: &Arc<Server>, cn: u32, packet: &GunSelect) -> Control {
    let registry = server.extra.get::<ClientRegistry>().unwrap();
    if let Some(mut client) = registry.get_mut(cn) {
        client.gun_select = packet.gun;
    }
    server.broadcast_except_chan1(
        cn,
        &channel1::Client {
            client_num: cn as i32,
            packet: channel1::GunSelect { gun: packet.gun },
        },
    );
    PASS
}

pub async fn on_shoot(server: &Arc<Server>, cn: u32, packet: &Shoot) -> Control {
    let registry = server.extra.get::<ClientRegistry>().unwrap();
    let game = server.extra.get::<GameState>().unwrap();
    let combat = server.extra.get::<CombatState>().unwrap();

    let gidx = gun_idx(packet.gun);
    let now_ms = game.gamemillis();

    {
        let Some(mut client) = registry.get_mut(cn) else { return PASS };
        if client.state != ClientState::Alive {
            return PASS;
        }

        // Fire-rate gate.
        if now_ms.saturating_sub(client.last_shot_ms) < GUN_ATTACK_DELAY[gidx] {
            return PASS;
        }

        // Ammo check and decrement.
        if let Some(slot) = ammo_slot_mut(&mut client.ammo, packet.gun) {
            if *slot == 0 {
                return PASS;
            }
            // -1 means infinite (instagib rifle); positive → consume one.
            if *slot > 0 {
                *slot -= 1;
            }
        }

        client.last_shot_ms = now_ms;
    }

    // Track projectile if applicable.
    if is_projectile(packet.gun) {
        combat.projectiles.insert((cn, packet.id), packet.gun);
    }

    server.broadcast_chan1(&channel1::ShotFx {
        client_num: cn as i32,
        gun: packet.gun,
        id: packet.id,
        from: packet.from,
        to: packet.to,
    });

    // Hitscan weapons resolve hits immediately.
    if !is_projectile(packet.gun) {
        apply_hits(cn, packet.gun, &packet.hits, &game, &registry, server);
    }

    PASS
}

pub async fn on_explode(server: &Arc<Server>, cn: u32, packet: &Explode) -> Control {
    let registry = server.extra.get::<ClientRegistry>().unwrap();
    let game = server.extra.get::<GameState>().unwrap();
    let combat = server.extra.get::<CombatState>().unwrap();

    // Look up and remove the pending projectile.
    let Some((_, gun)) = combat.projectiles.remove(&(cn, packet.id)) else {
        return PASS;
    };

    // Sanity: gun must match what we recorded.
    if gun != packet.gun {
        return PASS;
    }

    server.broadcast_chan1(&channel1::ExplodeFx {
        client_num: cn as i32,
        gun: packet.gun,
        id: packet.id,
    });

    apply_hits(cn, packet.gun, &packet.hits, &game, &registry, server);

    PASS
}

pub async fn on_suicide(server: &Arc<Server>, cn: u32, _packet: &Suicide) -> Control {
    let registry = server.extra.get::<ClientRegistry>().unwrap();
    let game = server.extra.get::<GameState>().unwrap();
    kill_client(cn, &game, &registry, server);
    PASS
}
