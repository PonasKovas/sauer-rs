use crate::map_state::{
    DeathrunState, MapState, Trap, get_server_state, is_waiting_for_map, send_map_to,
    with_map_state_mut,
};
use bytes::Bytes;
use cube_string::{CubeString, IntoCubeStringLossy};
use dashmap::DashMap;
use ogz_rs::data::Material;
use sauer_map::{Selection, raycast::raycast};
use sauer_proto::{
    Client,
    channel0, channel1::{self, EditMode, ServMsg},
    data::{
        ClientState, Direction, EditFaceMode, GameMode, Orient, PlayerState, Privilege, SelInfo,
        Vec3,
    },
};
use sauer_server::{
    Server,
    plugin::{Context, Plugin},
};
use std::sync::RwLock;
use tracing::{info, warn};

const BUTTON_SIZE: i32 = 8;
const BUTTON_FRONT_TEX: u16 = 248;
const BUTTON_SIDE_TEX: u16 = 242;
const BUTTON_COLOR: [f32; 3] = [0.2, 0.2, 0.2];

pub struct DeathrunEdit {
    player_dirs: DashMap<u32, Direction>,
}

impl DeathrunEdit {
    pub fn new() -> Self {
        Self {
            player_dirs: DashMap::new(),
        }
    }
}

impl Plugin for DeathrunEdit {
    async fn init(&self, server: &mut Server) {
        server.extra.insert(RwLock::new(DeathrunState::default()));
    }

    // Track every player's view direction so #newtrap can raycast correctly.
    async fn pos(&self, _ctx: &mut Context, packet: &channel0::Position) {
        self.player_dirs.insert(packet.client_num, packet.dir);
    }

    async fn text(&self, ctx: &mut Context, packet: &channel1::Text) {
        let cmd = packet.text.strip_colors().to_utf8();
        let cmd = cmd.trim();
        if cmd != "#newtrap" && cmd != "#deltrap" {
            return;
        }
        ctx.cancel();

        let cn = ctx.cn();
        let server = ctx.server().clone();

        let (pos, is_editing) = match server.players.try_get(cn) {
            Some(p) => (p.position, p.client_state == ClientState::Editing),
            None => return,
        };

        if !is_editing {
            server.send_chan1(
                cn,
                &ServMsg {
                    text: "Must be in edit mode to place a trap.".lossy_cubestring(),
                },
            );
            return;
        }

        let dir = match self.player_dirs.get(&cn) {
            Some(d) => *d,
            None => return,
        };

        let yaw_rad = (dir.yaw as f32).to_radians();
        let pitch_rad = (dir.pitch as f32).to_radians();
        let ray = [
            -yaw_rad.sin() * pitch_rad.cos(),
             yaw_rad.cos() * pitch_rad.cos(),
             pitch_rad.sin(),
        ];
        let origin = [pos.x, pos.y, pos.z + 14.0];

        let mut state = get_server_state(&server).write().unwrap();
        let map = match state.map.as_mut() {
            None => return,
            Some(m) => m,
        };

        let hit = match raycast(&map.world, origin, ray, 2048.0) {
            None => return,
            Some(h) => h,
        };
        let hit_pt = [
            origin[0] + ray[0] * hit.dist,
            origin[1] + ray[1] * hit.dist,
            origin[2] + ray[2] * hit.dist,
        ];

        // ── #deltrap ──────────────────────────────────────────────────────────
        if cmd == "#deltrap" {
            let trap_idx = state.traps.iter().position(|t| {
                (0..3).all(|ax| {
                    hit_pt[ax] >= t.button_pos[ax] as f32
                        && hit_pt[ax] < (t.button_pos[ax] + t.button_size) as f32
                })
            });
            let trap_idx = match trap_idx {
                None => {
                    server.send_chan1(
                        cn,
                        &ServMsg {
                            text: "Not looking at a trap.".lossy_cubestring(),
                        },
                    );
                    return;
                }
                Some(i) => i,
            };
            let trap = state.traps.remove(trap_idx);

            let map = state.map.as_mut().unwrap();
            let sel = Selection {
                o: trap.button_pos, s: [1, 1, 1], grid: trap.button_size,
                orient: trap.button_orient, cx: 0, cxs: 2, cy: 0, cys: 2, corner: 0,
            };
            map.world.fill(&sel, false);
            drop(state);

            let proto_orient = match trap.button_orient {
                0 => Orient::Left, 1 => Orient::Right, 2 => Orient::Back,
                3 => Orient::Front, 4 => Orient::Bottom, _ => Orient::Top,
            };
            server.broadcast_chan1(&Client {
                client_num: cn as i32,
                packet: channel1::DelCube {
                    sel: SelInfo {
                        o: Vec3 { x: trap.button_pos[0], y: trap.button_pos[1], z: trap.button_pos[2] },
                        s: Vec3 { x: 1, y: 1, z: 1 },
                        grid: trap.button_size,
                        orient: proto_orient,
                        cx: 0, cxs: 2, cy: 0, cys: 2, corner: 0,
                    },
                },
            });
            return;
        }

        // ── #newtrap ──────────────────────────────────────────────────────────
        let hit_orient = hit.orient;

        let button_size = BUTTON_SIZE;
        let d = (hit_orient >> 1) as usize;
        let dc = (hit_orient & 1) as usize;
        let r_axis = [1usize, 2, 0][d];
        let c_axis = [2usize, 0, 1][d];

        let snap = |v: f32| -> i32 { (v / button_size as f32).floor() as i32 * button_size };
        let snap_d = if dc == 0 {
            (hit_pt[d] / button_size as f32).floor() as i32 * button_size
        } else {
            ((hit_pt[d] - 0.001) / button_size as f32).ceil() as i32 * button_size
        };

        let mut adj = [0i32; 3];
        adj[d]      = if dc == 0 { snap_d - button_size } else { snap_d };
        adj[r_axis] = snap(hit_pt[r_axis]);
        adj[c_axis] = snap(hit_pt[c_axis]);

        let mut editf_o = adj;
        editf_o[d] = if dc == 0 { adj[d] + button_size } else { adj[d] - button_size };

        let ws = map.world.size;
        if adj.iter().any(|&v| v < 0 || v >= ws)
            || editf_o.iter().any(|&v| v < 0 || v >= ws)
            || adj[r_axis] + button_size > ws
            || adj[c_axis] + button_size > ws
        {
            return;
        }

        let sel = Selection {
            o: adj, s: [1, 1, 1], grid: button_size, orient: hit_orient,
            cx: 0, cxs: 2, cy: 0, cys: 2, corner: 0,
        };
        map.world.fill(&sel, true);
        map.world.set_texture(&sel, BUTTON_SIDE_TEX, true);
        map.world.set_texture(&sel, BUTTON_FRONT_TEX, false);
        map.world.set_material(&sel, Material { alpha: true, ..Default::default() });
        for _ in 0..6 {
            map.world.edit_face(&sel, 1, 0);
        }

        state.traps.push(Trap { button_pos: adj, button_size, button_orient: hit_orient });
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
                let mut d = vec![7u8]; // VSLOT_COLOR
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
                let mut d = vec![1u8]; // VSLOT_SCALE
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
                let mut d = vec![6u8]; // VSLOT_ALPHA
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

            // 4 corners × 6 pushes each, scattered randomly over 0..400ms.
            // Generate all random values before the first await (ThreadRng is not Send).
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

    async fn edit_mode(&self, ctx: &mut Context, _packet: &channel1::EditMode) {
        if is_waiting_for_map(ctx.server()) {
            ctx.server().send_chan1(
                ctx.cn(),
                &Client {
                    client_num: ctx.cn() as i32,
                    packet: EditMode { is_editing: false },
                },
            );
            ctx.server().send_chan1(
                ctx.cn(),
                &ServMsg {
                    text: CubeString::builder()
                        .color(cube_string::Color::Yellow)
                        .push_str("Waiting for map upload.")
                        .build(),
                },
            );
            ctx.cancel();
        }
    }

    async fn on_spawn(&self, ctx: &mut Context, _state: &mut PlayerState) {
        if is_waiting_for_map(ctx.server()) {
            ctx.cancel();
        }
    }

    async fn map_changed(&self, ctx: &mut Context, _map_name: &CubeString, _mode: GameMode) {
        let server = ctx.server();
        {
            let mut state = get_server_state(server).write().unwrap();
            state.map = None;
            state.waiting_for_map = true;
            state.traps.clear();
        }
        for cn in server.client_cns() {
            server.set_spectator(cn, true);
        }
    }

    async fn map(&self, ctx: &mut Context, data: &Bytes) {
        ctx.cancel();

        let cn = ctx.cn();
        let server = ctx.server();

        let is_spectator = server
            .players
            .try_get(cn)
            .map_or(false, |p| p.client_state == ClientState::Spectator);
        let is_master = server.get_privilege(cn) >= Privilege::Master;

        if is_spectator && !is_master {
            server.send_chan1(
                cn,
                &channel1::ServMsg {
                    text: "You must be unspectated to upload a map.".lossy_cubestring(),
                },
            );
            return;
        }

        match MapState::from_bytes(data) {
            Ok(state) => {
                let world_size = state.world.size;
                state.update_server_items(server);
                {
                    let mut s = get_server_state(server).write().unwrap();
                    s.map = Some(state);
                    s.waiting_for_map = false;
                }
                info!("cn {cn} uploaded a map (world_size={world_size}); state loaded");
                server.set_spectator(cn, false);
                server.send_chan1(
                    cn,
                    &channel1::ServMsg {
                        text: "Map received. You can now spawn and edit.".lossy_cubestring(),
                    },
                );
            }
            Err(e) => warn!("cn {cn} sent an invalid map: {e}"),
        }
    }

    async fn on_client_welcomed(&self, ctx: &mut Context) {
        let cn = ctx.cn();
        let server = ctx.server();
        if server.client_count() == 1 {
            server.set_privilege(cn, Privilege::Master);
            server.set_spectator(cn, false);
        }
        if send_map_to(server, cn) {
            info!("sent map state to cn {cn} after welcome");
        }
    }

    async fn get_map(&self, ctx: &mut Context, _packet: &channel1::GetMap) {
        let cn = ctx.cn();
        if send_map_to(ctx.server(), cn) {
            info!("sent map state to cn {cn} on N_GETMAP");
            ctx.cancel();
        }
    }

    async fn spectator(&self, ctx: &mut Context, packet: &channel1::SpectatorClient) {
        if !packet.val && is_waiting_for_map(ctx.server()) {
            ctx.server().send_chan1(
                ctx.cn(),
                &channel1::ServMsg {
                    text: "Waiting for map upload, /sendmap to proceed.".lossy_cubestring(),
                },
            );
            ctx.cancel();
        }
    }

    // ── Coop-edit map tracking ────────────────────────────────────────────────

    async fn edit_var(&self, ctx: &mut Context, packet: &channel1::EditVar) {
        with_map_state_mut(ctx, |state| state.handle_edit_var(packet));
    }

    async fn edit_tex(&self, ctx: &mut Context, packet: &channel1::EditTex) {
        with_map_state_mut(ctx, |state| state.handle_edit_tex(packet));
    }

    async fn edit_mat(&self, ctx: &mut Context, packet: &channel1::EditMat) {
        with_map_state_mut(ctx, |state| state.handle_edit_mat(packet));
    }

    async fn del_cube(&self, ctx: &mut Context, packet: &channel1::DelCube) {
        with_map_state_mut(ctx, |state| state.handle_del_cube(packet));
    }

    async fn replace(&self, ctx: &mut Context, packet: &channel1::Replace) {
        with_map_state_mut(ctx, |state| state.handle_replace(packet));
    }

    async fn edit_face(&self, ctx: &mut Context, packet: &channel1::EditFace) {
        with_map_state_mut(ctx, |state| state.handle_edit_face(packet));
    }

    async fn edit_ent(&self, ctx: &mut Context, packet: &channel1::EditEnt) {
        with_map_state_mut(ctx, |state| state.handle_edit_ent(packet));
    }
}
