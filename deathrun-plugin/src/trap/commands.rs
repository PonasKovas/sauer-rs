use super::stored::sync_traps_to_map;
use super::{BUTTON_SIZE, TrapEditState};
use crate::map_state::{Trap, get_server_state};
use cube_string::IntoCubeStringLossy;
use sauer_map::Selection;
use sauer_proto::{
    Client,
    channel1::{self, ServMsg},
    data::Vec3,
};
use sauer_server::plugin::Context;

pub fn cmd_deltrap(ctx: &Context, hit_pt: [f32; 3]) {
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

    let b_max = [trap.button_pos[0] + trap.button_size, trap.button_pos[1] + trap.button_size, trap.button_pos[2] + trap.button_size];
    map.unprotect_region(trap.button_pos, b_max);

    map.fill(&sel, false);
    map.set_material(&sel, Default::default());
    
    let payloads = map.generate_restore_packets(trap.button_pos, b_max);
    
    drop(state);

    sync_traps_to_map(server, id);

    let s_clone = server.clone();
    let target_cn = crate::SERVER_BOT_CN as i32;
    tokio::spawn(async move {
        crate::geo_sync::broadcast_payloads(&s_clone, target_cn, payloads, 0).await;
    });
}

pub fn cmd_newtrap(ctx: &Context, hit_pt: [f32; 3], hit_orient: u8) {
    let id = ctx.id();
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

    let ws = map.size;
    if adj.iter().any(|&v| v < 0 || v >= ws)
        || adj[r_axis] + BUTTON_SIZE > ws
        || adj[c_axis] + BUTTON_SIZE > ws
    {
        return;
    }

    let target_cn = crate::SERVER_BOT_CN as i32;
    crate::trap::placement::build_button_geometry(map, adj, BUTTON_SIZE, hit_orient);

    let b_max = [adj[0] + BUTTON_SIZE, adj[1] + BUTTON_SIZE, adj[2] + BUTTON_SIZE];
    map.protect_region(adj, b_max);

    let payloads = map.generate_restore_packets(adj, b_max);

    state.traps.push(Trap {
        button_pos: adj,
        button_size: BUTTON_SIZE,
        button_orient: hit_orient,
        trigger_payload: None,
        trigger_entities: Vec::new(),
        clip_pos: [0; 3],
        clip_s: [0; 3],
        clip_grid: 1,
        time: 2000,
        timeout: 20000,
        is_active: false,
    });
    drop(state);

    let s_clone = server.clone();
    tokio::spawn(async move {
        crate::geo_sync::broadcast_payloads(&s_clone, target_cn, payloads, 0).await;
        crate::trap::stored::sync_traps_to_map(&s_clone, id);
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

    let payload = sauer_map::Map::db_compress(&stored).unwrap_or_default();
    let payload_len = payload.len();

    let total_chunks = (payload_len + 15) / 16;

    let mut size_on_disk = 0;
    let mut size_in_mem = 0;
    for idx in 0..total_chunks {
        let name_len = sauer_map::Map::db_encode_chunk_name(idx).len();
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

    let (trap_idx, original_map, trigger_payload, trigger_entities, clip_pos, clip_s, clip_grid) = {
        let state = get_server_state(server).read().unwrap();

        if state.trap_edit.is_some() {
            server.send_chan1(id, &ServMsg { text: "A trap is already being edited. Use #endtrap first.".lossy_cubestring() });
            return;
        }

        let trap_idx = match state.traps.iter().position(|t| {
            (0..3).all(|ax| hit_pt[ax] >= t.button_pos[ax] as f32 && hit_pt[ax] < (t.button_pos[ax] + t.button_size) as f32)
        }) {
            None => {
                server.send_chan1(id, &ServMsg { text: "Not looking at a trap.".lossy_cubestring() });
                return;
            }
            Some(i) => i,
        };

        let trap = &state.traps[trap_idx];
        (trap_idx, Box::new(state.map.clone().unwrap()), trap.trigger_payload.clone(), trap.trigger_entities.clone(), trap.clip_pos, trap.clip_s, trap.clip_grid)
    };

    if clip_s != [0, 0, 0] {
        if let Some(payload) = &trigger_payload {
            let min = clip_pos;
            let max = [min[0] + clip_s[0]*clip_grid, min[1] + clip_s[1]*clip_grid, min[2] + clip_s[2]*clip_grid];
            
            let mut diff_ent_ids = std::collections::HashSet::new();
            {
                let mut state = get_server_state(server).write().unwrap();
                let map = state.map.as_mut().unwrap();

                for (id, ent) in map.entities.iter_mut().enumerate() {
                    if ent.ent_type != sauer_proto::data::EntityType::Empty && super::trigger::in_aabb(ent.position, min, max) {
                        ent.ent_type = sauer_proto::data::EntityType::Empty;
                        ent.attrs = [0; 5];
                        diff_ent_ids.insert(id);
                    }
                }

                for new_ent in &trigger_entities {
                    if let Some(id) = map.entities.iter().position(|e| e.ent_type == sauer_proto::data::EntityType::Empty) {
                        map.entities[id] = new_ent.clone();
                        diff_ent_ids.insert(id);
                    } else {
                        let id = map.entities.len();
                        map.entities.push(new_ent.clone());
                        diff_ent_ids.insert(id);
                    }
                }

                let mut paste_sel = payload.sel.clone();
                paste_sel.o = sauer_proto::data::Vec3 { x: min[0], y: min[1], z: min[2] };
                map.apply_block(&paste_sel, payload.nodes.clone(), false);
                map.invalidate_cache();
                crate::map_state::update_server_items(map, server);
            }
            
            let actor_cn = crate::SERVER_BOT_CN as i32;
            let mut batch = Vec::new();
            for id in diff_ent_ids {
                let state = get_server_state(server).read().unwrap();
                let ent = &state.map.as_ref().unwrap().entities[id];
                batch.push(Client {
                    client_num: actor_cn,
                    packet: channel1::EditEnt {
                        ent_id: id as i32,
                        pos: sauer_proto::data::Vec3 { x: ent.position[0], y: ent.position[1], z: ent.position[2] },
                        type_: ent.ent_type,
                        attrs: ent.attrs.map(|x| x as i32),
                    }
                });
                if batch.len() >= 50 {
                    server.broadcast_chan1(batch.clone());
                    batch.clear();
                }
            }
            if !batch.is_empty() {
                server.broadcast_chan1(batch);
            }

            let payloads = {
                let state = get_server_state(server).read().unwrap();
                sauer_map::chunking::chunk_region(state.map.as_ref().unwrap(), min, max, 4000)
            };
            let s_clone = server.clone();
            let target_cn = crate::SERVER_BOT_CN as i32;
            tokio::spawn(async move {
                crate::geo_sync::broadcast_payloads(&s_clone, target_cn, payloads, 0).await;
            });
        }
    }

    {
        let mut state = get_server_state(server).write().unwrap();
        state.trap_edit = Some(TrapEditState { trap_idx, original_map, is_edit: true });
    }

    crate::ui::broadcast_trap_edit_started(server, trap_idx);
}

pub fn cmd_settrap(ctx: &Context, hit_pt: [f32; 3]) {
    let id = ctx.id();
    let server = ctx.server();
    let mut state = get_server_state(server).write().unwrap();

    if state.trap_edit.is_some() {
        server.send_chan1(id, &ServMsg { text: "A trap is already being edited. Use #endtrap first.".lossy_cubestring() });
        return;
    }

    let trap_idx = match state.traps.iter().position(|t| {
        (0..3).all(|ax| hit_pt[ax] >= t.button_pos[ax] as f32 && hit_pt[ax] < (t.button_pos[ax] + t.button_size) as f32)
    }) {
        None => {
            server.send_chan1(id, &ServMsg { text: "Not looking at a trap.".lossy_cubestring() });
            return;
        }
        Some(i) => i,
    };

    let original_map = Box::new(state.map.clone().unwrap());
    state.trap_edit = Some(TrapEditState { trap_idx, original_map, is_edit: false });
    drop(state);

    crate::ui::broadcast_trap_edit_started(server, trap_idx);
}

pub fn cmd_endtrap(ctx: &Context) {
    let id = ctx.id();
    let server = ctx.server();
    let mut state = get_server_state(server).write().unwrap();

    let edit_state = match state.trap_edit.take() {
        Some(e) => e,
        None => {
            server.send_chan1(id, &ServMsg { text: "No trap is currently being edited.".lossy_cubestring() });
            return;
        }
    };

    let mut glob = Vec::new();
    let mut pers = Vec::new();
    if let Some(map) = state.map.as_mut() {
        let up = map.clear_virt_ents(id, "edit_particle");
        glob = up.global_broadcast;
        pers = up.personal;
    }
    let cn = crate::SERVER_BOT_CN as i32;
    if !glob.is_empty() {
        server.broadcast_chan1(glob.into_iter().map(|p| Client { client_num: cn, packet: p }).collect::<Vec<_>>());
    }
    if !pers.is_empty() {
        server.send_chan1(id, pers.into_iter().map(|p| Client { client_num: cn, packet: p }).collect::<Vec<_>>());
    }

    let current_map = state.map.as_ref().unwrap();
    let world_size = current_map.size;
    let empty_ent = ogz_rs::data::Entity { position: [0.0; 3], attrs: [0; 5], ent_type: ogz_rs::data::EntityType::Empty, extra_data: vec![] };

    let mut tight_min = [i32::MAX; 3];
    let mut tight_max = [i32::MIN; 3];

    let mut diff_cells: Vec<([i32; 3], i32, sauer_map::Node)> = Vec::new();
    sauer_map::octree::collect_diff_cells(&edit_state.original_map.root, &current_map.root, [0, 0, 0], world_size, &mut diff_cells);
    
    for &(pos, size, _) in &diff_cells {
        for i in 0..3 {
            tight_min[i] = tight_min[i].min(pos[i]);
            tight_max[i] = tight_max[i].max(pos[i] + size);
        }
    }

    let mut diff_ent_ids = std::collections::HashSet::new();
    for i in 0..current_map.entities.len().max(edit_state.original_map.entities.len()) {
        if current_map.virt_state.global_pool.contains(&i) { continue; } // Ignore virtual entities
        let c = current_map.entities.get(i).unwrap_or(&empty_ent);
        let o = edit_state.original_map.entities.get(i).unwrap_or(&empty_ent);
        if c != o {
            diff_ent_ids.insert(i);
            if c.ent_type != ogz_rs::data::EntityType::Empty {
                for j in 0..3 {
                    tight_min[j] = tight_min[j].min(c.position[j].floor() as i32);
                    tight_max[j] = tight_max[j].max(c.position[j].ceil() as i32 + 1);
                }
            }
            if o.ent_type != ogz_rs::data::EntityType::Empty {
                for j in 0..3 {
                    tight_min[j] = tight_min[j].min(o.position[j].floor() as i32);
                    tight_max[j] = tight_max[j].max(o.position[j].ceil() as i32 + 1);
                }
            }
        }
    }

    if edit_state.is_edit {
        let trap = &state.traps[edit_state.trap_idx];
        if trap.clip_s != [0, 0, 0] {
            let o_min = trap.clip_pos;
            let o_max = [
                o_min[0] + trap.clip_s[0] * trap.clip_grid,
                o_min[1] + trap.clip_s[1] * trap.clip_grid,
                o_min[2] + trap.clip_s[2] * trap.clip_grid,
            ];
            for i in 0..3 {
                tight_min[i] = tight_min[i].min(o_min[i]);
                tight_max[i] = tight_max[i].max(o_max[i]);
            }
        }
    }

    if tight_min[0] == i32::MAX {
        server.broadcast_chan1(&ServMsg { text: "Trap edit ended with no changes detected.".lossy_cubestring() });
        return;
    }

    let tight_min = std::array::from_fn(|i| tight_min[i].max(0));
    let tight_max = std::array::from_fn(|i| tight_max[i].min(world_size));

    let (clip_pos, clip_s, clip_grid, trigger_payload) = sauer_map::clipboard::generate_region_clipboard(current_map, tight_min, tight_max);

    let mut trigger_entities = Vec::new();
    for ent in &current_map.entities {
        if ent.ent_type != ogz_rs::data::EntityType::Empty && super::trigger::in_aabb(ent.position, tight_min, tight_max) {
            trigger_entities.push(ent.clone());
        }
    }

    let stored_candidate = crate::trap::StoredTrap {
        button_pos: state.traps[edit_state.trap_idx].button_pos,
        button_size: state.traps[edit_state.trap_idx].button_size,
        button_orient: state.traps[edit_state.trap_idx].button_orient,
        trigger_payload: Some(trigger_payload.clone()),
        trigger_entities: trigger_entities.iter().map(crate::trap::stored::StoredEntity::from).collect(),
        clip_pos, clip_s, clip_grid,
        time: state.traps[edit_state.trap_idx].time,
        timeout: state.traps[edit_state.trap_idx].timeout,
    };

    let payload = sauer_map::Map::db_compress(&stored_candidate).unwrap_or_default();
    let payload_len = payload.len();
    let mut is_too_large = false;

    if payload_len > 1_000_000 {
        is_too_large = true;
        server.send_chan1(id, &ServMsg { text: format!("\x0C3[ERROR]\x0C7 Trap edit discarded! Payload size ({} KB) exceeds 1MB limit.", payload_len / 1024).lossy_cubestring() });
    } else {
        if payload_len > 600_000 {
            server.send_chan1(id, &ServMsg { text: format!("\x0C6[WARNING]\x0C7 Trap payload size is {} KB. Keep edits smaller.", payload_len / 1024).lossy_cubestring() });
        }
        let trap = &mut state.traps[edit_state.trap_idx];
        trap.trigger_payload = Some(trigger_payload);
        trap.trigger_entities = trigger_entities;
        trap.clip_pos = clip_pos;
        trap.clip_s = clip_s;
        trap.clip_grid = clip_grid;
    }

    let target_cn = crate::SERVER_BOT_CN as i32;
    let merged_vslots = std::mem::take(&mut state.map.as_mut().unwrap().vslots);
    let virt_state = std::mem::take(&mut state.map.as_mut().unwrap().virt_state);
    state.map = Some(*edit_state.original_map.clone());
    if let Some(map) = state.map.as_mut() {
        if merged_vslots.len() > map.vslots.len() { map.vslots = merged_vslots; }
        map.virt_state = virt_state;
        crate::map_state::update_server_items(map, server);
    }
    drop(state);

    let mut batch = Vec::new();
    for &ent_id in &diff_ent_ids {
        let ent = edit_state.original_map.entities.get(ent_id).unwrap_or(&empty_ent);
        batch.push(Client {
            client_num: target_cn,
            packet: channel1::EditEnt {
                ent_id: ent_id as i32,
                pos: sauer_proto::data::Vec3 { x: ent.position[0], y: ent.position[1], z: ent.position[2] },
                type_: ent.ent_type,
                attrs: ent.attrs.map(|x| x as i32),
            },
        });
        if batch.len() >= 50 {
            server.broadcast_chan1(batch.clone());
            batch.clear();
        }
    }
    if !batch.is_empty() { server.broadcast_chan1(batch); }

    if diff_cells.len() > 0 {
        let payloads = sauer_map::chunking::chunk_region(&edit_state.original_map, tight_min, tight_max, 4000);
        let s_clone = server.clone();
        tokio::spawn(async move { crate::geo_sync::broadcast_payloads(&s_clone, target_cn, payloads, 0).await; });
    }

    sync_traps_to_map(server, id);

    if !is_too_large {
        server.broadcast_chan1(&ServMsg { text: "Trap recorded and area reverted successfully.".lossy_cubestring() });
    }
}
