use sauer_proto::data::{Orient, SelInfo, Vec3};

use super::BUTTON_COLOR_READY;

// ─── vslot application ────────────────────────────────────────────────────────

pub fn get_outer_face_selections(adj: [i32; 3], size: i32, orient: u8) -> [SelInfo; 6] {
    let d = (orient >> 1) as usize;
    let dc = orient & 1;
    let mut min = adj;
    if dc == 0 {
        min[d] = adj[d] + size - 2;
    }
    let mut s = [size, size, size];
    s[d] = 2;

    let o = Vec3 {
        x: min[0],
        y: min[1],
        z: min[2],
    };
    let sv = Vec3 {
        x: s[0],
        y: s[1],
        z: s[2],
    };
    [
        SelInfo {
            o,
            s: sv,
            grid: 1,
            orient: Orient::Left,
            cx: 0,
            cxs: 0,
            cy: 0,
            cys: 0,
            corner: 0,
        },
        SelInfo {
            o,
            s: sv,
            grid: 1,
            orient: Orient::Right,
            cx: 0,
            cxs: 0,
            cy: 0,
            cys: 0,
            corner: 0,
        },
        SelInfo {
            o,
            s: sv,
            grid: 1,
            orient: Orient::Back,
            cx: 0,
            cxs: 0,
            cy: 0,
            cys: 0,
            corner: 0,
        },
        SelInfo {
            o,
            s: sv,
            grid: 1,
            orient: Orient::Front,
            cx: 0,
            cxs: 0,
            cy: 0,
            cys: 0,
            corner: 0,
        },
        SelInfo {
            o,
            s: sv,
            grid: 1,
            orient: Orient::Bottom,
            cx: 0,
            cxs: 0,
            cy: 0,
            cys: 0,
            corner: 0,
        },
        SelInfo {
            o,
            s: sv,
            grid: 1,
            orient: Orient::Top,
            cx: 0,
            cxs: 0,
            cy: 0,
            cys: 0,
            corner: 0,
        },
    ]
}

pub fn get_payload_selection(adj: [i32; 3], size: i32, orient: u8) -> SelInfo {
    let d = (orient >> 1) as usize;
    let dc = orient & 1;
    let mut min = adj;
    if dc == 0 {
        min[d] = adj[d] + size - 2;
    }

    let mut offset = [size / 2, size / 2, size / 2];
    offset[d] = 0; // Layer A (min[d])

    let internal_orient = (d * 2) + 1; // Points inwards towards Layer B (min[d] + 1)

    SelInfo {
        o: Vec3 {
            x: min[0] + offset[0],
            y: min[1] + offset[1],
            z: min[2] + offset[2],
        },
        s: Vec3 { x: 1, y: 1, z: 1 },
        grid: 1,
        orient: sauer_proto::data::Orient::try_from(internal_orient as i32)
            .unwrap_or(sauer_proto::data::Orient::Left),
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

    // Buttons are solid: see-through (alpha) buttons made Sauerbraten flicker
    // when items or particles were near them.
    let mut data_sides = VSlotDelta::default();
    data_sides.color = Some((color[0], color[1], color[2]));

    let mut data_front = data_sides.clone();
    data_front.scale = Some(2.0);

    let proto_orient = Orient::try_from(orient as i32).unwrap_or(Orient::Left);

    let mut packets = Vec::new();
    for sel in get_outer_face_selections(adj, size, orient) {
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

pub fn build_button_geometry(map: &mut sauer_map::Map, adj: [i32; 3], size: i32, orient: u8) {
    let d = (orient >> 1) as usize;
    let dc = orient & 1;
    let mut min = adj;
    if dc == 0 {
        min[d] = adj[d] + size - 2;
    }
    let mut s = [size; 3];
    s[d] = 2;

    let sel = sauer_map::Selection {
        o: sauer_proto::data::Vec3 {
            x: min[0],
            y: min[1],
            z: min[2],
        },
        s: sauer_proto::data::Vec3 {
            x: s[0],
            y: s[1],
            z: s[2],
        },
        grid: 1, // fill exactly with 1x1x1 cubes, robust to deformation
        orient: sauer_proto::data::Orient::try_from(orient as i32)
            .unwrap_or(sauer_proto::data::Orient::Left),
        cx: 0,
        cxs: 0,
        cy: 0,
        cys: 0,
        corner: 0,
    };

    map.fill(&sel, true);
    map.set_texture(&sel, crate::trap::BUTTON_SIDE_TEX, true);
    map.set_texture(&sel, crate::trap::BUTTON_FRONT_TEX, false);
    map.set_material(&sel, Default::default());

    for pkt in get_outer_faces_vslot_packets(adj, size, orient, crate::trap::BUTTON_COLOR_READY) {
        map.apply_vslots(&pkt.sel, pkt.all_faces, pkt.delta != 0, &pkt.extra_data);
    }
}

// ─── Reset button colors ──────────────────────────────────────────────────────

/// Reset every trap button to the ready color directly in `map`'s vslot table.
/// Takes `(button_pos, button_size, button_orient)` tuples to avoid a split-borrow
/// when the caller holds `traps` and `map` in the same struct.
pub fn reset_button_colors_by_pos(map: &mut sauer_map::Map, buttons: &[([i32; 3], i32, u8)]) {
    for &(pos, size, orient) in buttons {
        for pkt in get_outer_faces_vslot_packets(pos, size, orient, BUTTON_COLOR_READY) {
            map.apply_vslots(&pkt.sel, pkt.all_faces, pkt.delta != 0, &pkt.extra_data);
        }
    }
}

// ─── Rebuild all trap buttons ─────────────────────────────────────────────────

pub fn rebuild_all_trap_buttons(map: &mut sauer_map::Map, traps: &[crate::map_state::Trap]) {
    for trap in traps {
        build_button_geometry(map, trap.button_pos, trap.button_size, trap.button_orient);
    }
    map.invalidate_cache();
}
