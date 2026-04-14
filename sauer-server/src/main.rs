use crate::net::{ClientMessage, SauerPeer};
use cube_string::IntoCubeStringLossy;
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

mod net;

const MAP: &str = "glasstest";

// Sauerbraten player physics constants
const PLAYER_RADIUS: f32 = 4.1;
const PLAYER_HEIGHT: f32 = 15.0; // eyeheight(14) + aboveeye(1)
const WALK_SPEED: f32 = 50.0;
const GRAVITY: f32 = 200.0;
const MAX_FALL: f32 = 200.0;
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
    pos: Vec3,
    vel: Vec3,
    last_tick: Instant,
}

impl FakePlayer {
    fn new() -> Self {
        Self {
            pos: Vec3::new(0., 0., 300.),
            vel: Vec3::ZERO,
            last_tick: Instant::now(),
        }
    }

    /// Step the fake player toward `target` using physics.
    fn step(&mut self, target: Vec3, world: &World) {
        let diff = target - self.pos;
        let delta = self.last_tick.elapsed().as_secs_f32() / 1000.0;
        self.last_tick = Instant::now();

        let xy_mag = (diff.x.powi(2) + diff.y.powi(2)).sqrt();
        let norm_x = diff.x / xy_mag;
        let norm_y = diff.y / xy_mag;

        self.vel.x = norm_x * WALK_SPEED;
        self.vel.y = norm_y * WALK_SPEED;
        self.vel.z = (-MAX_FALL).max(self.vel.z - GRAVITY * delta);

        // Check if moving to the next position causes a collision
        let next_pos = self.pos + self.vel;
        let bbox = BoundingBox::new(
            next_pos.x,
            next_pos.y,
            next_pos.z,
            PLAYER_RADIUS,
            PLAYER_HEIGHT,
        );

        match world.collide(&bbox) {
            CollisionResult::HitSurface {
                normal,
                penetration,
            } => {
                // 1. Push the player out of the geometry using the normal and penetration depth
                self.pos += normal * penetration;

                // 2. Cancel out velocity pushing *into* the wall to allow sliding
                let dot = self.vel.dot(normal);
                if dot < 0.0 {
                    self.vel -= normal * dot;
                }

                // 3. Apply the corrected sliding velocity
                self.pos += self.vel;
            }
            CollisionResult::TrappedInside => {
                // Reject movement entirely or kill player
                println!("Player is inside a wall!");
            }
            CollisionResult::Free => {
                // Space is clear, just apply velocity
                self.pos += self.vel;
            }
        }
    }
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
        server_auth: CubeString::new(),
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
        map_name: MAP.try_into().unwrap(),
        mode: GameMode::CoopEdit,
        has_items: true,
    })
    .await?;

    // Announce the fake player
    peer.send_chan1(&Chan1Server::InitClient {
        client_num: FAKE_CN,
        name: "FakeBot".lossy_cubestring(),
        team: "".lossy_cubestring(),
        player_model: PlayerModel::Ogro2,
    })
    .await?;

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

    let mut fake = FakePlayer::new();

    loop {
        let pkt = peer.recv().await?;
        match pkt {
            ClientMessage::Chan1(pkt) => match pkt {
                other => {
                    println!("rcv: {other:?}");
                }
            },

            ClientMessage::Chan0(pkt) => match pkt {
                Chan0Client::Position { pos: real_pos } => {
                    let real = Vec3::new(real_pos.pos.x, real_pos.pos.y, real_pos.pos.z);

                    fake.step(real, &map);

                    peer.send_chan1(&Chan1Server::Text {
                        client_num: FAKE_CN,
                        text: format!("{:?} - {:?}", fake.pos, fake.vel).lossy_cubestring(),
                    })
                    .await?;

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
                                x: fake.pos.x,
                                y: fake.pos.y,
                                z: fake.pos.z,
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
