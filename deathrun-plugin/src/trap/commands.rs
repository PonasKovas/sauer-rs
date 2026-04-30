use crate::map_state::{Trap, get_server_state};
use cube_string::IntoCubeStringLossy;
use ogz_rs::data::Material;
use sauer_map::Selection;
use sauer_proto::{
    Client,
    channel1::{self, ServMsg},
    data::{EditFaceMode, SelInfo, Vec3},
};
use sauer_server::plugin::Context;
use super::{BUTTON_FRONT_TEX, BUTTON_SIDE_TEX, BUTTON_SIZE, TrapEditState};
use super::stored::sync_traps_to_map;

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
        o: Vec3 { x: trap.button_pos[0], y: trap.button_pos[1], z: trap.button_pos[2] },
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

    sync_traps_to_map(server, cn);

    let (adj_sel, _) =
        super::placement::get_selections(trap.button_pos, trap.button_size, trap.button_orient);
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
    adj[d] = if dc == 0 { snap_d - BUTTON_SIZE } else { snap_d };
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
        o: Vec3 { x: adj[0], y: adj[1], z: adj[2] },
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
    map.world.fill(&sel, true);
    map.world.set_texture(&sel, BUTTON_SIDE_TEX, true);
    map.world.set_texture(&sel, BUTTON_FRONT_TEX, false);
    map.world.set_material(
        &sel,
        Material { alpha: true, ..Default::default() },
    );

    let target_cn = cn as i32;
    let (adj_sel, fill_sel) =
        super::placement::get_selections(adj, BUTTON_SIZE, hit_orient);

    super::placement::apply_internal_vslots(map, &adj_sel);

    // Snapshot before corner pushes: clipboard delivers the flat button with
    // correct vslot indices; corner animation then pushes edges on clients.
    let button_node =
        sauer_map::octree::extract_node_at(&map.world.root, [0, 0, 0], ws, adj, BUTTON_SIZE);
    let (clip_unpack_len, clip_data) =
        sauer_map::clipboard::generate_clipboard(&button_node, BUTTON_SIZE, &map.meta.vslots);

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
        timeout: 0,
        reset_clipboard: Vec::new(),
        reset_unpack_len: 0,
        reset_cells: Vec::new(),
        reset_entities_mod: Vec::new(),
        is_active: false,
    });
    drop(state);

    sync_traps_to_map(&server, cn);

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
        packet: channel1::DelCube { sel: adj_sel.clone() },
    });
    crate::geo_sync::broadcast_geometry(&server, cn, &adj_sel, clip_unpack_len, &clip_data);

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
                    sel: SelInfo { corner, ..adj_sel.clone() },
                    dir: 1,
                    mode: EditFaceMode::Corner,
                },
            });
        }
    });
}

pub fn cmd_traptime(ctx: &Context, hit_pt: [f32; 3], time: u32) {
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
                &ServMsg { text: "Not looking at a trap.".lossy_cubestring() },
            );
            return;
        }
        Some(i) => i,
    };

    state.traps[trap_idx].time = time;
    server.send_chan1(
        cn,
        &ServMsg {
            text: format!("Trap time set to {}ms.", time).lossy_cubestring(),
        },
    );
    drop(state);
    sync_traps_to_map(server, cn);
}

pub fn cmd_traptimeout(ctx: &Context, hit_pt: [f32; 3], timeout: u32) {
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
                &ServMsg { text: "Not looking at a trap.".lossy_cubestring() },
            );
            return;
        }
        Some(i) => i,
    };

    state.traps[trap_idx].timeout = timeout;
    server.send_chan1(
        cn,
        &ServMsg {
            text: format!("Trap timeout set to {}ms.", timeout).lossy_cubestring(),
        },
    );
    drop(state);
    sync_traps_to_map(server, cn);
}

pub fn cmd_edittrap(ctx: &Context, hit_pt: [f32; 3]) {
    let cn = ctx.cn();
    let server = ctx.server();

    // Phase 1: validate, snapshot the base state, and collect existing trigger data.
    // All done under a single write lock that is released before apply_and_broadcast.
    let (trap_idx, original_map, trigger_data) = {
        let state = get_server_state(server).write().unwrap();

        if state.trap_edit.is_some() {
            server.send_chan1(
                cn,
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
                    cn,
                    &ServMsg { text: "Not looking at a trap.".lossy_cubestring() },
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
        server.broadcast_chan1(&ServMsg {
            text: format!(
                "Trap edit mode started for trap {}. Type #endtrap to finish and save.",
                trap_idx
            )
            .lossy_cubestring(),
        });
        return;
    };

    // Phase 2: apply the existing trigger state to the server map and broadcast to clients.
    let region_sel = SelInfo {
        o: Vec3 { x: clip_pos[0], y: clip_pos[1], z: clip_pos[2] },
        s: Vec3 { x: clip_s[0], y: clip_s[1], z: clip_s[2] },
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
        cn,
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

    server.broadcast_chan1(&ServMsg {
        text: format!(
            "Trap {} edit mode: existing trigger applied. Edit and #endtrap to save.",
            trap_idx
        )
        .lossy_cubestring(),
    });
}

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
                &ServMsg { text: "Not looking at a trap.".lossy_cubestring() },
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
                &ServMsg { text: "No trap is currently being edited.".lossy_cubestring() },
            );
            return;
        }
    };

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

    let target_cn = ctx.cn() as i32;
    let region_sel = SelInfo {
        o: sauer_proto::data::Vec3 { x: clip_pos[0], y: clip_pos[1], z: clip_pos[2] },
        s: sauer_proto::data::Vec3 { x: clip_s[0], y: clip_s[1], z: clip_s[2] },
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
            packet: channel1::DelCube { sel: region_sel.clone() },
        });
        crate::geo_sync::broadcast_geometry(
            server,
            ctx.cn(),
            &region_sel,
            restore_unpack_len,
            &restore_compressed,
        );
    }

    // Restore entities to pre-recording state.
    for &ent_id in &edit_state.affected_entities {
        let ent = edit_state
            .original_map
            .world
            .entities
            .get(ent_id)
            .unwrap_or(&empty_ent);
        let ent_type = ent.ent_type;
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

    sync_traps_to_map(server, ctx.cn());

    server.broadcast_chan1(&ServMsg {
        text: "Trap recorded and area reverted successfully.".lossy_cubestring(),
    });
}
