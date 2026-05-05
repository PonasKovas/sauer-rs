use super::stored::sync_traps_to_map;
use super::{BUTTON_SIZE, TrapEditState};
use crate::map_state::{Trap, get_server_state};
use cube_string::IntoCubeStringLossy;
use sauer_map::Selection;
use sauer_proto::{
    Client,
    channel1::{self, ServMsg},
    data::{EditFaceMode, SelInfo, Vec3},
};
use sauer_server::plugin::Context;

pub fn cmd_deltrap(ctx: &Context, hit_pt: [f32; 3]) {
    let id = ctx.id();
    let server = ctx.server();
    let cn = server.get_cn(id).unwrap_or(0) as i32;
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
                id,
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
    map.world.fill(&sel, false);
    map.world.set_material(&sel, Default::default());
    drop(state);

    sync_traps_to_map(server, id);

    let (adj_sel, _) =
        super::placement::get_selections(trap.button_pos, trap.button_size, trap.button_orient);
    server.broadcast_chan1(&Client {
        client_num: crate::SERVER_BOT_CN as i32,
        packet: channel1::DelCube { sel: adj_sel },
    });
}

pub fn cmd_newtrap(ctx: &Context, hit_pt: [f32; 3], hit_orient: u8) {
    let id = ctx.id();
    let server = ctx.server().clone();
    let cn = server.get_cn(id).unwrap_or(0) as i32;
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

    let target_cn = crate::SERVER_BOT_CN as i32;
    let (adj_sel, fill_sel) = super::placement::get_selections(adj, BUTTON_SIZE, hit_orient);

    crate::trap::placement::build_button_geometry(map, adj, BUTTON_SIZE, hit_orient);

    // Snapshot before corner pushes: clipboard delivers the flat button with
    // correct vslot indices and the internal cavity; corner animation then pushes edges on clients.
    let button_node =
        sauer_map::octree::extract_node_at(&map.world.root, [0, 0, 0], ws, adj, BUTTON_SIZE);
    let (clip_unpack_len, clip_data) =
        sauer_map::clipboard::generate_clipboard(&button_node, BUTTON_SIZE, &map.meta.vslots);

    let sel = Selection {
        o: Vec3 {
            x: adj[0],
            y: adj[1],
            z: adj[2],
        },
        s: Vec3 { x: 1, y: 1, z: 1 },
        grid: BUTTON_SIZE,
        orient: sauer_proto::data::Orient::try_from(hit_orient as i32)
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
            map.world.edit_face(&csel, 1, 2);
        }
    }

    state.traps.push(Trap {
        button_pos: adj,
        button_size: BUTTON_SIZE,
        button_orient: hit_orient,
        trigger_clipboard: Vec::new(),
        trigger_unpack_len: 0,
        trigger_cells: Vec::new(),
        trigger_entities_mod: Vec::new(),
        trigger_entities_add: Vec::new(),
        clip_pos: [0; 3],
        clip_s: [1; 3],
        clip_grid: 1,
        time: 2000,
        timeout: 20000,
        reset_clipboard: Vec::new(),
        reset_unpack_len: 0,
        reset_cells: Vec::new(),
        reset_entities_mod: Vec::new(),
        is_active: false,
    });
    drop(state);

    // Broadcast the button using a clipboard so clients install the exact vslot
    // indices already in the server map, regardless of when /getmap is done.
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
    crate::geo_sync::broadcast_geometry(&server, target_cn, &adj_sel, clip_unpack_len, &clip_data);

    tokio::spawn(async move {
        use tokio::time::{Duration, sleep};
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

        // Sync traps to map ONLY AFTER corner pushes complete.
        // This ensures the client map collapses the branch BEFORE we inject
        // the VSlot payload, safely subdividing the deformed geometry
        // and avoiding payload destruction.
        crate::trap::stored::sync_traps_to_map(&server, id);
    });
}

pub fn cmd_traptime(ctx: &Context, hit_pt: [f32; 3], time: u32) {
    let id = ctx.id();
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
                id,
                &ServMsg {
                    text: "Not looking at a trap.".lossy_cubestring(),
                },
            );
            return;
        }
        Some(i) => i,
    };

    state.traps[trap_idx].time = time;
    let timeout = state.traps[trap_idx].timeout;
    let mut msg = format!("Trap time set to {}ms.", time);
    if timeout > 0 && time > timeout {
        msg.push_str(&format!(
            " Warning: this is greater than the timeout ({}ms).",
            timeout
        ));
    }
    server.send_chan1(
        id,
        &ServMsg {
            text: msg.lossy_cubestring(),
        },
    );
    drop(state);
    sync_traps_to_map(server, id);
}

pub fn cmd_traptimeout(ctx: &Context, hit_pt: [f32; 3], timeout: u32) {
    let id = ctx.id();
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
                id,
                &ServMsg {
                    text: "Not looking at a trap.".lossy_cubestring(),
                },
            );
            return;
        }
        Some(i) => i,
    };

    state.traps[trap_idx].timeout = timeout;
    let time = state.traps[trap_idx].time;
    let mut msg = format!("Trap timeout set to {}ms.", timeout);
    if timeout > 0 && timeout < time {
        msg.push_str(&format!(
            " Warning: this is less than the trap time ({}ms).",
            time
        ));
    }
    server.send_chan1(
        id,
        &ServMsg {
            text: msg.lossy_cubestring(),
        },
    );
    drop(state);
    sync_traps_to_map(server, id);
}

pub fn cmd_traptime_print(ctx: &Context, hit_pt: [f32; 3]) {
    let id = ctx.id();
    let server = ctx.server();
    let state = get_server_state(server).read().unwrap();

    let trap_idx = state.traps.iter().position(|t| {
        (0..3).all(|ax| {
            hit_pt[ax] >= t.button_pos[ax] as f32
                && hit_pt[ax] < (t.button_pos[ax] + t.button_size) as f32
        })
    });

    let trap_idx = match trap_idx {
        None => {
            server.send_chan1(
                id,
                &ServMsg {
                    text: "Not looking at a trap.".lossy_cubestring(),
                },
            );
            return;
        }
        Some(i) => i,
    };

    let time = state.traps[trap_idx].time;
    server.send_chan1(
        id,
        &ServMsg {
            text: format!("Trap time is {}ms.", time).lossy_cubestring(),
        },
    );
}

pub fn cmd_traptimeout_print(ctx: &Context, hit_pt: [f32; 3]) {
    let id = ctx.id();
    let server = ctx.server();
    let state = get_server_state(server).read().unwrap();

    let trap_idx = state.traps.iter().position(|t| {
        (0..3).all(|ax| {
            hit_pt[ax] >= t.button_pos[ax] as f32
                && hit_pt[ax] < (t.button_pos[ax] + t.button_size) as f32
        })
    });

    let trap_idx = match trap_idx {
        None => {
            server.send_chan1(
                id,
                &ServMsg {
                    text: "Not looking at a trap.".lossy_cubestring(),
                },
            );
            return;
        }
        Some(i) => i,
    };

    let timeout = state.traps[trap_idx].timeout;
    server.send_chan1(
        id,
        &ServMsg {
            text: format!("Trap timeout is {}ms.", timeout).lossy_cubestring(),
        },
    );
}

pub fn cmd_trapinfo(ctx: &Context, hit_pt: [f32; 3]) {
    let id = ctx.id();
    let server = ctx.server();
    let state = get_server_state(server).read().unwrap();

    let trap_idx = state.traps.iter().position(|t| {
        (0..3).all(|ax| {
            hit_pt[ax] >= t.button_pos[ax] as f32
                && hit_pt[ax] < (t.button_pos[ax] + t.button_size) as f32
        })
    });

    let trap_idx = match trap_idx {
        None => {
            server.send_chan1(
                id,
                &ServMsg {
                    text: "Not looking at a trap.".lossy_cubestring(),
                },
            );
            return;
        }
        Some(i) => i,
    };

    let trap = &state.traps[trap_idx];
    let stored = crate::trap::StoredTrap::from_trap(trap);
    let raw_bytes = bitcode::serialize(&stored).unwrap_or_default();
    let uncompressed_size = raw_bytes.len();

    let mut zlib = Vec::new();
    {
        use std::io::Write;
        let mut enc = flate2::write::ZlibEncoder::new(&mut zlib, flate2::Compression::default());
        let _ = enc.write_all(&raw_bytes);
        let _ = enc.finish();
    }
    let payload_len = zlib.len() + 6;

    let total_chunks = (payload_len + 15) / 16;

    let mut size_on_disk = 0;
    let mut size_in_mem = 0;
    for idx in 0..total_chunks {
        let name_len = crate::trap::vslot_store::encode_chunk_name(idx).len();
        size_on_disk += 1 + 1 + name_len + 16;
        size_in_mem += 32 + name_len;
    }

    let mut msg = format!("\x0C1[Trap {} Info]\n", trap_idx);
    msg.push_str(&format!("\x0C7Uncompressed bitcode: \x0C2{} bytes\n", uncompressed_size));
    msg.push_str(&format!("\x0C7Compressed payload: \x0C2{} bytes\n", payload_len));
    msg.push_str(&format!("\x0C7VSlot params (chunks): \x0C2{}\n", total_chunks));
    msg.push_str(&format!("\x0C7Est. Disk Size: \x0C2{} bytes\n", size_on_disk));
    msg.push_str(&format!("\x0C7Est. Memory Size: \x0C2{} bytes", size_in_mem));

    server.send_chan1(
        id,
        &ServMsg {
            text: msg.lossy_cubestring(),
        },
    );
}

pub fn cmd_edittrap(ctx: &Context, hit_pt: [f32; 3]) {
    let id = ctx.id();
    let server = ctx.server();

    // Phase 1: validate, snapshot the base state, and collect existing trigger data.
    // All done under a single write lock that is released before apply_and_broadcast.
    let (trap_idx, original_map, trigger_data) = {
        let state = get_server_state(server).write().unwrap();

        if state.trap_edit.is_some() {
            server.send_chan1(
                id,
                &ServMsg {
                    text: "A trap is already being edited. Use #endtrap first.".lossy_cubestring(),
                },
            );
            return;
        }

        let trap_idx = match state.traps.iter().position(|t| {
            (0..3).all(|ax| {
                hit_pt[ax] >= t.button_pos[ax] as f32
                    && hit_pt[ax] < (t.button_pos[ax] + t.button_size) as f32
            })
        }) {
            None => {
                server.send_chan1(
                    id,
                    &ServMsg {
                        text: "Not looking at a trap.".lossy_cubestring(),
                    },
                );
                return;
            }
            Some(i) => i,
        };

        let trap = &state.traps[trap_idx];
        let original_map = state.map.clone().unwrap();

        let trigger_data = if !trap.trigger_clipboard.is_empty()
            || !trap.trigger_entities_mod.is_empty()
            || !trap.trigger_entities_add.is_empty()
        {
            Some((
                trap.trigger_cells.clone(),
                trap.trigger_entities_mod.clone(),
                trap.trigger_entities_add.clone(),
                trap.trigger_clipboard.clone(),
                trap.trigger_unpack_len,
                trap.clip_pos,
                trap.clip_s,
                trap.clip_grid,
            ))
        } else {
            None
        };

        (trap_idx, original_map, trigger_data)
    };

    let Some((
        trigger_cells,
        trigger_entities_mod,
        trigger_entities_add,
        trigger_clipboard,
        trigger_unpack_len,
        clip_pos,
        clip_s,
        clip_grid,
    )) = trigger_data
    else {
        // No existing trigger: start recording from current state (same as #settrap).
        let mut state = get_server_state(server).write().unwrap();
        state.trap_edit = Some(TrapEditState {
            trap_idx,
            original_map: Box::new(original_map),
            affected_entities: std::collections::HashSet::new(),
        });
        drop(state);
        crate::ui::broadcast_trap_edit_started(server, trap_idx);
        return;
    };

    // Phase 2: apply the existing trigger state to the server map and broadcast to clients.
    let region_sel = SelInfo {
        o: Vec3 {
            x: clip_pos[0],
            y: clip_pos[1],
            z: clip_pos[2],
        },
        s: Vec3 {
            x: clip_s[0],
            y: clip_s[1],
            z: clip_s[2],
        },
        grid: clip_grid,
        orient: sauer_proto::data::Orient::Left,
        cx: 0,
        cxs: 0,
        cy: 0,
        cys: 0,
        corner: 0,
    };

    let assigned_ids = super::trigger::apply_and_broadcast(
        server,
        id,
        &region_sel,
        trigger_unpack_len,
        &trigger_clipboard,
        &trigger_cells,
        &trigger_entities_mod,
        &trigger_entities_add,
    );

    // Phase 3: set trap_edit with the pre-trigger map as the baseline for diffing.
    {
        let mut state = get_server_state(server).write().unwrap();
        let mut affected = std::collections::HashSet::new();
        for &(id, _) in &trigger_entities_mod {
            affected.insert(id);
        }
        for id in assigned_ids {
            affected.insert(id);
        }
        state.trap_edit = Some(TrapEditState {
            trap_idx,
            original_map: Box::new(original_map),
            affected_entities: affected,
        });
    }

    crate::ui::broadcast_trap_edit_started(server, trap_idx);
}

pub fn cmd_settrap(ctx: &Context, hit_pt: [f32; 3]) {
    let id = ctx.id();
    let server = ctx.server();
    let mut state = get_server_state(server).write().unwrap();

    if state.trap_edit.is_some() {
        server.send_chan1(
            id,
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
                id,
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
        affected_entities: std::collections::HashSet::new(),
    });
    drop(state);

    crate::ui::broadcast_trap_edit_started(server, trap_idx);
}

pub fn cmd_endtrap(ctx: &Context) {
    let id = ctx.id();
    let cn = ctx.server().get_cn(id).unwrap_or(0) as i32;
    let server = ctx.server();
    let mut state = get_server_state(server).write().unwrap();

    let edit_state = match state.trap_edit.take() {
        Some(e) => e,
        None => {
            server.send_chan1(
                id,
                &ServMsg {
                    text: "No trap is currently being edited.".lossy_cubestring(),
                },
            );
            return;
        }
    };

    if let Some(ent_id) = state.trap_edit_particle_ent.take() {
        server.broadcast_chan1(&Client {
            client_num: crate::SERVER_BOT_CN as i32,
            packet: channel1::EditEnt {
                ent_id: ent_id as i32,
                pos: sauer_proto::data::Vec3 {
                    x: 0.0,
                    y: 0.0,
                    z: 0.0,
                },
                type_: sauer_proto::data::EntityType::Empty,
                attrs: [0; 5],
            },
        });

        if let Some(map) = state.map.as_mut() {
            if let Some(ent) = map.world.entities.get_mut(ent_id) {
                ent.ent_type = ogz_rs::data::EntityType::Empty;
                ent.attrs = [0; 5];
            }
        }
    }

    // Collect the minimal per-cell diff to get accurate (no artifact) tight bounds.
    let current_map = state.map.as_ref().unwrap();
    let world_size = current_map.world.size;
    let mut trigger_cells: Vec<([i32; 3], i32, sauer_map::Node)> = Vec::new();
    sauer_map::octree::collect_diff_cells(
        &edit_state.original_map.world.root,
        &current_map.world.root,
        [0, 0, 0],
        world_size,
        &mut trigger_cells,
    );

    // Completely filter out any cells that intersect the button's bounds!
    // This ensures the button geometry and payload are INDESTRUCTIBLE by the trap itself.
    let trap_pos = state.traps[edit_state.trap_idx].button_pos;
    let trap_size = state.traps[edit_state.trap_idx].button_size;
    let b_min = trap_pos;
    let b_max = [
        b_min[0] + trap_size,
        b_min[1] + trap_size,
        b_min[2] + trap_size,
    ];

    trigger_cells.retain(|&(pos, size, _)| {
        let c_min = pos;
        let c_max = [pos[0] + size, pos[1] + size, pos[2] + size];
        let intersects = (0..3).all(|i| c_min[i] < b_max[i] && c_max[i] > b_min[i]);
        !intersects
    });

    if trigger_cells.is_empty() && edit_state.affected_entities.is_empty() {
        server.broadcast_chan1(&ServMsg {
            text: "Trap edit ended with no changes detected.".lossy_cubestring(),
        });
        return;
    }

    // Compute tight bounding box from the actual changed cells (no apply_region artifacts).
    let mut tight_min = [i32::MAX; 3];
    let mut tight_max = [i32::MIN; 3];
    for &(pos, size, _) in &trigger_cells {
        for i in 0..3 {
            tight_min[i] = tight_min[i].min(pos[i]);
            tight_max[i] = tight_max[i].max(pos[i] + size);
        }
    }
    let tight_min = std::array::from_fn(|i| tight_min[i].max(0));
    let tight_max = std::array::from_fn(|i| tight_max[i].min(world_size));

    let (
        clip_pos,
        clip_s,
        clip_grid,
        trigger_unpack_len,
        trigger_compressed,
        restore_unpack_len,
        restore_compressed,
    ) = if !trigger_cells.is_empty() {
        // Trigger clipboard from the post-edit map (tight bounds only).
        let (cp, cs, cg, tul, tc) = sauer_map::clipboard::generate_region_clipboard(
            &current_map.world.root,
            world_size,
            tight_min,
            tight_max,
            &current_map.meta.vslots,
        );

        // Restore clipboard from original map (same region dims for consistency).
        let (rul, rc) = sauer_map::clipboard::generate_clipboard_for_region(
            &edit_state.original_map.world.root,
            world_size,
            cp,
            cs,
            cg,
            &edit_state.original_map.meta.vslots,
        );

        (cp, cs, cg, tul, tc, rul, rc)
    } else {
        ([0; 3], [0; 3], 1, 0, Vec::new(), 0, Vec::new())
    };

    // Collect trigger entities (post-edit state).
    let empty_ent = ogz_rs::data::Entity {
        position: [0.0; 3],
        attrs: [0; 5],
        ent_type: ogz_rs::data::EntityType::Empty,
        extra_data: vec![],
    };

    let mut trigger_entities_mod = Vec::new();
    let mut trigger_entities_add = Vec::new();

    for &ent_id in &edit_state.affected_entities {
        let curr = current_map.world.entities.get(ent_id).unwrap_or(&empty_ent);
        if ent_id < edit_state.original_map.world.entities.len() {
            trigger_entities_mod.push((ent_id, curr.clone()));
        } else if curr.ent_type != ogz_rs::data::EntityType::Empty {
            trigger_entities_add.push(curr.clone());
        }
    }

    // Compute reset (base-state) data so a save-while-active can be recovered.
    let reset_cells: Vec<([i32; 3], i32, sauer_map::Node)> = trigger_cells
        .iter()
        .map(|&(pos, size, _)| {
            let node = sauer_map::octree::extract_node_at(
                &edit_state.original_map.world.root,
                [0, 0, 0],
                world_size,
                pos,
                size,
            );
            (pos, size, node)
        })
        .collect();
    let reset_entities_mod: Vec<(usize, ogz_rs::data::Entity)> = trigger_entities_mod
        .iter()
        .map(|(id, _)| {
            let ent = edit_state
                .original_map
                .world
                .entities
                .get(*id)
                .cloned()
                .unwrap_or_else(|| empty_ent.clone());
            (*id, ent)
        })
        .collect();

    let stored_candidate = crate::trap::StoredTrap {
        button_pos: state.traps[edit_state.trap_idx].button_pos,
        button_size: state.traps[edit_state.trap_idx].button_size,
        button_orient: state.traps[edit_state.trap_idx].button_orient,
        trigger_clipboard: trigger_compressed.clone(),
        trigger_unpack_len,
        trigger_cells: trigger_cells.iter().map(|(pos, size, node)| {
            let mut packed = Vec::new();
            sauer_map::clipboard::pack_cube(node, &mut packed);
            (*pos, *size, packed)
        }).collect(),
        trigger_entities_mod: trigger_entities_mod.iter().map(|(id, e)| (*id, crate::trap::stored::StoredEntity::from(e))).collect(),
        trigger_entities_add: trigger_entities_add.iter().map(|e| crate::trap::stored::StoredEntity::from(e)).collect(),
        clip_pos,
        clip_s,
        clip_grid,
        time: state.traps[edit_state.trap_idx].time,
        timeout: state.traps[edit_state.trap_idx].timeout,
        reset_clipboard: restore_compressed.clone(),
        reset_unpack_len: restore_unpack_len,
        reset_cells: reset_cells.iter().map(|(pos, size, node)| {
            let mut packed = Vec::new();
            sauer_map::clipboard::pack_cube(node, &mut packed);
            (*pos, *size, packed)
        }).collect(),
        reset_entities_mod: reset_entities_mod.iter().map(|(id, e)| (*id, crate::trap::stored::StoredEntity::from(e))).collect(),
    };

    let raw_bytes = bitcode::serialize(&stored_candidate).unwrap_or_default();
    let mut zlib = Vec::new();
    {
        use std::io::Write;
        let mut enc = flate2::write::ZlibEncoder::new(&mut zlib, flate2::Compression::default());
        let _ = enc.write_all(&raw_bytes);
        let _ = enc.finish();
    }
    let payload_len = zlib.len() + 6;

    let mut is_too_large = false;
    if payload_len > 1_000_000 {
        is_too_large = true;
        server.send_chan1(id, &ServMsg {
            text: format!("\x0C3[ERROR]\x0C7 Trap edit discarded! Payload size ({} KB) exceeds the 1MB engine limit. Please make smaller edits.", payload_len / 1024).lossy_cubestring(),
        });
    } else {
        if payload_len > 600_000 {
            server.send_chan1(id, &ServMsg {
                text: format!("\x0C6[WARNING]\x0C7 Trap payload size is {} KB. Values over 1000 KB will be rejected to prevent map corruption.", payload_len / 1024).lossy_cubestring(),
            });
        }
        
        let trap = &mut state.traps[edit_state.trap_idx];
        trap.trigger_clipboard = trigger_compressed;
        trap.trigger_unpack_len = trigger_unpack_len;
        trap.trigger_cells = trigger_cells.clone();
        trap.trigger_entities_mod = trigger_entities_mod;
        trap.trigger_entities_add = trigger_entities_add;
        trap.clip_pos = clip_pos;
        trap.clip_s = clip_s;
        trap.clip_grid = clip_grid;
        trap.reset_clipboard = restore_compressed.clone();
        trap.reset_unpack_len = restore_unpack_len;
        trap.reset_cells = reset_cells;
        trap.reset_entities_mod = reset_entities_mod;
    }

    let target_cn = crate::SERVER_BOT_CN as i32;
    let region_sel = SelInfo {
        o: sauer_proto::data::Vec3 {
            x: clip_pos[0],
            y: clip_pos[1],
            z: clip_pos[2],
        },
        s: sauer_proto::data::Vec3 {
            x: clip_s[0],
            y: clip_s[1],
            z: clip_s[2],
        },
        grid: clip_grid,
        orient: sauer_proto::data::Orient::Left,
        cx: 0,
        cxs: 0,
        cy: 0,
        cys: 0,
        corner: 0,
    };

    // Revert clients: DelCube the tight region then paste original state.
    // (pasteblock in vanilla is transparent — needs DelCube first to clear added geometry.)
    if !trigger_cells.is_empty() {
        server.broadcast_chan1(&Client {
            client_num: target_cn,
            packet: channel1::DelCube {
                sel: region_sel.clone(),
            },
        });
        crate::geo_sync::broadcast_geometry(
            server,
            target_cn,
            &region_sel,
            restore_unpack_len,
            &restore_compressed,
        );
    }

    // Restore entities to pre-recording state.
    let mut batch = Vec::new();
    for &ent_id in &edit_state.affected_entities {
        let ent = edit_state
            .original_map
            .world
            .entities
            .get(ent_id)
            .unwrap_or(&empty_ent);
        let ent_type = ent.ent_type;
        batch.push(Client {
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
        if batch.len() >= 50 {
            server.broadcast_chan1(batch.clone());
            batch.clear();
        }
    }
    if !batch.is_empty() {
        server.broadcast_chan1(batch);
    }

    // Restore server geometry to original, keeping the expanded vslot table so indices
    // stay in sync with clients (recording may have appended new vslot entries).
    let merged_vslots = std::mem::take(&mut state.map.as_mut().unwrap().meta.vslots);
    state.map = Some(*edit_state.original_map);
    if let Some(map) = state.map.as_mut() {
        if merged_vslots.len() > map.meta.vslots.len() {
            map.meta.vslots = merged_vslots;
        }
        map.update_server_items(server);
    }
    drop(state);

    sync_traps_to_map(server, id);

    if !is_too_large {
        server.broadcast_chan1(&ServMsg {
            text: "Trap recorded and area reverted successfully.".lossy_cubestring(),
        });
    } else {
        server.broadcast_chan1(&ServMsg {
            text: "Area reverted. Trap changes were discarded due to payload size limits.".lossy_cubestring(),
        });
    }
}
