// TEMPORARY TEST CODE — remove once button placement is verified working
use crate::map_state::get_server_state;
use dashmap::DashMap;
use sauer_map::{Selection, raycast::raycast};
use sauer_proto::{
    Client,
    channel0, channel1,
    data::{Direction, EditFaceMode, EntityType, Orient, SelInfo, Vec3},
};
use sauer_server::plugin::{Context, Plugin};
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::{SystemTime, UNIX_EPOCH};

// Buttons are always this many world units on a side, regardless of hit cube size
const BUTTON_SIZE: i32 = 8;

// Placeholder texture — swap once button placement is confirmed
const BUTTON_TEX: u16 = 2;

pub struct DeathrunPlay {
    last_raycast_ms: AtomicU64,
    /// Last known view direction per player, updated every N_POS
    player_dirs: DashMap<u32, Direction>,
}

impl DeathrunPlay {
    pub fn new() -> Self {
        Self {
            last_raycast_ms: AtomicU64::new(0),
            player_dirs: DashMap::new(),
        }
    }
}

impl Plugin for DeathrunPlay {
    // ── crosshair fire-particle test ─────────────────────────────────────────

    async fn pos(&self, ctx: &mut Context, packet: &channel0::Position) {
        // Always keep direction current regardless of throttle
        self.player_dirs.insert(packet.client_num, packet.dir);

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

        // Build view ray: x = -sin(yaw)*cos(pitch), y = cos(yaw)*cos(pitch), z = sin(pitch)
        let yaw_rad = (packet.dir.yaw as f32).to_radians();
        let pitch_rad = (packet.dir.pitch as f32).to_radians();
        let ray = [
            -yaw_rad.sin() * pitch_rad.cos(),
             yaw_rad.cos() * pitch_rad.cos(),
             pitch_rad.sin(),
        ];
        // N_POS sends feet position; eye is 14 units above (physent::eyeheight default)
        let origin = [packet.pos.x, packet.pos.y, packet.pos.z + 14.0];

        let state_lock = get_server_state(ctx.server()).read().unwrap();
        let hit_pos = match state_lock.map.as_ref() {
            None => return,
            Some(map) => match raycast(&map.world, origin, ray, 2048.0) {
                None => return,
                Some(hit) => [
                    origin[0] + ray[0] * hit.dist,
                    origin[1] + ray[1] * hit.dist,
                    origin[2] + ray[2] * hit.dist,
                ],
            },
        };
        drop(state_lock);

        // attr1=0 = fire+smoke, attr2/attr3=0 → defaults (radius=1.5, height=0.5)
        ctx.server().broadcast_chan1(&Client {
            client_num: packet.client_num as i32,
            packet: channel1::EditEnt {
                ent_id: 0,
                pos: Vec3 { x: hit_pos[0], y: hit_pos[1], z: hit_pos[2] },
                type_: EntityType::Particles,
                attrs: [0, 0, 0, 0, 0],
            },
        });
    }

    // ── button placement on shoot ─────────────────────────────────────────────

    async fn shoot(&self, ctx: &mut Context, _packet: &channel1::Shoot) {
        // Use the player's exact view direction from the last N_POS, not the shot direction
        // (shots have spread; the crosshair is always exact)
        let cn = ctx.cn();
        let dir = match self.player_dirs.get(&cn) {
            Some(d) => *d,
            None => return,
        };
        let pos = match ctx.server().players.try_get(cn) {
            Some(p) => p.position,
            None => return,
        };

        let yaw_rad = (dir.yaw as f32).to_radians();
        let pitch_rad = (dir.pitch as f32).to_radians();
        let ray = [
            -yaw_rad.sin() * pitch_rad.cos(),
             yaw_rad.cos() * pitch_rad.cos(),
             pitch_rad.sin(),
        ];
        // N_POS sends feet position; eye is 14 units above
        let origin = [pos.x, pos.y, pos.z + 14.0];

        let server = ctx.server().clone();

        let mut state = get_server_state(&server).write().unwrap();
        let map = match state.map.as_mut() {
            None => return,
            Some(m) => m,
        };

        // Copy scalars immediately to release the immutable borrow on map.world
        let (hit_pos, hit_size, hit_orient, hit_dist) =
            match raycast(&map.world, origin, ray, 2048.0) {
                None => return,
                Some(h) => (h.pos, h.size, h.orient, h.dist),
            };

        let button_size = BUTTON_SIZE;

        // Decompose orientation into normal axis d and face side dc (0=near, 1=far)
        let d = (hit_orient >> 1) as usize;  // 0=X, 1=Y, 2=Z
        let dc = (hit_orient & 1) as usize;  // 0=near face, 1=far face

        // R and C are the two axes perpendicular to d (Sauerbraten's permutation table)
        let r_axis = [1usize, 2, 0][d];
        let c_axis = [2usize, 0, 1][d];

        // Exact hit point on the surface — used to snap the button within the face
        let hit_pt = [
            origin[0] + ray[0] * hit_dist,
            origin[1] + ray[1] * hit_dist,
            origin[2] + ray[2] * hit_dist,
        ];

        let snap = |v: f32| -> i32 { (v / button_size as f32).floor() as i32 * button_size };

        // Snap the hit point along the normal axis — avoids placing on the AABB boundary of
        // deformed/curved cubes and always places the button flush with the visible surface.
        let snap_d = (hit_pt[d] / button_size as f32).floor() as i32 * button_size;

        // Button cube (the empty space just outside the wall face)
        let mut adj = [0i32; 3];
        adj[d]      = if dc == 0 { snap_d - button_size } else { snap_d };
        adj[r_axis] = snap(hit_pt[r_axis]);
        adj[c_axis] = snap(hit_pt[c_axis]);

        // N_EDITF selection: the wall cube one step inside from adj.
        // mpeditface(mode=Fill, dir=-1) shifts this selection into adj, filling it solid.
        let mut editf_o = adj;
        editf_o[d] = if dc == 0 { adj[d] + button_size } else { adj[d] - button_size };

        // Bounds checks
        let ws = map.world.size;
        if adj.iter().any(|&v| v < 0 || v >= ws)
            || editf_o.iter().any(|&v| v < 0 || v >= ws)
            || adj[r_axis] + button_size > ws
            || adj[c_axis] + button_size > ws
        {
            return;
        }

        // Internal server state: fill adj solid and set texture
        let sel = Selection {
            o: adj,
            s: [1, 1, 1],
            grid: button_size,
            orient: hit_orient,
            cx: 0, cxs: 2, cy: 0, cys: 2,
            corner: 0,
        };
        map.world.fill(&sel, true);
        map.world.set_texture(&sel, BUTTON_TEX, true);
        drop(state);

        let proto_orient = match hit_orient {
            0 => Orient::Left,
            1 => Orient::Right,
            2 => Orient::Back,
            3 => Orient::Front,
            4 => Orient::Bottom,
            _ => Orient::Top,
        };

        // N_EDITF: selection is the wall cube; mpeditface shifts it into adj and fills solid
        server.broadcast_chan1(&Client {
            client_num: cn as i32,
            packet: channel1::EditFace {
                sel: SelInfo {
                    o: Vec3 { x: editf_o[0], y: editf_o[1], z: editf_o[2] },
                    s: Vec3 { x: 1, y: 1, z: 1 },
                    grid: button_size,
                    orient: proto_orient,
                    cx: 0, cxs: 2, cy: 0, cys: 2,
                    corner: 0,
                },
                dir: -1,
                mode: EditFaceMode::Fill,
            },
        });

        // N_EDITT: selection is adj directly — overrides the inherited wall texture
        server.broadcast_chan1(&Client {
            client_num: cn as i32,
            packet: channel1::EditTex {
                sel: SelInfo {
                    o: Vec3 { x: adj[0], y: adj[1], z: adj[2] },
                    s: Vec3 { x: 1, y: 1, z: 1 },
                    grid: button_size,
                    orient: proto_orient,
                    cx: 0, cxs: 2, cy: 0, cys: 2,
                    corner: 0,
                },
                tex: BUTTON_TEX as i32,
                all_faces: true,
                extra_data: vec![],
            },
        });
    }
}
