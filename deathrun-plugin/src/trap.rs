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

// ─── Recording State ──────────────────────────────────────────────────────────

#[derive(Clone)]
pub enum RecordedPacket {
    EditFace(channel1::EditFace),
    EditTex(channel1::EditTex),
    EditMat(channel1::EditMat),
    DelCube(channel1::DelCube),
    EditVSlot(channel1::EditVSlot),
    Replace(channel1::Replace),
    Flip(channel1::Flip),
    Rotate(channel1::Rotate),
    EditEnt(channel1::EditEnt),
}

pub struct TrapEditState {
    pub trap_idx: usize,
    pub original_map: Box<crate::map_state::MapState>,
    pub affected_min: [i32; 3],
    pub affected_max: [i32; 3],
    pub affected_entities: std::collections::HashSet<usize>,
}

impl TrapEditState {
    pub fn add_bounds(&mut self, min: [i32; 3], max: [i32; 3]) {
        for i in 0..3 {
            if min[i] < self.affected_min[i] {
                self.affected_min[i] = min[i];
            }
            if max[i] > self.affected_max[i] {
                self.affected_max[i] = max[i];
            }
        }
    }

    pub fn add_sel(&mut self, sel: &SelInfo) {
        let min = [sel.o.x, sel.o.y, sel.o.z];
        let max = [
            min[0] + sel.s.x * sel.grid,
            min[1] + sel.s.y * sel.grid,
            min[2] + sel.s.z * sel.grid,
        ];
        self.add_bounds(min, max);
    }

    pub fn add_edit_face(&mut self, sel: &SelInfo, dir: i32) {
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
        self.add_bounds(min, max);
    }
}

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

fn apply_internal_vslots(map: &mut crate::map_state::MapState, sel: &SelInfo) {
    let putint = |v: &mut Vec<u8>, n: i32| {
        if n < 128 && n > -127 {
            v.push(n as u8);
        } else if n < 0x8000 && n >= -0x8000 {
            v.push(0x80);
            v.extend_from_slice(&(n as i16).to_le_bytes());
        } else {
            v.push(0x81);
            v.extend_from_slice(&n.to_le_bytes());
        }
    };

    let mut data1 = Vec::new();
    putint(&mut data1, (1 << 6) | (1 << 7));
    data1.extend_from_slice(&0.8f32.to_le_bytes());
    data1.extend_from_slice(&0.8f32.to_le_bytes());
    for &f in &BUTTON_COLOR {
        data1.extend_from_slice(&f.to_le_bytes());
    }
    map.handle_edit_v_slot(&channel1::EditVSlot {
        sel: sel.clone(),
        delta: 0,
        all_faces: true,
        extra_data: data1,
    });

    let mut data2 = Vec::new();
    putint(&mut data2, (1 << 1) | (1 << 6) | (1 << 7));
    data2.extend_from_slice(&2.0f32.to_le_bytes());
    data2.extend_from_slice(&0.8f32.to_le_bytes());
    data2.extend_from_slice(&0.8f32.to_le_bytes());
    for &f in &BUTTON_COLOR {
        data2.extend_from_slice(&f.to_le_bytes());
    }
    map.handle_edit_v_slot(&channel1::EditVSlot {
        sel: sel.clone(),
        delta: 0,
        all_faces: false,
        extra_data: data2,
    });
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
            trigger_clipboard: t.trigger_clipboard.clone(),
            trigger_unpack_len: t.trigger_unpack_len,
            trigger_entities: t.trigger_entities.clone(),
            restore_clipboard: t.restore_clipboard.clone(),
            restore_unpack_len: t.restore_unpack_len,
            original_entities: t.original_entities.clone(),
            clip_pos: t.clip_pos,
            clip_grid: t.clip_grid,
            affected_min: t.affected_min,
            affected_max: t.affected_max,
            is_active: t.is_active,
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

            for corner in 0..4 {
                let mut csel = sel.clone();
                csel.corner = corner;
                for _ in 0..6 {
                    map.world.edit_face(&csel, 1, 2);
                }
            }

            let (adj_sel, _) =
                get_selections(trap.button_pos, trap.button_size, trap.button_orient);
            apply_internal_vslots(map, &adj_sel);

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

    for corner in 0..4 {
        let mut csel = sel.clone();
        csel.corner = corner;
        for _ in 0..6 {
            map.world.edit_face(&csel, 1, 2);
        }
    }

    let target_cn = cn as i32;
    let (adj_sel, fill_sel) = get_selections(adj, BUTTON_SIZE, hit_orient);
    let setup = build_setup_packets(target_cn, &adj_sel, &fill_sel);

    apply_internal_vslots(map, &adj_sel);

    state.traps.push(Trap {
        button_pos: adj,
        button_size: BUTTON_SIZE,
        button_orient: hit_orient,
        trigger_clipboard: Vec::new(),
        trigger_unpack_len: 0,
        trigger_entities: std::collections::HashMap::new(),
        restore_clipboard: Vec::new(),
        restore_unpack_len: 0,
        original_entities: std::collections::HashMap::new(),
        clip_pos: [0; 3],
        clip_grid: 1,
        affected_min: [i32::MAX; 3],
        affected_max: [i32::MIN; 3],
        is_active: false,
    });
    drop(state);

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

// ─── Recording & Replaying Logic ─────────────────────────────────────────────

pub fn cmd_settrap(ctx: &Context, hit_pt: [f32; 3]) {
    let cn = ctx.cn();
    let server = ctx.server();
    let mut state = get_server_state(server).write().unwrap();

    if state.trap_edit.is_some() {
        server.send_chan1(
            cn,
            &ServMsg {
                text: "A trap is already being edited. Use #endtrap first.".lossy_cubestring(),
            },
        );
        return;
    }

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

    let original_map = state.map.clone().unwrap();

    state.trap_edit = Some(TrapEditState {
        trap_idx,
        original_map: Box::new(original_map),
        affected_min: [i32::MAX; 3],
        affected_max: [i32::MIN; 3],
        affected_entities: std::collections::HashSet::new(),
    });

    server.broadcast_chan1(&ServMsg {
        text: format!(
            "Trap edit mode started for trap {}. Type #endtrap to finish and save.",
            trap_idx
        )
        .lossy_cubestring(),
    });
}


pub fn cmd_endtrap(ctx: &Context) {
    let server = ctx.server();
    let mut state = get_server_state(server).write().unwrap();

    let edit_state = match state.trap_edit.take() {
        Some(e) => e,
        None => {
            server.send_chan1(
                ctx.cn(),
                &ServMsg {
                    text: "No trap is currently being edited.".lossy_cubestring(),
                },
            );
            return;
        }
    };

    // Compute the tight bounding box of what actually changed between original and current map.
    // This avoids inflated bounds from edit selections that touched unchanged geometry.
    let current_map = state.map.as_ref().unwrap();
    let world_size = current_map.world.size;
    let mut diff_min = [i32::MAX; 3];
    let mut diff_max = [i32::MIN; 3];
    crate::recreate::diff_bounds(
        &edit_state.original_map.world.root,
        &current_map.world.root,
        [0, 0, 0],
        world_size,
        &mut diff_min,
        &mut diff_max,
    );

    if diff_min[0] == i32::MAX {
        server.broadcast_chan1(&ServMsg {
            text: "Trap edit ended with no geometry changes detected.".lossy_cubestring(),
        });
        return;
    }

    let min = diff_min;
    let max = diff_max;

    eprintln!(
        "[trap] diff bounds: {:?}..{:?} (spans {}x{}x{})",
        min, max, max[0]-min[0], max[1]-min[1], max[2]-min[2]
    );

    // Generate ONE restore clipboard (from original map, pre-edit state)
    let (clip_pos, clip_grid, restore_unpack_len, restore_compressed) =
        crate::recreate::generate_region_clipboard(
            &edit_state.original_map.world.root,
            world_size,
            min,
            max,
        );

    // Generate ONE trigger clipboard (from CURRENT map, post-edit state)
    let (_, _, trigger_unpack_len, trigger_compressed) =
        crate::recreate::generate_region_clipboard(
            &current_map.world.root,
            world_size,
            min,
            max,
        );

    let mut original_entities = std::collections::HashMap::new();
    let mut trigger_entities = std::collections::HashMap::new();
    for &ent_id in &edit_state.affected_entities {
        // Original entity
        if let Some(ent) = edit_state.original_map.world.entities.get(ent_id) {
            original_entities.insert(ent_id, ent.clone());
        } else {
            original_entities.insert(
                ent_id,
                ogz_rs::data::Entity {
                    position: [0.0, 0.0, 0.0],
                    attrs: [0; 5],
                    ent_type: ogz_rs::data::EntityType::Empty,
                    extra_data: vec![],
                },
            );
        }

        // Trigger entity
        if let Some(ent) = current_map.world.entities.get(ent_id) {
            trigger_entities.insert(ent_id, ent.clone());
        } else {
            trigger_entities.insert(
                ent_id,
                ogz_rs::data::Entity {
                    position: [0.0, 0.0, 0.0],
                    attrs: [0; 5],
                    ent_type: ogz_rs::data::EntityType::Empty,
                    extra_data: vec![],
                },
            );
        }
    }

    let trap = &mut state.traps[edit_state.trap_idx];
    trap.trigger_clipboard = trigger_compressed;
    trap.trigger_unpack_len = trigger_unpack_len;
    trap.trigger_entities = trigger_entities;
    trap.restore_clipboard = restore_compressed.clone();
    trap.restore_unpack_len = restore_unpack_len;
    trap.original_entities = original_entities;
    trap.clip_pos = clip_pos;
    trap.clip_grid = clip_grid;
    trap.affected_min = min;
    trap.affected_max = max;

    let target_cn = ctx.cn() as i32;

    // Delete the affected region then paste the single restore clipboard
    let del_sel = SelInfo {
        o: sauer_proto::data::Vec3 { x: min[0], y: min[1], z: min[2] },
        s: sauer_proto::data::Vec3 {
            x: max[0] - min[0],
            y: max[1] - min[1],
            z: max[2] - min[2],
        },
        grid: 1,
        orient: sauer_proto::data::Orient::Left,
        cx: 0, cxs: 0, cy: 0, cys: 0, corner: 0,
    };
    server.broadcast_chan1(&Client {
        client_num: target_cn,
        packet: channel1::DelCube { sel: del_sel },
    });
    server.broadcast_chan1(&channel1::ClipboardServer {
        client_num: target_cn,
        unpack_len: restore_unpack_len,
        data: restore_compressed,
    });
    server.broadcast_chan1(&Client {
        client_num: target_cn,
        packet: channel1::PasteServer {
            sel: SelInfo {
                o: sauer_proto::data::Vec3 { x: clip_pos[0], y: clip_pos[1], z: clip_pos[2] },
                s: sauer_proto::data::Vec3 { x: 1, y: 1, z: 1 },
                grid: clip_grid,
                orient: sauer_proto::data::Orient::Left,
                cx: 0, cxs: 1, cy: 0, cys: 1, corner: 0,
            },
        },
    });

    for (&ent_id, ent) in &trap.original_entities {
        let ent_type = sauer_proto::data::EntityType::try_from(u8::from(ent.ent_type) as i32)
            .unwrap_or(sauer_proto::data::EntityType::Empty);
        server.broadcast_chan1(&Client {
            client_num: target_cn,
            packet: channel1::EditEnt {
                ent_id: ent_id as i32,
                pos: sauer_proto::data::Vec3 {
                    x: ent.position[0],
                    y: ent.position[1],
                    z: ent.position[2],
                },
                type_: ent_type,
                attrs: ent.attrs.map(|x| x as i32),
            },
        });
    }

    state.map = Some(*edit_state.original_map);

    server.broadcast_chan1(&ServMsg {
        text: "Trap recorded and area reverted successfully.".lossy_cubestring(),
    });
}

pub fn trigger_trap(server: &std::sync::Arc<Server>, trap_idx: usize, actor_cn: u32) {
    let mut state = get_server_state(server).write().unwrap();
    if trap_idx >= state.traps.len() {
        return;
    }
    let trap = &mut state.traps[trap_idx];

    if trap.is_active {
        return;
    }
    if trap.trigger_clipboard.is_empty() && trap.trigger_entities.is_empty() {
        return;
    }

    trap.is_active = true;

    let trigger_clipboard = trap.trigger_clipboard.clone();
    let trigger_unpack_len = trap.trigger_unpack_len;
    let trigger_entities = trap.trigger_entities.clone();
    let restore_clipboard = trap.restore_clipboard.clone();
    let restore_unpack_len = trap.restore_unpack_len;
    let original_entities = trap.original_entities.clone();
    let clip_pos = trap.clip_pos;
    let clip_grid = trap.clip_grid;
    let min = trap.affected_min;
    let max = trap.affected_max;

    drop(state);

    let server_clone = std::sync::Arc::clone(server);
    tokio::spawn(async move {
        let del_sel = SelInfo {
            o: sauer_proto::data::Vec3 { x: min[0], y: min[1], z: min[2] },
            s: sauer_proto::data::Vec3 {
                x: max[0] - min[0],
                y: max[1] - min[1],
                z: max[2] - min[2],
            },
            grid: 1,
            orient: sauer_proto::data::Orient::Left,
            cx: 0, cxs: 0, cy: 0, cys: 0, corner: 0,
        };
        let paste_sel = SelInfo {
            o: sauer_proto::data::Vec3 { x: clip_pos[0], y: clip_pos[1], z: clip_pos[2] },
            s: sauer_proto::data::Vec3 { x: 1, y: 1, z: 1 },
            grid: clip_grid,
            orient: sauer_proto::data::Orient::Left,
            cx: 0, cxs: 1, cy: 0, cys: 1, corner: 0,
        };

        // --- 1. APPLY TRIGGERED STATE ---
        server_clone.broadcast_chan1(&Client {
            client_num: actor_cn as i32,
            packet: channel1::DelCube { sel: del_sel.clone() },
        });
        server_clone.broadcast_chan1(&channel1::ClipboardServer {
            client_num: actor_cn as i32,
            unpack_len: trigger_unpack_len,
            data: trigger_clipboard.clone(),
        });
        server_clone.broadcast_chan1(&Client {
            client_num: actor_cn as i32,
            packet: channel1::PasteServer { sel: paste_sel.clone() },
        });

        for (&ent_id, ent) in &trigger_entities {
            let ent_type = sauer_proto::data::EntityType::try_from(u8::from(ent.ent_type) as i32)
                .unwrap_or(sauer_proto::data::EntityType::Empty);
            server_clone.broadcast_chan1(&Client {
                client_num: actor_cn as i32,
                packet: channel1::EditEnt {
                    ent_id: ent_id as i32,
                    pos: sauer_proto::data::Vec3 {
                        x: ent.position[0], y: ent.position[1], z: ent.position[2],
                    },
                    type_: ent_type,
                    attrs: ent.attrs.map(|x| x as i32),
                },
            });
        }

        // Update server map state
        {
            let mut state = get_server_state(&server_clone).write().unwrap();
            if let Some(map) = state.map.as_mut() {
                map.handle_del_cube(&channel1::DelCube { sel: del_sel.clone() });
                apply_clipboard_to_map(map, &trigger_clipboard);
                for (&ent_id, ent) in &trigger_entities {
                    let ent_type = sauer_proto::data::EntityType::try_from(
                        u8::from(ent.ent_type) as i32,
                    ).unwrap_or(sauer_proto::data::EntityType::Empty);
                    map.handle_edit_ent(&channel1::EditEnt {
                        ent_id: ent_id as i32,
                        pos: sauer_proto::data::Vec3 {
                            x: ent.position[0], y: ent.position[1], z: ent.position[2],
                        },
                        type_: ent_type,
                        attrs: ent.attrs.map(|x| x as i32),
                    });
                }
            }
        }

        tokio::time::sleep(std::time::Duration::from_secs(10)).await;

        // --- 2. RESTORE ORIGINAL STATE ---
        server_clone.broadcast_chan1(&Client {
            client_num: actor_cn as i32,
            packet: channel1::DelCube { sel: del_sel.clone() },
        });
        server_clone.broadcast_chan1(&channel1::ClipboardServer {
            client_num: actor_cn as i32,
            unpack_len: restore_unpack_len,
            data: restore_clipboard.clone(),
        });
        server_clone.broadcast_chan1(&Client {
            client_num: actor_cn as i32,
            packet: channel1::PasteServer { sel: paste_sel },
        });

        for (&ent_id, ent) in &original_entities {
            let ent_type = sauer_proto::data::EntityType::try_from(u8::from(ent.ent_type) as i32)
                .unwrap_or(sauer_proto::data::EntityType::Empty);
            server_clone.broadcast_chan1(&Client {
                client_num: actor_cn as i32,
                packet: channel1::EditEnt {
                    ent_id: ent_id as i32,
                    pos: sauer_proto::data::Vec3 {
                        x: ent.position[0], y: ent.position[1], z: ent.position[2],
                    },
                    type_: ent_type,
                    attrs: ent.attrs.map(|x| x as i32),
                },
            });
        }

        // Update server map state
        {
            let mut state = get_server_state(&server_clone).write().unwrap();
            if let Some(map) = state.map.as_mut() {
                map.handle_del_cube(&channel1::DelCube { sel: del_sel });
                apply_clipboard_to_map(map, &restore_clipboard);
                for (&ent_id, ent) in &original_entities {
                    let ent_type = sauer_proto::data::EntityType::try_from(
                        u8::from(ent.ent_type) as i32,
                    ).unwrap_or(sauer_proto::data::EntityType::Empty);
                    map.handle_edit_ent(&channel1::EditEnt {
                        ent_id: ent_id as i32,
                        pos: sauer_proto::data::Vec3 {
                            x: ent.position[0], y: ent.position[1], z: ent.position[2],
                        },
                        type_: ent_type,
                        attrs: ent.attrs.map(|x| x as i32),
                    });
                }
            }
            if trap_idx < state.traps.len() {
                state.traps[trap_idx].is_active = false;
            }
        }
    });
}

/// Applies a region clipboard to the server's internal map state.
/// Nodes are packed X→Y→Z; we call set_node_at for each cell.
fn apply_clipboard_to_map(map: &mut crate::map_state::MapState, compressed: &[u8]) {
    let Some((cb_sel, nodes)) = crate::block_reader::decode_clipboard(compressed) else {
        return;
    };
    let grid = cb_sel.grid;
    let sx = cb_sel.s.x;
    let sy = cb_sel.s.y;
    let sz = cb_sel.s.z;
    let ox = cb_sel.o.x;
    let oy = cb_sel.o.y;
    let oz = cb_sel.o.z;
    for xi in 0..sx {
        for yi in 0..sy {
            for zi in 0..sz {
                let idx = (xi * sy * sz + yi * sz + zi) as usize;
                if idx >= nodes.len() {
                    continue;
                }
                let pos = [ox + xi * grid, oy + yi * grid, oz + zi * grid];
                map.world.set_node_at(pos, grid, nodes[idx].clone());
            }
        }
    }
    map.invalidate_cache();
}
