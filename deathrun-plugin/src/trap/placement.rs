use crate::map_state::get_server_state;
use sauer_proto::{
    Client, PacketSequence,
    channel1::{self},
    data::{EditFaceMode, Orient, SelInfo, Vec3},
};
use sauer_server::Server;

use super::{BUTTON_COLOR_READY, BUTTON_FRONT_TEX, BUTTON_SIDE_TEX};

// ─── vslot packing helpers ────────────────────────────────────────────────────

pub(super) fn pack_vslot_bytes(entries: &[(u8, &[u8])]) -> Vec<u8> {
    let mut buf = Vec::new();
    for (type_byte, data) in entries {
        buf.push(*type_byte);
        buf.extend_from_slice(data);
    }
    buf.push(0xFF);
    buf
}

// ─── Selection helpers ────────────────────────────────────────────────────────

pub(super) fn get_selections(
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

pub(super) fn apply_internal_vslots(map: &mut crate::map_state::MapState, sel: &SelInfo) {
    // Apply in the exact same order as build_setup_packets broadcasts to clients,
    // so the server's vslot indices stay in sync with what clients create locally.
    // Type bytes: 1=SCALE, 6=ALPHA, 7=COLOR
    let color_bytes: Vec<u8> = BUTTON_COLOR_READY
        .iter()
        .flat_map(|f| f.to_le_bytes())
        .collect();
    let alpha_bytes: Vec<u8> = [0.8f32, 0.8f32]
        .iter()
        .flat_map(|f| f.to_le_bytes())
        .collect();
    let scale_bytes: Vec<u8> = 2.0f32.to_le_bytes().to_vec();

    // Step 1: color, all faces
    map.handle_edit_v_slot(&channel1::EditVSlot {
        sel: sel.clone(),
        delta: 0,
        all_faces: true,
        extra_data: pack_vslot_bytes(&[(7, &color_bytes)]),
    });

    // Step 2: scale, front face only
    map.handle_edit_v_slot(&channel1::EditVSlot {
        sel: sel.clone(),
        delta: 0,
        all_faces: false,
        extra_data: pack_vslot_bytes(&[(1, &scale_bytes)]),
    });

    // Step 3: alpha, all faces
    map.handle_edit_v_slot(&channel1::EditVSlot {
        sel: sel.clone(),
        delta: 0,
        all_faces: true,
        extra_data: pack_vslot_bytes(&[(6, &alpha_bytes)]),
    });
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
        let (adj_sel, _) = get_selections(pos, size, orient);
        let color_bytes: Vec<u8> = BUTTON_COLOR_READY
            .iter()
            .flat_map(|f| f.to_le_bytes())
            .collect();
        let data = pack_vslot_bytes(&[(7, &color_bytes)]);
        map.handle_edit_v_slot(&sauer_proto::channel1::EditVSlot {
            sel: adj_sel,
            delta: 0,
            all_faces: true,
            extra_data: data,
        });
    }
}

// ─── Rebuild all trap buttons ─────────────────────────────────────────────────

pub fn rebuild_all_trap_buttons(
    map: &mut crate::map_state::MapState,
    traps: &[crate::map_state::Trap],
) {
    for trap in traps {
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
        map.world.fill(&sel, true);
        map.world.set_texture(&sel, crate::trap::BUTTON_SIDE_TEX, true);
        map.world.set_texture(&sel, crate::trap::BUTTON_FRONT_TEX, false);
        map.world.set_material(
            &sel,
            ogz_rs::data::Material {
                alpha: true,
                ..Default::default()
            },
        );

        let (adj_sel, _) = get_selections(trap.button_pos, trap.button_size, trap.button_orient);
        apply_internal_vslots(map, &adj_sel);

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
    cn: u32,
    adj: [i32; 3],
    button_size: i32,
    hit_orient: u8,
    clip_unpack_len: i32,
    clip_data: Vec<u8>,
) {
    let target_cn = cn as i32;
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
    let cn = ctx.cn();
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
            let sel = sauer_map::Selection {
                o: Vec3 {
                    x: trap.button_pos[0],
                    y: trap.button_pos[1],
                    z: trap.button_pos[2],
                },
                s: Vec3 { x: 1, y: 1, z: 1 },
                grid: trap.button_size,
                orient: sauer_proto::data::Orient::try_from(trap.button_orient as i32)
                    .unwrap_or(sauer_proto::data::Orient::Left),
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
                ogz_rs::data::Material {
                    alpha: true,
                    ..Default::default()
                },
            );

            let (adj_sel, _) =
                get_selections(trap.button_pos, trap.button_size, trap.button_orient);
            apply_internal_vslots(map, &adj_sel);

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
            cn,
            trap.button_pos,
            trap.button_size,
            trap.button_orient,
            clip_unpack_len,
            clip_data,
        );
    }
}
