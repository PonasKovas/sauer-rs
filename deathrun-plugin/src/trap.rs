use crate::map_state::{Trap, get_server_state};
use cube_string::IntoCubeStringLossy;
use ogz_rs::data::Material;
use sauer_map::Selection;
use sauer_proto::{
    Client, PacketSequence,
    channel1::{self, ServMsg},
    data::{EditFaceMode, Orient, SelInfo, Vec3},
};
use sauer_server::{Server, plugin::Context};

pub const BUTTON_SIZE: i32 = 8;
pub const BUTTON_FRONT_TEX: u16 = 248;
pub const BUTTON_SIDE_TEX: u16 = 242;
pub const BUTTON_COLOR: [f32; 3] = [0.2, 0.2, 0.2];

// ─── Intersection checks ──────────────────────────────────────────────────────

pub fn intersects_bounds(min: [i32; 3], max: [i32; 3], trap: &Trap) -> bool {
    let t_min = trap.button_pos;
    let t_max = [
        trap.button_pos[0] + trap.button_size,
        trap.button_pos[1] + trap.button_size,
        trap.button_pos[2] + trap.button_size,
    ];
    (0..3).all(|i| min[i] < t_max[i] && max[i] > t_min[i])
}

pub fn intersects_sel(sel: &SelInfo, trap: &Trap) -> bool {
    let min = [sel.o.x, sel.o.y, sel.o.z];
    let max = [
        min[0] + sel.s.x * sel.grid,
        min[1] + sel.s.y * sel.grid,
        min[2] + sel.s.z * sel.grid,
    ];
    intersects_bounds(min, max, trap)
}

pub fn intersects_edit_face(sel: &SelInfo, dir: i32, trap: &Trap) -> bool {
    let mut min = [sel.o.x, sel.o.y, sel.o.z];
    let mut max = [
        min[0] + sel.s.x * sel.grid,
        min[1] + sel.s.y * sel.grid,
        min[2] + sel.s.z * sel.grid,
    ];

    let d = (sel.orient as usize) >> 1;
    let expansion = dir.abs() * sel.grid;
    min[d] -= expansion;
    max[d] += expansion;

    intersects_bounds(min, max, trap)
}

// ─── Packet Generation Helpers ────────────────────────────────────────────────

type ButtonSetupPackets = (
    Client<channel1::EditFace>,
    Client<channel1::EditTex>,
    Client<channel1::EditTex>,
    Client<channel1::EditVSlot>,
    Client<channel1::EditVSlot>,
    Client<channel1::EditMat>,
    Client<channel1::EditVSlot>,
);

fn build_setup_packets(cn: i32, adj_sel: &SelInfo, fill_sel: &SelInfo) -> ButtonSetupPackets {
    let mut color_vslot = vec![7u8];
    for &f in &BUTTON_COLOR {
        color_vslot.extend_from_slice(&f.to_le_bytes());
    }
    color_vslot.push(0xFF);

    let mut scale_vslot = vec![1u8];
    scale_vslot.extend_from_slice(&2.0f32.to_le_bytes());
    scale_vslot.push(0xFF);

    let mut alpha_vslot = vec![6u8];
    alpha_vslot.extend_from_slice(&0.8f32.to_le_bytes());
    alpha_vslot.extend_from_slice(&0.8f32.to_le_bytes());
    alpha_vslot.push(0xFF);

    (
        Client {
            client_num: cn,
            packet: channel1::EditFace {
                sel: fill_sel.clone(),
                dir: -1,
                mode: EditFaceMode::Fill,
            },
        },
        Client {
            client_num: cn,
            packet: channel1::EditTex {
                sel: adj_sel.clone(),
                tex: BUTTON_SIDE_TEX as i32,
                all_faces: true,
                extra_data: vec![],
            },
        },
        Client {
            client_num: cn,
            packet: channel1::EditTex {
                sel: adj_sel.clone(),
                tex: BUTTON_FRONT_TEX as i32,
                all_faces: false,
                extra_data: vec![],
            },
        },
        Client {
            client_num: cn,
            packet: channel1::EditVSlot {
                sel: adj_sel.clone(),
                delta: 0,
                all_faces: true,
                extra_data: color_vslot,
            },
        },
        Client {
            client_num: cn,
            packet: channel1::EditVSlot {
                sel: adj_sel.clone(),
                delta: 0,
                all_faces: false,
                extra_data: scale_vslot,
            },
        },
        Client {
            client_num: cn,
            packet: channel1::EditMat {
                sel: adj_sel.clone(),
                mat: 1024,
                filter: -1,
            },
        },
        Client {
            client_num: cn,
            packet: channel1::EditVSlot {
                sel: adj_sel.clone(),
                delta: 0,
                all_faces: true,
                extra_data: alpha_vslot,
            },
        },
    )
}

fn corner_pushes(
    sel: &SelInfo,
    corner: i32,
    cn: i32,
) -> impl PacketSequence<sauer_proto::route::S2C<sauer_proto::route::Chan1>> {
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
    (
        pkt.clone(),
        pkt.clone(),
        pkt.clone(),
        pkt.clone(),
        pkt.clone(),
        pkt,
    )
}

fn get_selections(adj: [i32; 3], button_size: i32, hit_orient: u8) -> (SelInfo, SelInfo) {
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
    (adj_sel, fill_sel)
}

// ─── Automated rebuilding ─────────────────────────────────────────────────────

/// Instantaneously sends all the packets required to rebuild the button in a single atomic ENet packet.
fn send_atomic_rebuild(server: &Server, cn: u32, adj: [i32; 3], button_size: i32, hit_orient: u8) {
    let target_cn = cn as i32;
    let (adj_sel, fill_sel) = get_selections(adj, button_size, hit_orient);
    let setup = build_setup_packets(target_cn, &adj_sel, &fill_sel);

    server.broadcast_chan1((
        setup,
        corner_pushes(&adj_sel, 0, target_cn),
        corner_pushes(&adj_sel, 1, target_cn),
        corner_pushes(&adj_sel, 2, target_cn),
        corner_pushes(&adj_sel, 3, target_cn),
    ));
}

/// Restores trap to map tracking memory, and triggers atomic network sync.
pub fn check_and_rebuild_traps(ctx: &Context, intersects: impl Fn(&Trap) -> bool) {
    let server = ctx.server().clone();
    let cn = ctx.cn();
    let mut state = get_server_state(&server).write().unwrap();

    let to_rebuild: Vec<_> = state
        .traps
        .iter()
        .filter(|t| intersects(t))
        .map(|t| Trap {
            button_pos: t.button_pos,
            button_size: t.button_size,
            button_orient: t.button_orient,
        })
        .collect();

    if to_rebuild.is_empty() {
        return;
    }

    for trap in to_rebuild {
        if let Some(map) = state.map.as_mut() {
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
            map.invalidate_cache();
        }
        send_atomic_rebuild(
            &server,
            cn,
            trap.button_pos,
            trap.button_size,
            trap.button_orient,
        );
    }
}

// ─── Command Handlers ─────────────────────────────────────────────────────────

pub fn cmd_deltrap(ctx: &Context, hit_pt: [f32; 3]) {
    let cn = ctx.cn();
    let server = ctx.server();
    let mut state = get_server_state(server).write().unwrap();

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

    let (adj_sel, _) = get_selections(trap.button_pos, trap.button_size, trap.button_orient);
    server.broadcast_chan1(&Client {
        client_num: cn as i32,
        packet: channel1::DelCube { sel: adj_sel },
    });
}

pub fn cmd_newtrap(ctx: &Context, hit_pt: [f32; 3], hit_orient: u8) {
    let cn = ctx.cn();
    let server = ctx.server().clone();
    let mut state = get_server_state(&server).write().unwrap();

    let map = match state.map.as_mut() {
        None => return,
        Some(m) => m,
    };

    let d = (hit_orient >> 1) as usize;
    let dc = (hit_orient & 1) as usize;
    let r_axis = [1usize, 2, 0][d];
    let c_axis = [2usize, 0, 1][d];

    let snap = |v: f32| -> i32 { (v / BUTTON_SIZE as f32).floor() as i32 * BUTTON_SIZE };
    let snap_d = if dc == 0 {
        (hit_pt[d] / BUTTON_SIZE as f32).floor() as i32 * BUTTON_SIZE
    } else {
        ((hit_pt[d] - 0.001) / BUTTON_SIZE as f32).ceil() as i32 * BUTTON_SIZE
    };

    let mut adj = [0i32; 3];
    adj[d] = if dc == 0 {
        snap_d - BUTTON_SIZE
    } else {
        snap_d
    };
    adj[r_axis] = snap(hit_pt[r_axis]);
    adj[c_axis] = snap(hit_pt[c_axis]);

    let ws = map.world.size;
    if adj.iter().any(|&v| v < 0 || v >= ws)
        || adj[r_axis] + BUTTON_SIZE > ws
        || adj[c_axis] + BUTTON_SIZE > ws
    {
        return;
    }

    let sel = Selection {
        o: adj,
        s: [1, 1, 1],
        grid: BUTTON_SIZE,
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
        button_size: BUTTON_SIZE,
        button_orient: hit_orient,
    });
    drop(state);

    // Re-use animation exclusively for new placements for the cosmetic factor
    let target_cn = cn as i32;
    let (adj_sel, fill_sel) = get_selections(adj, BUTTON_SIZE, hit_orient);
    let setup = build_setup_packets(target_cn, &adj_sel, &fill_sel);

    tokio::spawn(async move {
        use tokio::time::{Duration, sleep};
        server.broadcast_chan1(setup);

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
