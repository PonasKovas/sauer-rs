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

    let mut data_sides = VSlotDelta::default();
    data_sides.color = Some((color[0], color[1], color[2]));
    // Ignored by the engine unless the cube has alpha material, which buttons
    // only get on the client of whoever is aiming at them.
    let a = crate::trap::BUTTON_SEETHROUGH_ALPHA;
    data_sides.alpha = Some((a, a));

    let mut data_front = data_sides.clone();
    data_front.scale = Some(2.0);
    data_sides.scale = Some(crate::trap::BUTTON_SIDE_SCALE);

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
    // Solid for everyone. This also undoes the alpha material in maps saved by
    // someone who was looking through a button at the time.
    map.set_material(&sel, Default::default());

    for pkt in get_outer_faces_vslot_packets(adj, size, orient, crate::trap::BUTTON_COLOR_READY) {
        map.apply_vslots(&pkt.sel, pkt.all_faces, pkt.delta != 0, &pkt.extra_data);
    }
}

// ─── See-through buttons ──────────────────────────────────────────────────────

/// A button's `(pos, size, orient)`.
pub type ButtonKey = ([i32; 3], i32, u8);

pub fn button_key(trap: &crate::map_state::Trap) -> ButtonKey {
    (trap.button_pos, trap.button_size, trap.button_orient)
}

/// Which button each player sees through, kept in `Server::extra` (both modes
/// insert it on init). The flag is false once geometry was resent over the
/// buttons: that paste turns them solid on the client again, so the alpha has
/// to be sent once more.
#[derive(Default)]
pub struct SeeThrough(dashmap::DashMap<u64, (ButtonKey, bool)>);

/// Call after resending map geometry to clients: whoever is looking through a
/// button gets its alpha again on their next position update.
pub fn mark_seethrough_stale(server: &sauer_server::Server) {
    if let Some(st) = server.extra.get::<SeeThrough>() {
        st.0.iter_mut().for_each(|mut e| e.1 = false);
    }
}

/// Like `mark_seethrough_stale`, for one player who just loaded a whole map.
pub fn mark_seethrough_stale_for(server: &sauer_server::Server, id: u64) {
    if let Some(mut e) = server
        .extra
        .get::<SeeThrough>()
        .and_then(|st| st.0.get_mut(&id))
    {
        e.1 = false;
    }
}

pub fn forget_seethrough(server: &sauer_server::Server, id: u64) {
    if let Some(st) = server.extra.get::<SeeThrough>() {
        st.0.remove(&id);
    }
}

/// Makes `target` see-through for player `id` only, by giving its cubes alpha
/// material on their client, and turns the previous one back to solid.
pub fn update_seethrough_button(server: &sauer_server::Server, id: u64, target: Option<ButtonKey>) {
    let Some(shown) = server.extra.get::<SeeThrough>().map(|st| &st.0) else {
        return;
    };
    let prev = shown.get(&id).map(|e| *e);
    match (prev, target) {
        (None, None) => return,
        (Some((old, true)), Some(new)) if old == new => return,
        _ => {}
    }
    let prev = prev.map(|(key, _)| key).filter(|&old| Some(old) != target);

    let send = |key: ButtonKey, alpha: bool| {
        let (pos, size, orient) = key;
        let mat = ogz_rs::data::Material {
            alpha,
            ..Default::default()
        };
        server.send_chan1(
            id,
            &sauer_proto::Client {
                client_num: crate::SERVER_BOT_CN as i32,
                packet: sauer_proto::channel1::EditMat {
                    sel: get_outer_face_selections(pos, size, orient)[0].clone(),
                    mat: mat.to_bits() as i32,
                    filter: -1,
                },
            },
        );
    };

    if let Some(old) = prev {
        // Skip it if the map changed since: the client got a fresh map with a
        // solid button, and the spot may now hold something else.
        let still_there = crate::map_state::get_server_state(server)
            .read()
            .unwrap()
            .traps
            .iter()
            .any(|t| button_key(t) == old);
        if still_there {
            send(old, false);
        }
    }
    if let Some(new) = target {
        send(new, true);
        shown.insert(id, (new, true));
    } else {
        shown.remove(&id);
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
