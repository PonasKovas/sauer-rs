use crate::map_state::get_server_state;
use sauer_proto::{
    Client, PacketSequence,
    channel1::{self},
    data::{EditFaceMode, Orient, SelInfo, Vec3},
};
use sauer_server::Server;

use super::BUTTON_COLOR_READY;

// ─── vslot packing helpers ────────────────────────────────────────────────────

pub fn pack_vslot_bytes(entries: &[(u8, &[u8])]) -> Vec<u8> {
    let mut buf = Vec::new();
    for (type_byte, data) in entries {
        buf.push(*type_byte);
        buf.extend_from_slice(data);
    }
    buf.push(0xFF);
    buf
}

// ─── Selection helpers ────────────────────────────────────────────────────────

pub fn get_selections(
    adj: [i32; 3],
    button_size: i32,
    hit_orient: u8,
) -> (SelInfo, SelInfo) {
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

// ─── vslot application ────────────────────────────────────────────────────────

pub fn get_outer_face_selections(adj: [i32; 3], size: i32) -> [SelInfo; 6] {
    let o = Vec3 { x: adj[0], y: adj[1], z: adj[2] };
    let s = Vec3 { x: 1, y: 1, z: 1 };
    [
        SelInfo { o, s, grid: size, orient: Orient::Left, cx: 0, cxs: 0, cy: 0, cys: 0, corner: 0 },
        SelInfo { o, s, grid: size, orient: Orient::Right, cx: 0, cxs: 0, cy: 0, cys: 0, corner: 0 },
        SelInfo { o, s, grid: size, orient: Orient::Back, cx: 0, cxs: 0, cy: 0, cys: 0, corner: 0 },
        SelInfo { o, s, grid: size, orient: Orient::Front, cx: 0, cxs: 0, cy: 0, cys: 0, corner: 0 },
        SelInfo { o, s, grid: size, orient: Orient::Bottom, cx: 0, cxs: 0, cy: 0, cys: 0, corner: 0 },
        SelInfo { o, s, grid: size, orient: Orient::Top, cx: 0, cxs: 0, cy: 0, cys: 0, corner: 0 },
    ]
}

pub fn get_cavity_floor_selection(adj: [i32; 3]) -> SelInfo {
    SelInfo {
        o: Vec3 { x: adj[0] + 2, y: adj[1] + 2, z: adj[2] },
        s: Vec3 { x: 2, y: 2, z: 1 },
        grid: 2,
        orient: Orient::Top, // Top of the blocks making the floor
        cx: 0, cxs: 0, cy: 0, cys: 0, corner: 0,
    }
}

pub fn get_outer_faces_vslot_packets(adj: [i32; 3], size: i32, orient: u8, color: [f32; 3]) -> Vec<sauer_proto::channel1::EditVSlot> {
    let color_bytes = color.iter().flat_map(|f| f.to_le_bytes()).collect::<Vec<u8>>();
    let alpha_bytes = [0.8f32, 0.8f32].iter().flat_map(|f| f.to_le_bytes()).collect::<Vec<u8>>();
    let scale_bytes = 2.0f32.to_le_bytes().to_vec();

    let mut data_sides = Vec::new();
    data_sides.push(7); data_sides.extend_from_slice(&color_bytes);
    data_sides.push(6); data_sides.extend_from_slice(&alpha_bytes);
    data_sides.push(0xFF);

    let mut data_front = Vec::new();
    data_front.push(7); data_front.extend_from_slice(&color_bytes);
    data_front.push(6); data_front.extend_from_slice(&alpha_bytes);
    data_front.push(1); data_front.extend_from_slice(&scale_bytes);
    data_front.push(0xFF);

    let proto_orient = Orient::try_from(orient as i32).unwrap_or(Orient::Left);

    let mut packets = Vec::new();
    for sel in get_outer_face_selections(adj, size) {
        let extra_data = if sel.orient == proto_orient { data_front.clone() } else { data_sides.clone() };
        packets.push(sauer_proto::channel1::EditVSlot {
            sel,
            delta: 0,
            all_faces: false,
            extra_data,
        });
    }
    packets
}

pub fn build_button_geometry(map: &mut crate::map_state::MapState, adj: [i32; 3], size: i32, orient: u8) {
    // 1. Fill solid button bounds
    let sel = SelInfo {
        o: Vec3 { x: adj[0], y: adj[1], z: adj[2] },
        s: Vec3 { x: 1, y: 1, z: 1 },
        grid: size,
        orient: Orient::try_from(orient as i32).unwrap_or(Orient::Left),
        cx: 0, cxs: 2, cy: 0, cys: 2, corner: 0,
    };
    map.world.fill(&sel, true);
    map.world.set_texture(&sel, crate::trap::BUTTON_SIDE_TEX, true);
    map.world.set_texture(&sel, crate::trap::BUTTON_FRONT_TEX, false);
    map.world.set_material(&sel, ogz_rs::data::Material { alpha: true, ..Default::default() });

    // 2. Hollow out internal cavity for storing payload safely
    let cavity_sel = SelInfo {
        o: Vec3 { x: adj[0] + 2, y: adj[1] + 2, z: adj[2] + 2 },
        s: Vec3 { x: 2, y: 2, z: 2 },
        grid: 2,
        orient: Orient::Top,
        cx: 0, cxs: 0, cy: 0, cys: 0, corner: 0,
    };
    map.world.fill(&cavity_sel, false);

    // 3. Apply appearance VSlots strictly to the 6 outer faces
    for pkt in get_outer_faces_vslot_packets(adj, size, orient, crate::trap::BUTTON_COLOR_READY) {
        map.handle_edit_v_slot(&pkt);
    }
}

// ─── Corner-push animation helpers ───────────────────────────────────────────

pub(super) fn corner_pushes(
    sel: &SelInfo,
    corner: i32,
    cn: i32,
) -> impl PacketSequence<sauer_proto::route::S2C<sauer_proto::route::Chan1>>
+ sauer_proto::PacketDescriptor
+ std::fmt::Debug {
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

// ─── Reset button colors ──────────────────────────────────────────────────────

/// Reset every trap button to the ready color directly in `map`'s vslot table.
/// Takes `(button_pos, button_size, button_orient)` tuples to avoid a split-borrow
/// when the caller holds `traps` and `map` in the same struct.
pub fn reset_button_colors_by_pos(
    map: &mut crate::map_state::MapState,
    buttons: &[([i32; 3], i32, u8)],
) {
    for &(pos, size, orient) in buttons {
        for pkt in get_outer_faces_vslot_packets(pos, size, orient, BUTTON_COLOR_READY) {
            map.handle_edit_v_slot(&pkt);
        }
    }
}

// ─── Rebuild all trap buttons ─────────────────────────────────────────────────

pub fn rebuild_all_trap_buttons(
    map: &mut crate::map_state::MapState,
    traps: &[crate::map_state::Trap],
) {
    for trap in traps {
        build_button_geometry(map, trap.button_pos, trap.button_size, trap.button_orient);
        
        let sel = sauer_map::Selection {
            o: sauer_proto::data::Vec3 {
                x: trap.button_pos[0],
                y: trap.button_pos[1],
                z: trap.button_pos[2],
            },
            s: sauer_proto::data::Vec3 { x: 1, y: 1, z: 1 },
            grid: trap.button_size,
            orient: sauer_proto::data::Orient::try_from(trap.button_orient as i32).unwrap_or(sauer_proto::data::Orient::Left),
            cx: 0, cxs: 2, cy: 0, cys: 2, corner: 0,
        };
        for corner in 0..4 {
            let mut csel = sel.clone();
            csel.corner = corner;
            for _ in 0..6 {
                map.world.edit_face(&csel, 1, 2);
            }
        }
    }
    map.invalidate_cache();
}

// ─── Atomic rebuild ───────────────────────────────────────────────────────────

pub(super) fn send_atomic_rebuild(
    server: &Server,
    owner_id: u64,
    adj: [i32; 3],
    button_size: i32,
    hit_orient: u8,
    clip_unpack_len: i32,
    clip_data: Vec<u8>,
) {
    let target_cn = server.get_cn(owner_id).unwrap_or(0) as i32;
    let (adj_sel, fill_sel) = get_selections(adj, button_size, hit_orient);

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
        packet: channel1::DelCube {
            sel: adj_sel.clone(),
        },
    });
    crate::geo_sync::broadcast_geometry(server, target_cn, &adj_sel, clip_unpack_len, &clip_data);
    server.broadcast_chan1((
        corner_pushes(&adj_sel, 0, target_cn),
        corner_pushes(&adj_sel, 1, target_cn),
        corner_pushes(&adj_sel, 2, target_cn),
        corner_pushes(&adj_sel, 3, target_cn),
    ));
}

// ─── check_and_rebuild_traps ──────────────────────────────────────────────────

pub fn check_and_rebuild_traps(
    ctx: &sauer_server::plugin::Context,
    intersects: impl Fn(&crate::map_state::Trap) -> bool,
) {
    let server = ctx.server().clone();
    let id = ctx.id();
    let mut state = get_server_state(&server).write().unwrap();

    // Never rebuild the trap currently being recorded — the user is intentionally editing
    // that area and auto-rebuilds would corrupt the server map state and fight the user.
    let recording_idx = state.trap_edit.as_ref().map(|e| e.trap_idx);

    let to_rebuild: Vec<_> = state
        .traps
        .iter()
        .enumerate()
        .filter(|(i, t)| intersects(t) && Some(*i) != recording_idx)
        .map(|(_, t)| crate::map_state::Trap {
            button_pos: t.button_pos,
            button_size: t.button_size,
            button_orient: t.button_orient,
            trigger_clipboard: t.trigger_clipboard.clone(),
            trigger_unpack_len: t.trigger_unpack_len,
            trigger_cells: t.trigger_cells.clone(),
            trigger_entities_mod: t.trigger_entities_mod.clone(),
            trigger_entities_add: t.trigger_entities_add.clone(),
            clip_pos: t.clip_pos,
            clip_s: t.clip_s,
            clip_grid: t.clip_grid,
            time: t.time,
            timeout: t.timeout,
            reset_clipboard: t.reset_clipboard.clone(),
            reset_unpack_len: t.reset_unpack_len,
            reset_cells: t.reset_cells.clone(),
            reset_entities_mod: t.reset_entities_mod.clone(),
            is_active: t.is_active,
        })
        .collect();

    if to_rebuild.is_empty() {
        return;
    }

    for trap in to_rebuild {
        let mut clip_unpack_len = 0;
        let mut clip_data = Vec::new();

        if let Some(map) = state.map.as_mut() {
            build_button_geometry(map, trap.button_pos, trap.button_size, trap.button_orient);

            let btn_node = sauer_map::octree::extract_node_at(
                &map.world.root,
                [0, 0, 0],
                map.world.size,
                trap.button_pos,
                trap.button_size,
            );
            let (ul, d) = sauer_map::clipboard::generate_clipboard(
                &btn_node,
                trap.button_size,
                &map.meta.vslots,
            );
            clip_unpack_len = ul;
            clip_data = d;

            let sel = sauer_map::Selection {
                o: Vec3 {
                    x: trap.button_pos[0],
                    y: trap.button_pos[1],
                    z: trap.button_pos[2],
                },
                s: Vec3 { x: 1, y: 1, z: 1 },
                grid: trap.button_size,
                orient: sauer_proto::data::Orient::Left,
                cx: 0, cxs: 2, cy: 0, cys: 2, corner: 0,
            };
            for corner in 0..4 {
                let mut csel = sel.clone();
                csel.corner = corner;
                for _ in 0..6 {
                    map.world.edit_face(&csel, 1, 2);
                }
            }

            map.invalidate_cache();
        }

        send_atomic_rebuild(
            &server,
            id,
            trap.button_pos,
            trap.button_size,
            trap.button_orient,
            clip_unpack_len,
            clip_data,
        );
    }

    crate::trap::stored::sync_traps_to_map(&server, id);
}
