// TEMPORARY TEST CODE — remove once raycast is verified working
use crate::map_state::get_server_state;
use sauer_map::raycast::raycast;
use sauer_proto::{Client, channel0, channel1, data::{EntityType, Vec3}};
use sauer_server::plugin::{Context, Plugin};
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::{SystemTime, UNIX_EPOCH};

pub struct DeathrunPlay {
    last_raycast_ms: AtomicU64,
}

impl DeathrunPlay {
    pub fn new() -> Self {
        Self {
            last_raycast_ms: AtomicU64::new(0),
        }
    }
}

impl Plugin for DeathrunPlay {
    async fn pos(&self, ctx: &mut Context, packet: &channel0::Position) {
        // Only track the first player for the test
        if packet.client_num != 0 {
            return;
        }

        // Throttle to ~10 Hz on channel 1 (reliable)
        let now_ms = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap_or_default()
            .as_millis() as u64;
        let last = self.last_raycast_ms.load(Ordering::Relaxed);
        if now_ms.saturating_sub(last) < 100 {
            return;
        }
        self.last_raycast_ms.store(now_ms, Ordering::Relaxed);

        // Build view ray from Sauerbraten's vecfromyawpitch convention:
        //   x = -sin(yaw) * cos(pitch)
        //   y =  cos(yaw) * cos(pitch)
        //   z =  sin(pitch)
        let yaw_rad = (packet.dir.yaw as f32).to_radians();
        let pitch_rad = (packet.dir.pitch as f32).to_radians();
        let ray = [
            -yaw_rad.sin() * pitch_rad.cos(),
             yaw_rad.cos() * pitch_rad.cos(),
             pitch_rad.sin(),
        ];
        // N_POS sends feet position; eye is 14 units above (physent::eyeheight default)
        let origin = [packet.pos.x, packet.pos.y, packet.pos.z + 14.0];

        // Cast ray against the server-side map octree
        let state_lock = get_server_state(ctx.server()).read().unwrap();
        let hit_pos = match state_lock.map.as_ref() {
            None => return,
            Some(map) => {
                match raycast(&map.world, origin, ray, 2048.0) {
                    None => return,
                    Some(hit) => [
                        origin[0] + ray[0] * hit.dist,
                        origin[1] + ray[1] * hit.dist,
                        origin[2] + ray[2] * hit.dist,
                    ],
                }
            }
        };
        drop(state_lock);

        // Move a fire entity (id=0) to the exact hit coordinate.
        // attr1=0 = fire+smoke, attr2/attr3=0 → defaults (radius=1.5, height=0.5).
        ctx.server().broadcast_chan1(&Client {
            client_num: packet.client_num as i32,
            packet: channel1::EditEnt {
                ent_id: 0,
                pos: Vec3 {
                    x: hit_pos[0],
                    y: hit_pos[1],
                    z: hit_pos[2],
                },
                type_: EntityType::Particles,
                attrs: [0, 0, 0, 0, 0],
            },
        });
    }
}
