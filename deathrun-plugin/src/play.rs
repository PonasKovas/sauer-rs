// TEMPORARY TEST CODE — remove once button placement is verified working
use crate::map_state::get_server_state;
use dashmap::DashMap;
use ogz_rs::data::Material;
use sauer_map::{Selection, raycast::raycast};
use sauer_proto::{
    Client,
    channel0, channel1,
    data::{Direction, EditFaceMode, EntityType, Orient, SelInfo, Vec3},
};
use sauer_server::plugin::{Context, Plugin};
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::{SystemTime, UNIX_EPOCH};

const BUTTON_SIZE: i32 = 8;
const BUTTON_FRONT_TEX: u16 = 248;
const BUTTON_SIDE_TEX: u16 = 242;
const BUTTON_COLOR: [f32; 3] = [0.2, 0.2, 0.2];

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
        let (_hit_pos, _hit_size, hit_orient, hit_dist) =
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

        // dc=0 (near face, player at -d): floor snap → button ends at or before the surface.
        // dc=1 (far face, player at +d): ceil snap → button starts at or after the surface,
        // preventing it from snapping back into the same grid cell as an inset/deformed cube.
        // Small epsilon before ceil guards against hit points that land exactly on a grid line.
        let snap_d = if dc == 0 {
            (hit_pt[d] / button_size as f32).floor() as i32 * button_size
        } else {
            ((hit_pt[d] - 0.001) / button_size as f32).ceil() as i32 * button_size
        };

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
        map.world.set_texture(&sel, BUTTON_SIDE_TEX, true);
        map.world.set_texture(&sel, BUTTON_FRONT_TEX, false);
        map.world.set_material(&sel, Material { alpha: true, ..Default::default() });
        for _ in 0..6 {
            map.world.edit_face(&sel, 1, 0);
        }
        drop(state);

        let proto_orient = match hit_orient {
            0 => Orient::Left,
            1 => Orient::Right,
            2 => Orient::Back,
            3 => Orient::Front,
            4 => Orient::Bottom,
            _ => Orient::Top,
        };

        let adj_sel = SelInfo {
            o: Vec3 { x: adj[0], y: adj[1], z: adj[2] },
            s: Vec3 { x: 1, y: 1, z: 1 },
            grid: button_size,
            orient: proto_orient,
            cx: 0, cxs: 2, cy: 0, cys: 2,
            corner: 0,
        };
        let fill_sel = SelInfo {
            o: Vec3 { x: editf_o[0], y: editf_o[1], z: editf_o[2] },
            ..adj_sel
        };

        tokio::spawn(async move {
            use std::time::Duration;
            use tokio::time::sleep;

            // Immediately: fill solid + all visual properties
            server.broadcast_chan1(&Client {
                client_num: cn as i32,
                packet: channel1::EditFace { sel: fill_sel, dir: -1, mode: EditFaceMode::Fill },
            });
            server.broadcast_chan1(&Client {
                client_num: cn as i32,
                packet: channel1::EditTex {
                    sel: adj_sel.clone(), tex: BUTTON_SIDE_TEX as i32,
                    all_faces: true, extra_data: vec![],
                },
            });
            server.broadcast_chan1(&Client {
                client_num: cn as i32,
                packet: channel1::EditTex {
                    sel: adj_sel.clone(), tex: BUTTON_FRONT_TEX as i32,
                    all_faces: false, extra_data: vec![],
                },
            });
            let color_vslot = {
                let mut d = vec![7u8];
                for &f in &BUTTON_COLOR { d.extend_from_slice(&f.to_le_bytes()); }
                d.push(0xFF); d
            };
            server.broadcast_chan1(&Client {
                client_num: cn as i32,
                packet: channel1::EditVSlot {
                    sel: adj_sel.clone(), delta: 0, all_faces: true, extra_data: color_vslot,
                },
            });
            let scale_vslot = {
                let mut d = vec![1u8];
                d.extend_from_slice(&2.0f32.to_le_bytes());
                d.push(0xFF); d
            };
            server.broadcast_chan1(&Client {
                client_num: cn as i32,
                packet: channel1::EditVSlot {
                    sel: adj_sel.clone(), delta: 0, all_faces: false, extra_data: scale_vslot,
                },
            });
            server.broadcast_chan1(&Client {
                client_num: cn as i32,
                packet: channel1::EditMat { sel: adj_sel.clone(), mat: 1024, filter: -1 },
            });
            let alpha_vslot = {
                let mut d = vec![6u8];
                d.extend_from_slice(&0.8f32.to_le_bytes());
                d.extend_from_slice(&0.8f32.to_le_bytes());
                d.push(0xFF); d
            };
            server.broadcast_chan1(&Client {
                client_num: cn as i32,
                packet: channel1::EditVSlot {
                    sel: adj_sel.clone(), delta: 0, all_faces: true, extra_data: alpha_vslot,
                },
            });

            // 4 corners × 6 pushes each = 24 events at random times within 0..400ms
            // Generate all times before any await (ThreadRng is not Send)
            let mut events: Vec<(u64, i32)> = {
                use rand::Rng;
                let mut rng = rand::thread_rng();
                (0i32..4)
                    .flat_map(|c| std::iter::repeat(c).take(6))
                    .map(|c| (rng.gen_range(0u64..=400), c))
                    .collect()
            };
            events.sort_by_key(|&(t, _)| t);

            let mut elapsed = 0u64;
            for (t, corner) in events {
                if t > elapsed {
                    sleep(Duration::from_millis(t - elapsed)).await;
                    elapsed = t;
                }
                server.broadcast_chan1(&Client {
                    client_num: cn as i32,
                    packet: channel1::EditFace {
                        sel: SelInfo { corner, ..adj_sel.clone() },
                        dir: 1,
                        mode: EditFaceMode::Corner,
                    },
                });
            }
        });
    }
}
