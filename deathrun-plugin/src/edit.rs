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
    Client, channel0,
    channel1::{self, EditMode, ServMsg},
    data::{
        ClientState, Direction, EditFaceMode, GameMode, Orient, PlayerState, Privilege, SelInfo,
        Vec3,
    },
};
use sauer_server::{
    Server,
    plugin::{Context, Plugin},
};
use std::sync::{Arc, RwLock};
use tracing::{info, warn};

const BUTTON_SIZE: i32 = 8;
const BUTTON_FRONT_TEX: u16 = 248;
const BUTTON_SIDE_TEX: u16 = 242;
const BUTTON_COLOR: [f32; 3] = [0.2, 0.2, 0.2];

// ─── Sauerbraten internal variable length 'putint' parser ─────────────────────

fn getint(bytes: &[u8], pos: &mut usize) -> Option<i32> {
    if *pos >= bytes.len() {
        return None;
    }
    let c = bytes[*pos] as i8;
    *pos += 1;
    if c == -128 {
        if *pos + 1 >= bytes.len() {
            return None;
        }
        let b0 = bytes[*pos] as u8 as i32;
        let b1 = bytes[*pos + 1] as i8 as i32;
        *pos += 2;
        Some(b0 | (b1 << 8))
    } else if c == -127 {
        if *pos + 3 >= bytes.len() {
            return None;
        }
        let b0 = bytes[*pos] as u8 as i32;
        let b1 = bytes[*pos + 1] as u8 as i32;
        let b2 = bytes[*pos + 2] as u8 as i32;
        let b3 = bytes[*pos + 3] as i8 as i32;
        *pos += 4;
        Some(b0 | (b1 << 8) | (b2 << 16) | (b3 << 24))
    } else {
        Some(c as i32)
    }
}

fn skip_undo_ents(bytes: &[u8], pos: &mut usize) -> Option<()> {
    let numents = getint(bytes, pos)?;
    for _ in 0..numents {
        getint(bytes, pos)?; // id
        getint(bytes, pos)?; // type
        getint(bytes, pos)?; // x
        getint(bytes, pos)?; // y
        getint(bytes, pos)?; // z
        for _ in 0..5 {
            getint(bytes, pos)?; // attr 0-4
        }
    }
    Some(())
}

fn get_undo_redo_bounds(data: &[u8]) -> Option<([i32; 3], [i32; 3], i32)> {
    use flate2::read::ZlibDecoder;
    use std::io::Read;
    let mut decoder = ZlibDecoder::new(data);
    let mut decompressed = Vec::new();
    decoder
        .by_ref()
        .take(4096)
        .read_to_end(&mut decompressed)
        .ok()?;

    let mut pos = 0;
    skip_undo_ents(&decompressed, &mut pos)?;

    if pos >= decompressed.len() {
        return None; // Entity-only undo, no geometry block bounds
    }

    let x = getint(&decompressed, &mut pos)?;
    let y = getint(&decompressed, &mut pos)?;
    let z = getint(&decompressed, &mut pos)?;
    let xs = getint(&decompressed, &mut pos)?;
    let ys = getint(&decompressed, &mut pos)?;
    let zs = getint(&decompressed, &mut pos)?;
    let grid = getint(&decompressed, &mut pos)?;

    Some(([x, y, z], [xs, ys, zs], grid))
}

fn get_clipboard_bounds(data: &[u8]) -> Option<([i32; 3], [i32; 3], i32)> {
    use flate2::read::ZlibDecoder;
    use std::io::Read;
    let mut decoder = ZlibDecoder::new(data);
    let mut decompressed = Vec::new();
    decoder
        .by_ref()
        .take(256)
        .read_to_end(&mut decompressed)
        .ok()?;

    let mut pos = 0;
    let x = getint(&decompressed, &mut pos)?;
    let y = getint(&decompressed, &mut pos)?;
    let z = getint(&decompressed, &mut pos)?;
    let xs = getint(&decompressed, &mut pos)?;
    let ys = getint(&decompressed, &mut pos)?;
    let zs = getint(&decompressed, &mut pos)?;
    let grid = getint(&decompressed, &mut pos)?;

    Some(([x, y, z], [xs, ys, zs], grid))
}

// ─── Intersection checks ──────────────────────────────────────────────────────

fn intersects_trap(min: [i32; 3], max: [i32; 3], trap: &Trap) -> bool {
    let t_min = trap.button_pos;
    let t_max = [
        trap.button_pos[0] + trap.button_size,
        trap.button_pos[1] + trap.button_size,
        trap.button_pos[2] + trap.button_size,
    ];

    (0..3).all(|i| min[i] < t_max[i] && max[i] > t_min[i])
}

fn intersects_sel(sel: &SelInfo, trap: &Trap) -> bool {
    let min = [sel.o.x, sel.o.y, sel.o.z];
    let max = [
        sel.o.x + sel.s.x * sel.grid,
        sel.o.y + sel.s.y * sel.grid,
        sel.o.z + sel.s.z * sel.grid,
    ];
    intersects_trap(min, max, trap)
}

fn intersects_edit_face(sel: &SelInfo, dir: i32, trap: &Trap) -> bool {
    let mut min = [sel.o.x, sel.o.y, sel.o.z];
    let mut max = [
        sel.o.x + sel.s.x * sel.grid,
        sel.o.y + sel.s.y * sel.grid,
        sel.o.z + sel.s.z * sel.grid,
    ];

    // Push/Pull operations expand into the block *adjacent* to the face.
    // Expand the bounds on the face's normal axis to safely catch nearby traps.
    let d = (sel.orient as usize) >> 1;
    let expansion = dir.abs() * sel.grid;
    min[d] -= expansion;
    max[d] += expansion;

    intersects_trap(min, max, trap)
}

// ─── Rebuilding logic ─────────────────────────────────────────────────────────

/// Spawns the cosmetic corner-pushing animation sequentially. Only used for the #newtrap command.
fn spawn_trap_animation(
    server: Arc<Server>,
    cn: u32,
    adj: [i32; 3],
    button_size: i32,
    hit_orient: u8,
) {
    let d = (hit_orient >> 1) as usize;
    let dc = (hit_orient & 1) as usize;
    let mut editf_o = adj;
    editf_o[d] = if dc == 0 {
        adj[d] + button_size
    } else {
        adj[d] - button_size
    };

    let proto_orient = match hit_orient {
        0 => Orient::Left,
        1 => Orient::Right,
        2 => Orient::Back,
        3 => Orient::Front,
        4 => Orient::Bottom,
        _ => Orient::Top,
    };

    let adj_sel = SelInfo {
        o: Vec3 {
            x: adj[0],
            y: adj[1],
            z: adj[2],
        },
        s: Vec3 { x: 1, y: 1, z: 1 },
        grid: button_size,
        orient: proto_orient,
        cx: 0,
        cxs: 2,
        cy: 0,
        cys: 2,
        corner: 0,
    };
    let fill_sel = SelInfo {
        o: Vec3 {
            x: editf_o[0],
            y: editf_o[1],
            z: editf_o[2],
        },
        ..adj_sel.clone()
    };

    tokio::spawn(async move {
        use std::time::Duration;
        use tokio::time::sleep;

        let target_cn = cn as i32;

        server.broadcast_chan1(&Client {
            client_num: target_cn,
            packet: channel1::EditFace {
                sel: fill_sel,
                dir: -1,
                mode: EditFaceMode::Fill,
            },
        });
        server.broadcast_chan1(&Client {
            client_num: target_cn,
            packet: channel1::EditTex {
                sel: adj_sel.clone(),
                tex: BUTTON_SIDE_TEX as i32,
                all_faces: true,
                extra_data: vec![],
            },
        });
        server.broadcast_chan1(&Client {
            client_num: target_cn,
            packet: channel1::EditTex {
                sel: adj_sel.clone(),
                tex: BUTTON_FRONT_TEX as i32,
                all_faces: false,
                extra_data: vec![],
            },
        });
        let color_vslot = {
            let mut d = vec![7u8]; // VSLOT_COLOR
            for &f in &BUTTON_COLOR {
                d.extend_from_slice(&f.to_le_bytes());
            }
            d.push(0xFF);
            d
        };
        server.broadcast_chan1(&Client {
            client_num: target_cn,
            packet: channel1::EditVSlot {
                sel: adj_sel.clone(),
                delta: 0,
                all_faces: true,
                extra_data: color_vslot,
            },
        });
        let scale_vslot = {
            let mut d = vec![1u8]; // VSLOT_SCALE
            d.extend_from_slice(&2.0f32.to_le_bytes());
            d.push(0xFF);
            d
        };
        server.broadcast_chan1(&Client {
            client_num: target_cn,
            packet: channel1::EditVSlot {
                sel: adj_sel.clone(),
                delta: 0,
                all_faces: false,
                extra_data: scale_vslot,
            },
        });
        server.broadcast_chan1(&Client {
            client_num: target_cn,
            packet: channel1::EditMat {
                sel: adj_sel.clone(),
                mat: 1024,
                filter: -1,
            },
        });
        let alpha_vslot = {
            let mut d = vec![6u8]; // VSLOT_ALPHA
            d.extend_from_slice(&0.8f32.to_le_bytes());
            d.extend_from_slice(&0.8f32.to_le_bytes());
            d.push(0xFF);
            d
        };
        server.broadcast_chan1(&Client {
            client_num: target_cn,
            packet: channel1::EditVSlot {
                sel: adj_sel.clone(),
                delta: 0,
                all_faces: true,
                extra_data: alpha_vslot,
            },
        });

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
                client_num: target_cn,
                packet: channel1::EditFace {
                    sel: SelInfo {
                        corner,
                        ..adj_sel.clone()
                    },
                    dir: 1,
                    mode: EditFaceMode::Corner,
                },
            });
        }
    });
}

/// Instantaneously sends all the packets required to rebuild the button in a single atomic ENet packet
/// utilizing the `sauer_proto` multi-tuple sequence combinator logic.
fn send_atomic_rebuild(server: &Server, cn: u32, adj: [i32; 3], button_size: i32, hit_orient: u8) {
    let d = (hit_orient >> 1) as usize;
    let dc = (hit_orient & 1) as usize;
    let mut editf_o = adj;
    editf_o[d] = if dc == 0 {
        adj[d] + button_size
    } else {
        adj[d] - button_size
    };

    let proto_orient = match hit_orient {
        0 => Orient::Left,
        1 => Orient::Right,
        2 => Orient::Back,
        3 => Orient::Front,
        4 => Orient::Bottom,
        _ => Orient::Top,
    };

    let adj_sel = SelInfo {
        o: Vec3 {
            x: adj[0],
            y: adj[1],
            z: adj[2],
        },
        s: Vec3 { x: 1, y: 1, z: 1 },
        grid: button_size,
        orient: proto_orient,
        cx: 0,
        cxs: 2,
        cy: 0,
        cys: 2,
        corner: 0,
    };
    let fill_sel = SelInfo {
        o: Vec3 {
            x: editf_o[0],
            y: editf_o[1],
            z: editf_o[2],
        },
        ..adj_sel.clone()
    };

    let color_vslot = {
        let mut d = vec![7u8];
        for &f in &BUTTON_COLOR {
            d.extend_from_slice(&f.to_le_bytes());
        }
        d.push(0xFF);
        d
    };
    let scale_vslot = {
        let mut d = vec![1u8];
        d.extend_from_slice(&2.0f32.to_le_bytes());
        d.push(0xFF);
        d
    };
    let alpha_vslot = {
        let mut d = vec![6u8];
        d.extend_from_slice(&0.8f32.to_le_bytes());
        d.extend_from_slice(&0.8f32.to_le_bytes());
        d.push(0xFF);
        d
    };

    let target_cn = cn as i32;

    let p1 = Client {
        client_num: target_cn,
        packet: channel1::EditFace {
            sel: fill_sel,
            dir: -1,
            mode: EditFaceMode::Fill,
        },
    };
    let p2 = Client {
        client_num: target_cn,
        packet: channel1::EditTex {
            sel: adj_sel.clone(),
            tex: BUTTON_SIDE_TEX as i32,
            all_faces: true,
            extra_data: vec![],
        },
    };
    let p3 = Client {
        client_num: target_cn,
        packet: channel1::EditTex {
            sel: adj_sel.clone(),
            tex: BUTTON_FRONT_TEX as i32,
            all_faces: false,
            extra_data: vec![],
        },
    };
    let p4 = Client {
        client_num: target_cn,
        packet: channel1::EditVSlot {
            sel: adj_sel.clone(),
            delta: 0,
            all_faces: true,
            extra_data: color_vslot,
        },
    };
    let p5 = Client {
        client_num: target_cn,
        packet: channel1::EditVSlot {
            sel: adj_sel.clone(),
            delta: 0,
            all_faces: false,
            extra_data: scale_vslot,
        },
    };
    let p6 = Client {
        client_num: target_cn,
        packet: channel1::EditMat {
            sel: adj_sel.clone(),
            mat: 1024,
            filter: -1,
        },
    };
    let p7 = Client {
        client_num: target_cn,
        packet: channel1::EditVSlot {
            sel: adj_sel.clone(),
            delta: 0,
            all_faces: true,
            extra_data: alpha_vslot,
        },
    };

    // Grouping the 6 necessary pushes to deform a corner into a single sub-sequence payload
    fn corner_pushes(
        sel: &SelInfo,
        corner: i32,
        cn: i32,
    ) -> impl sauer_proto::PacketSequence<sauer_proto::route::S2C<sauer_proto::route::Chan1>> {
        let pkt = Client {
            client_num: cn,
            packet: channel1::EditFace {
                sel: SelInfo {
                    corner,
                    ..sel.clone()
                },
                dir: 1,
                mode: EditFaceMode::Corner,
            },
        };
        // 6 consecutive pushes forms a perfect inner slope vertex.
        (
            pkt.clone(),
            pkt.clone(),
            pkt.clone(),
            pkt.clone(),
            pkt.clone(),
            pkt,
        )
    }

    // Atomic broadcast (all packets combined into a nested tuple are written to the identical ENet datagram simultaneously)
    server.broadcast_chan1((
        (p1, p2, p3, p4, p5, p6, p7),
        corner_pushes(&adj_sel, 0, target_cn),
        corner_pushes(&adj_sel, 1, target_cn),
        corner_pushes(&adj_sel, 2, target_cn),
        corner_pushes(&adj_sel, 3, target_cn),
    ));
}

fn check_and_rebuild_traps(ctx: &mut Context, intersects: impl Fn(&Trap) -> bool) {
    let server = ctx.server().clone();
    let cn = ctx.cn();
    let mut state = get_server_state(&server).write().unwrap();

    let mut to_rebuild = Vec::new();
    for trap in &state.traps {
        if intersects(trap) {
            to_rebuild.push(Trap {
                button_pos: trap.button_pos,
                button_size: trap.button_size,
                button_orient: trap.button_orient,
            });
        }
    }

    if to_rebuild.is_empty() {
        return;
    }

    for trap in to_rebuild {
        let adj = trap.button_pos;
        let button_size = trap.button_size;
        let hit_orient = trap.button_orient;

        if let Some(map) = state.map.as_mut() {
            let sel = Selection {
                o: adj,
                s: [1, 1, 1],
                grid: button_size,
                orient: hit_orient,
                cx: 0,
                cxs: 2,
                cy: 0,
                cys: 2,
                corner: 0,
            };

            // Instantly reinstate the full bounds of the button geometry directly to the server's tracking memory
            map.world.fill(&sel, true);
            map.world.set_texture(&sel, BUTTON_SIDE_TEX, true);
            map.world.set_texture(&sel, BUTTON_FRONT_TEX, false);
            map.world.set_material(
                &sel,
                Material {
                    alpha: true,
                    ..Default::default()
                },
            );

            // Equivalent to 24 corner pushes (mode 0 == face push). 6x face push resolves the exact same vertices locally.
            for _ in 0..6 {
                map.world.edit_face(&sel, 1, 0);
            }
            map.invalidate_cache();
        }

        // Send a single completely synchronized network datagram payload to clients to repair the desync securely
        send_atomic_rebuild(&server, cn, adj, button_size, hit_orient);
    }
}

// ─── Plugin implementation ────────────────────────────────────────────────────

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
                o: trap.button_pos,
                s: [1, 1, 1],
                grid: trap.button_size,
                orient: trap.button_orient,
                cx: 0,
                cxs: 2,
                cy: 0,
                cys: 2,
                corner: 0,
            };
            map.world.fill(&sel, false);
            drop(state);

            let proto_orient = match trap.button_orient {
                0 => Orient::Left,
                1 => Orient::Right,
                2 => Orient::Back,
                3 => Orient::Front,
                4 => Orient::Bottom,
                _ => Orient::Top,
            };
            server.broadcast_chan1(&Client {
                client_num: cn as i32,
                packet: channel1::DelCube {
                    sel: SelInfo {
                        o: Vec3 {
                            x: trap.button_pos[0],
                            y: trap.button_pos[1],
                            z: trap.button_pos[2],
                        },
                        s: Vec3 { x: 1, y: 1, z: 1 },
                        grid: trap.button_size,
                        orient: proto_orient,
                        cx: 0,
                        cxs: 2,
                        cy: 0,
                        cys: 2,
                        corner: 0,
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
        adj[d] = if dc == 0 {
            snap_d - button_size
        } else {
            snap_d
        };
        adj[r_axis] = snap(hit_pt[r_axis]);
        adj[c_axis] = snap(hit_pt[c_axis]);

        let ws = map.world.size;
        if adj.iter().any(|&v| v < 0 || v >= ws)
            || adj[r_axis] + button_size > ws
            || adj[c_axis] + button_size > ws
        {
            return;
        }

        let sel = Selection {
            o: adj,
            s: [1, 1, 1],
            grid: button_size,
            orient: hit_orient,
            cx: 0,
            cxs: 2,
            cy: 0,
            cys: 2,
            corner: 0,
        };
        map.world.fill(&sel, true);
        map.world.set_texture(&sel, BUTTON_SIDE_TEX, true);
        map.world.set_texture(&sel, BUTTON_FRONT_TEX, false);
        map.world.set_material(
            &sel,
            Material {
                alpha: true,
                ..Default::default()
            },
        );
        for _ in 0..6 {
            map.world.edit_face(&sel, 1, 0);
        }

        state.traps.push(Trap {
            button_pos: adj,
            button_size,
            button_orient: hit_orient,
        });
        drop(state);

        spawn_trap_animation(server, cn, adj, button_size, hit_orient);
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

    // ── Coop-edit map tracking + Trap immunities ────────────────────────────────────────────────

    async fn edit_tex(&self, ctx: &mut Context, packet: &channel1::EditTex) {
        with_map_state_mut(ctx, |state| state.handle_edit_tex(packet));
        check_and_rebuild_traps(ctx, |trap| intersects_sel(&packet.sel, trap));
    }

    async fn edit_mat(&self, ctx: &mut Context, packet: &channel1::EditMat) {
        with_map_state_mut(ctx, |state| state.handle_edit_mat(packet));
        check_and_rebuild_traps(ctx, |trap| intersects_sel(&packet.sel, trap));
    }

    async fn del_cube(&self, ctx: &mut Context, packet: &channel1::DelCube) {
        with_map_state_mut(ctx, |state| state.handle_del_cube(packet));
        check_and_rebuild_traps(ctx, |trap| intersects_sel(&packet.sel, trap));
    }

    async fn replace(&self, ctx: &mut Context, packet: &channel1::Replace) {
        with_map_state_mut(ctx, |state| state.handle_replace(packet));
        check_and_rebuild_traps(ctx, |trap| {
            if packet.in_sel == 0 {
                true // Matches everything because it targets the entire map root
            } else {
                intersects_sel(&packet.sel, trap)
            }
        });
    }

    async fn edit_face(&self, ctx: &mut Context, packet: &channel1::EditFace) {
        with_map_state_mut(ctx, |state| state.handle_edit_face(packet));
        check_and_rebuild_traps(ctx, |trap| {
            intersects_edit_face(&packet.sel, packet.dir, trap)
        });
    }

    async fn edit_ent(&self, ctx: &mut Context, packet: &channel1::EditEnt) {
        with_map_state_mut(ctx, |state| state.handle_edit_ent(packet));
    }

    async fn edit_v_slot(&self, ctx: &mut Context, packet: &channel1::EditVSlot) {
        check_and_rebuild_traps(ctx, |trap| intersects_sel(&packet.sel, trap));
    }

    async fn flip(&self, ctx: &mut Context, packet: &channel1::Flip) {
        check_and_rebuild_traps(ctx, |trap| intersects_sel(&packet.sel, trap));
    }

    async fn rotate(&self, ctx: &mut Context, packet: &channel1::Rotate) {
        check_and_rebuild_traps(ctx, |trap| intersects_sel(&packet.sel, trap));
    }

    async fn paste(&self, ctx: &mut Context, packet: &channel1::PasteClient) {
        let cn = ctx.cn();
        let server = ctx.server().clone();
        let mut clipboard_bounds = None;
        if let Some(p) = server.players.try_get(cn) {
            if let Some(cb) = &p.clipboard {
                clipboard_bounds = get_clipboard_bounds(&cb.data);
            }
        }

        if let Some((_, cb_s, cb_grid)) = clipboard_bounds {
            let min = [packet.sel.o.x, packet.sel.o.y, packet.sel.o.z];
            let max = [
                min[0] + cb_s[0] * cb_grid,
                min[1] + cb_s[1] * cb_grid,
                min[2] + cb_s[2] * cb_grid,
            ];
            check_and_rebuild_traps(ctx, |trap| intersects_trap(min, max, trap));
        } else {
            check_and_rebuild_traps(ctx, |_| true);
        }
    }

    async fn undo(&self, ctx: &mut Context, packet: &channel1::UndoClient) {
        if let Some((o, s, grid)) = get_undo_redo_bounds(&packet.data) {
            let max = [o[0] + s[0] * grid, o[1] + s[1] * grid, o[2] + s[2] * grid];
            check_and_rebuild_traps(ctx, |trap| intersects_trap(o, max, trap));
        } else {
            check_and_rebuild_traps(ctx, |_| true);
        }
    }

    async fn redo(&self, ctx: &mut Context, packet: &channel1::RedoClient) {
        if let Some((o, s, grid)) = get_undo_redo_bounds(&packet.data) {
            let max = [o[0] + s[0] * grid, o[1] + s[1] * grid, o[2] + s[2] * grid];
            check_and_rebuild_traps(ctx, |trap| intersects_trap(o, max, trap));
        } else {
            check_and_rebuild_traps(ctx, |_| true);
        }
    }
}
