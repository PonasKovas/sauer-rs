use sauer_proto::data::{Orient, SelInfo, Vec3};

use super::BUTTON_COLOR_READY;

// ─── vslot application ────────────────────────────────────────────────────────

pub fn get_outer_face_selections(adj: [i32; 3], size: i32) -> [SelInfo; 6] {
    let o = Vec3 {
        x: adj[0],
        y: adj[1],
        z: adj[2],
    };
    let s = Vec3 { x: 1, y: 1, z: 1 };
    [
        SelInfo {
            o,
            s,
            grid: size,
            orient: Orient::Left,
            cx: 0,
            cxs: 0,
            cy: 0,
            cys: 0,
            corner: 0,
        },
        SelInfo {
            o,
            s,
            grid: size,
            orient: Orient::Right,
            cx: 0,
            cxs: 0,
            cy: 0,
            cys: 0,
            corner: 0,
        },
        SelInfo {
            o,
            s,
            grid: size,
            orient: Orient::Back,
            cx: 0,
            cxs: 0,
            cy: 0,
            cys: 0,
            corner: 0,
        },
        SelInfo {
            o,
            s,
            grid: size,
            orient: Orient::Front,
            cx: 0,
            cxs: 0,
            cy: 0,
            cys: 0,
            corner: 0,
        },
        SelInfo {
            o,
            s,
            grid: size,
            orient: Orient::Bottom,
            cx: 0,
            cxs: 0,
            cy: 0,
            cys: 0,
            corner: 0,
        },
        SelInfo {
            o,
            s,
            grid: size,
            orient: Orient::Top,
            cx: 0,
            cxs: 0,
            cy: 0,
            cys: 0,
            corner: 0,
        },
    ]
}

pub fn get_payload_selection(adj: [i32; 3], orient: u8) -> SelInfo {
    let d = (orient >> 1) as usize;
    let dc = orient & 1;

    let mut offset = [1, 1, 1];
    if dc == 0 {
        offset[d] = 7;
    }

    SelInfo {
        o: Vec3 {
            x: adj[0] + offset[0],
            y: adj[1] + offset[1],
            z: adj[2] + offset[2],
        },
        s: Vec3 { x: 1, y: 1, z: 1 },
        grid: 1, // 1-cube wide selection forces subdivision into an internal invisible face
        orient: Orient::Top,
        cx: 0,
        cxs: 0,
        cy: 0,
        cys: 0,
        corner: 0,
    }
}

pub fn get_outer_faces_vslot_packets(
    adj: [i32; 3],
    size: i32,
    orient: u8,
    color: [f32; 3],
) -> Vec<sauer_proto::channel1::EditVSlot> {
    use sauer_proto::data::VSlotDelta;

    let mut data_sides = VSlotDelta::default();
    data_sides.color = Some((color[0], color[1], color[2]));
    data_sides.alpha = Some((0.8, 0.8));

    let mut data_front = data_sides.clone();
    data_front.scale = Some(2.0);

    let proto_orient = Orient::try_from(orient as i32).unwrap_or(Orient::Left);

    let mut packets = Vec::new();
    for sel in get_outer_face_selections(adj, size) {
        let extra_data = if sel.orient == proto_orient {
            data_front.clone()
        } else {
            data_sides.clone()
        };
        packets.push(sauer_proto::channel1::EditVSlot {
            sel,
            delta: 0,
            all_faces: false,
            extra_data,
        });
    }
    packets
}

pub fn build_button_geometry(
    map: &mut sauer_map::Map,
    adj: [i32; 3],
    size: i32,
    orient: u8,
) {
    // 1. Fill solid button bounds
    let sel = sauer_map::Selection {
        o: sauer_proto::data::Vec3 { x: adj[0], y: adj[1], z: adj[2] },
        s: sauer_proto::data::Vec3 { x: 1, y: 1, z: 1 },
        grid: size,
        orient: sauer_proto::data::Orient::try_from(orient as i32).unwrap_or(sauer_proto::data::Orient::Left),
        cx: 0, cxs: 2, cy: 0, cys: 2, corner: 0,
    };
    map.fill(&sel, true);
    map.set_texture(&sel, crate::trap::BUTTON_SIDE_TEX, true);
    map.set_texture(&sel, crate::trap::BUTTON_FRONT_TEX, false);
    map.set_material(&sel, ogz_rs::data::Material { alpha: true, ..Default::default() });

    // 2. Apply appearance VSlots strictly to the 6 outer faces
    for corner in 0..4 {
        let mut csel = sel.clone();
        csel.corner = corner;
        for _ in 0..6 {
            map.edit_face(&csel, 1, 2);
        }
    }

    for pkt in get_outer_faces_vslot_packets(adj, size, orient, crate::trap::BUTTON_COLOR_READY) {
        map.apply_vslots(&pkt.sel, pkt.all_faces, pkt.delta != 0, &pkt.extra_data);
    }
}

// ─── Reset button colors ──────────────────────────────────────────────────────

/// Reset every trap button to the ready color directly in `map`'s vslot table.
/// Takes `(button_pos, button_size, button_orient)` tuples to avoid a split-borrow
/// when the caller holds `traps` and `map` in the same struct.
pub fn reset_button_colors_by_pos(
    map: &mut sauer_map::Map,
    buttons: &[([i32; 3], i32, u8)],
) {
    for &(pos, size, orient) in buttons {
        for pkt in get_outer_faces_vslot_packets(pos, size, orient, BUTTON_COLOR_READY) {
            map.apply_vslots(&pkt.sel, pkt.all_faces, pkt.delta != 0, &pkt.extra_data);
        }
    }
}

// ─── Rebuild all trap buttons ─────────────────────────────────────────────────

pub fn rebuild_all_trap_buttons(
    map: &mut sauer_map::Map,
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
            orient: sauer_proto::data::Orient::try_from(trap.button_orient as i32)
                .unwrap_or(sauer_proto::data::Orient::Left),
            cx: 0,
            cxs: 2,
            cy: 0,
            cys: 2,
            corner: 0,
        };
        for corner in 0..4 {
            let mut csel = sel.clone();
            csel.corner = corner;
            for _ in 0..6 {
                map.edit_face(&csel, 1, 2);
            }
        }
    }
    map.invalidate_cache();
}


