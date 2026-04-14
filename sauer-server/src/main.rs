use cube_string::{Color, CubeString};
use enet_rs::{Host, HostConfig};
use glam::Vec3;
use ogz_rs::OgzMap;
use sauer_map::{BoundingBox, CollisionResult, World};
use sauer_proto::{
    Chan0Client, Chan0Server, Chan1Client, Chan1Server,
    data::{
        ArmourType, GameMode, Gun, PhysState, PlayerAmmo, PlayerModel, PlayerPosition, PlayerState,
        PositionPhysState, Vector3 as SauerVec3,
    },
};
use std::sync::Arc;
use std::time::Instant;

use crate::net::{ClientMessage, SauerPeer};

mod net;

const MAP: &str = "glasstest";

// Sauerbraten player physics constants
const PLAYER_RADIUS: f32 = 4.1;
const PLAYER_HEIGHT: f32 = 15.0; // eyeheight(14) + aboveeye(1)
const WALK_SPEED: f32 = 50.0;
const GRAVITY: f32 = 200.0;
const MAX_FALL: f32 = 200.0;
const STEP_HEIGHT: f32 = 4.0;
const FAKE_CN: i32 = 1;

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    let ogz_map = Arc::new(OgzMap::parse(
        &tokio::fs::read("/home/mykolas/.config/sauerbraten/packages/base/glasstest.ogz").await?,
    )?);

    let mut host = Host::bind(
        "127.0.0.1:28785",
        HostConfig {
            max_peers: 1024,
            channel_count: sauer_proto::CHANNEL_COUNT as u8,
            use_checksum: false,
            incoming_bandwidth: 0,
            outgoing_bandwidth: 0,
        },
    )
    .await?;

    while let Some(peer) = host.accept().await {
        let peer = SauerPeer::new(peer);
        let ogz_map = Arc::clone(&ogz_map);
        tokio::spawn(async move {
            if let Err(e) = handle_client(peer, ogz_map).await {
                println!("Error: {e}");
            }
        });
    }

    Ok(())
}

struct FakePlayer {
    /// None until first real-player POS is received.
    pos: Option<Vec3>,
    vel: Vec3,
    spawned: bool,
    last_tick: Instant,
}

impl FakePlayer {
    fn new() -> Self {
        Self {
            pos: None,
            vel: Vec3::ZERO,
            spawned: false,
            last_tick: Instant::now(),
        }
    }

    /// Step the fake player toward `target` using physics.
    fn step(&mut self, target: Vec3, world: &World) {
        let now = Instant::now();
        let dt = now.duration_since(self.last_tick).as_secs_f32().min(0.1);
        self.last_tick = now;

        let pos = self.pos.get_or_insert(target + Vec3::new(0.0, 0.0, 20.0));

        // Horizontal velocity toward target
        let diff = Vec3::new(target.x - pos.x, target.y - pos.y, 0.0);
        let dist = diff.length();
        let horiz = if dist > 1.0 {
            diff / dist * WALK_SPEED
        } else {
            Vec3::ZERO
        };

        self.vel.x = horiz.x;
        self.vel.y = horiz.y;
        self.vel.z -= GRAVITY * dt;
        self.vel.z = self.vel.z.max(-MAX_FALL);

        let (new_pos, on_ground) = resolve_move(*pos, self.vel * dt, world);
        *pos = new_pos;

        if on_ground {
            self.vel.z = 0.0;
        }
    }
}

/// Attempt a physics move with stair stepping and wall sliding.
/// Returns (new_pos, on_ground).
fn resolve_move(pos: Vec3, move_vec: Vec3, world: &World) -> (Vec3, bool) {
    let candidate = pos + move_vec;
    match collide(candidate, world) {
        CollisionResult::Free => (candidate, false),
        CollisionResult::TrappedInside => (pos, false),
        CollisionResult::HitSurface {
            normal,
            penetration,
        } => {
            let pushed = candidate + normal * penetration;

            // Floor surface: push out and land
            if normal.z > 0.5 {
                // Only apply push-out; horizontal movement already in candidate
                if let CollisionResult::Free | CollisionResult::HitSurface { .. } =
                    collide(pushed, world)
                {
                    return (pushed, true);
                }
                return (pos, true);
            }

            // Stair step-up: try lifting by STEP_HEIGHT then moving horizontally
            let step_base = pos + Vec3::new(0.0, 0.0, STEP_HEIGHT);
            if let CollisionResult::Free = collide(step_base, world) {
                let step_candidate = step_base + Vec3::new(move_vec.x, move_vec.y, 0.0);
                if let CollisionResult::Free = collide(step_candidate, world) {
                    return (step_candidate, false);
                }
            }

            // Slide along wall: remove velocity component into the normal
            let slide = move_vec - normal * move_vec.dot(normal);
            let slid = pos + slide;
            if let CollisionResult::Free = collide(slid, world) {
                return (slid, false);
            }

            // Plain push-out
            if let CollisionResult::Free = collide(pushed, world) {
                return (pushed, false);
            }

            (pos, false)
        }
    }
}

fn collide(pos: Vec3, world: &World) -> CollisionResult {
    let bbox = BoundingBox::new(pos.x, pos.y, pos.z, PLAYER_RADIUS, PLAYER_HEIGHT);
    world.collide(&bbox)
}

async fn handle_client(mut peer: SauerPeer, ogz: Arc<OgzMap>) -> anyhow::Result<()> {
    let map = World::from_ogz(&ogz);

    peer.send_chan1(&Chan1Server::ServInfo {
        client_num: 0,
        protocol: sauer_proto::PROTOCOL_VERSION,
        session_id: 0,
        has_password: false,
        server_desc: CubeString::builder()
            .color(Color::Magenta)
            .push_str("Retard server v2")
            .build(),
        server_auth: "".into(),
    })
    .await?;

    let player_name = 'outer: loop {
        let pkt = peer.recv().await?;

        match pkt {
            ClientMessage::Chan1(Chan1Client::Connect {
                name,
                player_model: _,
                password_hash: _,
                auth_desc: _,
                auth_name: _,
            }) => break 'outer name,
            _ => continue,
        };
    };

    println!("{player_name} connected");

    peer.send_chan1(&Chan1Server::Welcome).await?;
    peer.send_chan1(&Chan1Server::MapChange {
        map_name: MAP.into(),
        mode: GameMode::CoopEdit,
        has_items: true,
    })
    .await?;

    // Announce the fake player
    peer.send_chan1(&Chan1Server::InitClient {
        client_num: FAKE_CN,
        name: "FakeBot".into(),
        team: "".into(),
        player_model: PlayerModel::Ogro2,
    })
    .await?;

    let mut fake = FakePlayer::new();

    loop {
        let pkt = peer.recv().await?;
        match pkt {
            ClientMessage::Chan1(pkt) => match pkt {
                Chan1Client::TrySpawn => {
                    // Spawn the real player
                    peer.send_chan1(&Chan1Server::SpawnState {
                        client_num: 0,
                        state: PlayerState {
                            life_sequence: 0,
                            health: 100,
                            max_health: 100,
                            armour: 0,
                            armour_type: ArmourType::Yellow,
                            gun_select: Gun::Fist,
                            ammo: PlayerAmmo::default(),
                        },
                    })
                    .await?;

                    // Spawn the fake player via SpawnState so the client sets
                    // state=CS_ALIVE and calls pickgamespawn (placing it on map).
                    // N_POS updates from us will override the position immediately.
                    peer.send_chan1(&Chan1Server::SpawnState {
                        client_num: FAKE_CN,
                        state: PlayerState {
                            life_sequence: 0,
                            health: 100,
                            max_health: 100,
                            armour: 0,
                            armour_type: ArmourType::Yellow,
                            gun_select: Gun::Fist,
                            ammo: PlayerAmmo::default(),
                        },
                    })
                    .await?;

                    fake.spawned = true;
                }
                other => {
                    println!("rcv: {other:?}");
                }
            },

            ClientMessage::Chan0(pkt) => match pkt {
                Chan0Client::Position { pos: real_pos } => {
                    if !fake.spawned {
                        continue;
                    }

                    let real = Vec3::new(real_pos.pos.x, real_pos.pos.y, real_pos.pos.z);

                    fake.step(real, &map);

                    let fake_pos = fake.pos.unwrap();
                    peer.send_chan0(&Chan0Server::Position {
                        pos: PlayerPosition {
                            client_num: FAKE_CN as u32,
                            phys_state: PositionPhysState {
                                phys_state: PhysState::Fall,
                                life_sequence: false,
                                move_dir: 0,
                                strafe: 0,
                            },
                            flags: 0,
                            pos: SauerVec3 {
                                x: fake_pos.x,
                                y: fake_pos.y,
                                z: fake_pos.z,
                            },
                            dir: real_pos.dir,
                            roll: 0,
                            mag: 0,
                            vel_dir: 0,
                            fall_mag: 0,
                            fall_dir: 0,
                        },
                    })
                    .await?;
                }
                _ => {}
            },

            _ => {}
        }
    }
}
