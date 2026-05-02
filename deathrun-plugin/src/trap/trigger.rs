use crate::map_state::get_server_state;
use sauer_proto::{
    Client, channel1,
    data::{Orient, SelInfo, Vec3},
};
use sauer_server::Server;

use super::placement::{get_selections, pack_vslot_bytes};
use super::{BUTTON_COLOR_ACTIVE, BUTTON_COLOR_READY};

pub fn trigger_trap(server: &std::sync::Arc<Server>, trap_idx: usize, actor_cn: u32) {
    let mut state = get_server_state(server).write().unwrap();
    if trap_idx >= state.traps.len() {
        return;
    }
    let trap = &mut state.traps[trap_idx];

    if trap.is_active {
        return;
    }
    if trap.trigger_clipboard.is_empty()
        && trap.trigger_entities_mod.is_empty()
        && trap.trigger_entities_add.is_empty()
    {
        return;
    }

    trap.is_active = true;

    let trigger_clipboard = trap.trigger_clipboard.clone();
    let trigger_unpack_len = trap.trigger_unpack_len;
    let trigger_cells = trap.trigger_cells.clone();
    let trigger_entities_mod = trap.trigger_entities_mod.clone();
    let trigger_entities_add = trap.trigger_entities_add.clone();
    let clip_pos = trap.clip_pos;
    let clip_s = trap.clip_s;
    let clip_grid = trap.clip_grid;

    let button_pos = trap.button_pos;
    let button_size = trap.button_size;
    let button_orient = trap.button_orient;
    let trap_time = trap.time;
    let trap_timeout = trap.timeout;

    let is_edit_mode = *server.game_mode.read().unwrap() == sauer_proto::data::GameMode::CoopEdit;
    let effective_timeout = if is_edit_mode || trap_timeout == 0 {
        trap_time
    } else {
        trap_timeout.max(trap_time)
    };

    drop(state);

    let server_clone = std::sync::Arc::clone(server);
    tokio::spawn(async move {
        let ent_id = {
            let mut state = get_server_state(&server_clone).write().unwrap();
            let mut id = None;
            if let Some(map) = state.map.as_mut() {
                let blank = ogz_rs::data::Entity {
                    position: [0.0, 0.0, -10000.0],
                    attrs: [0; 5],
                    ent_type: ogz_rs::data::EntityType::Particles, // Secured immediately
                    extra_data: vec![],
                };
                let i = map
                    .world
                    .entities
                    .iter()
                    .position(|e| e.ent_type == ogz_rs::data::EntityType::Empty)
                    .unwrap_or_else(|| {
                        let idx = map.world.entities.len();
                        map.world.entities.push(blank.clone());
                        idx
                    });
                map.world.entities[i] = blank;
                id = Some(i);
            }
            id
        };

        // Calculate progress bar position to be exactly 0.5 units off the face
        let mut px = button_pos[0] as f32 + button_size as f32 / 2.0;
        let mut py = button_pos[1] as f32 + button_size as f32 / 2.0;
        let mut pz = button_pos[2] as f32 + button_size as f32 / 2.0;

        let d = (button_orient >> 1) as usize;
        let dc = button_orient & 1;
        let offset = if dc == 0 {
            -0.5
        } else {
            button_size as f32 + 0.5
        };

        if d == 0 {
            px = button_pos[0] as f32 + offset;
        } else if d == 1 {
            py = button_pos[1] as f32 + offset;
        } else if d == 2 {
            pz = button_pos[2] as f32 + offset;
        }

        let target_cns = {
            let is_play_mode =
                *server_clone.game_mode.read().unwrap() != sauer_proto::data::GameMode::CoopEdit;
            if is_play_mode {
                let maps = server_clone
                    .extra
                    .get::<std::sync::Arc<std::sync::RwLock<crate::play::PlayMaps>>>()
                    .unwrap();
                let lock = maps.read().unwrap();
                lock.evil_cn.into_iter().collect::<Vec<_>>()
            } else {
                server_clone.client_cns()
            }
        };

        // Send initial 100% progress BEFORE applying trigger (beats client PasteServer freeze)
        if let Some(id) = ent_id {
            let color = (0 << 8) | (15 << 4); // Full green
            for &cn in &target_cns {
                server_clone.send_chan1(
                    cn,
                    &sauer_proto::Client {
                        client_num: actor_cn as i32,
                        packet: sauer_proto::channel1::EditEnt {
                            ent_id: id as i32,
                            pos: sauer_proto::data::Vec3 {
                                x: px,
                                y: py,
                                z: pz,
                            },
                            type_: sauer_proto::data::EntityType::Particles,
                            attrs: [5, 100, color as i32, 0, 0],
                        },
                    },
                );
            }
        }

        // Snapshot restore data from current (pre-trigger) map state.
        let (restore_unpack_len, restore_compressed, restore_cells, restore_entities_mod) = {
            let state = get_server_state(&server_clone).read().unwrap();
            let map = match state.map.as_ref() {
                Some(m) => m,
                None => return,
            };
            let (unpack_len, compressed) = if !trigger_cells.is_empty() {
                sauer_map::clipboard::generate_clipboard_for_region(
                    &map.world.root,
                    map.world.size,
                    clip_pos,
                    clip_s,
                    clip_grid,
                    &map.meta.vslots,
                )
            } else {
                (0, Vec::new())
            };
            let cells: Vec<([i32; 3], i32, sauer_map::Node)> = trigger_cells
                .iter()
                .map(|&(pos, size, _)| {
                    let node = sauer_map::octree::extract_node_at(
                        &map.world.root,
                        [0, 0, 0],
                        map.world.size,
                        pos,
                        size,
                    );
                    (pos, size, node)
                })
                .collect();
            let mut r_ents = Vec::new();
            for &(id, _) in &trigger_entities_mod {
                if let Some(ent) = map.world.entities.get(id) {
                    r_ents.push((id, ent.clone()));
                }
            }
            (unpack_len, compressed, cells, r_ents)
        };

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
            orient: Orient::Left,
            cx: 0,
            cxs: 0,
            cy: 0,
            cys: 0,
            corner: 0,
        };

        // --- 1. APPLY TRIGGER ---
        let assigned_ids = apply_and_broadcast(
            &server_clone,
            actor_cn,
            &region_sel,
            trigger_unpack_len,
            &trigger_clipboard,
            &trigger_cells,
            &trigger_entities_mod,
            &trigger_entities_add,
        );

        if is_edit_mode {
            let mut state = get_server_state(&server_clone).write().unwrap();
            state.active_traps.push(crate::trap::ActiveTrap {
                trap_idx,
                progress_bar_ent_id: ent_id,
                assigned_ids: assigned_ids.clone(),
            });
            let active_traps = state.active_traps.clone();
            if let Some(map) = state.map.as_mut() {
                let changed = crate::entity_store::write(
                    &mut map.world,
                    &active_traps,
                    crate::trap::ACTIVE_TRAP_STORE_VERSION,
                    crate::trap::ACTIVE_TRAP_DOMAIN,
                )
                .unwrap_or_default();
                for (id, ent) in changed {
                    let ent_type = ent.ent_type;
                    server_clone.broadcast_chan1(&sauer_proto::Client {
                        client_num: actor_cn as i32,
                        packet: sauer_proto::channel1::EditEnt {
                            ent_id: id as i32,
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
            }
        }

        if let Some(epos) = server_clone.players.try_get(actor_cn).map(|p| p.position) {
            for cn in server_clone.client_cns() {
                if let Some(p) = server_clone.players.try_get(cn) {
                    let d = [
                        p.position.x - epos.x,
                        p.position.y - epos.y,
                        p.position.z - epos.z,
                    ];
                    let dist_sq = d[0] * d[0] + d[1] * d[1] + d[2] * d[2];
                    if dist_sq < 500.0 * 500.0 {
                        server_clone.send_chan1(
                            cn,
                            &sauer_proto::Client {
                                client_num: actor_cn as i32,
                                packet: sauer_proto::channel1::Sound { sound_id: 64 },
                            },
                        );
                    }
                }
            }
        }

        let empty_ent = ogz_rs::data::Entity {
            position: [0.0; 3],
            attrs: [0; 5],
            ent_type: ogz_rs::data::EntityType::Empty,
            extra_data: vec![],
        };
        let mut final_restore_ents = restore_entities_mod.clone();
        for id in &assigned_ids {
            final_restore_ents.push((*id, empty_ent.clone()));
        }

        set_button_color(
            &server_clone,
            actor_cn,
            button_pos,
            button_size,
            button_orient,
            BUTTON_COLOR_ACTIVE,
        );

        let start = tokio::time::Instant::now();
        let total_dur = std::time::Duration::from_millis(effective_timeout as u64);
        let revert_dur = std::time::Duration::from_millis(trap_time as u64);
        let mut reverted = false;
        let mut last_progress = 100;

        let mut interval = tokio::time::interval(std::time::Duration::from_millis(33));

        loop {
            // Check if map was reloaded or round reset logic deleted the trap.
            let is_valid = {
                let state = get_server_state(&server_clone).read().unwrap();
                state.traps.get(trap_idx).map(|t| t.is_active) == Some(true)
            };
            if !is_valid {
                return; // Abort immediately, map changed!
            }

            let elapsed = start.elapsed();
            if elapsed >= total_dur {
                break;
            }

            if !reverted && elapsed >= revert_dur {
                apply_and_broadcast(
                    &server_clone,
                    actor_cn,
                    &region_sel,
                    restore_unpack_len,
                    &restore_compressed,
                    &restore_cells,
                    &final_restore_ents,
                    &[],
                );
                if effective_timeout > trap_time {
                    // Force button color back to ACTIVE for the duration of the timeout cooldown.
                    set_button_color(
                        &server_clone,
                        actor_cn,
                        button_pos,
                        button_size,
                        button_orient,
                        BUTTON_COLOR_ACTIVE,
                    );
                }
                reverted = true;
            }

            if let Some(id) = ent_id {
                let remaining = total_dur.saturating_sub(elapsed).as_millis() as i32;
                let progress = (remaining * 100 / effective_timeout as i32).clamp(0, 100);

                if progress != last_progress {
                    last_progress = progress;

                    let g = (progress * 15 / 100).clamp(0, 15);
                    let r = ((100 - progress) * 15 / 100).clamp(0, 15);
                    let color = (r << 8) | (g << 4);

                    for &cn in &target_cns {
                        server_clone.send_chan1(
                            cn,
                            &sauer_proto::Client {
                                client_num: actor_cn as i32,
                                packet: sauer_proto::channel1::EditEnt {
                                    ent_id: id as i32,
                                    pos: sauer_proto::data::Vec3 {
                                        x: px,
                                        y: py,
                                        z: pz,
                                    },
                                    type_: sauer_proto::data::EntityType::Particles,
                                    attrs: [5, progress as i32, color as i32, 0, 0],
                                },
                            },
                        );
                    }
                }
            }
            interval.tick().await;
        }

        if !reverted {
            apply_and_broadcast(
                &server_clone,
                actor_cn,
                &region_sel,
                restore_unpack_len,
                &restore_compressed,
                &restore_cells,
                &final_restore_ents,
                &[],
            );
        }

        if let Some(id) = ent_id {
            let mut state = get_server_state(&server_clone).write().unwrap();
            if let Some(map) = state.map.as_mut() {
                if let Some(ent) = map.world.entities.get_mut(id) {
                    ent.ent_type = ogz_rs::data::EntityType::Empty;
                    ent.attrs = [0; 5];
                }
            }
            drop(state);

            for &cn in &target_cns {
                server_clone.send_chan1(
                    cn,
                    &sauer_proto::Client {
                        client_num: actor_cn as i32,
                        packet: sauer_proto::channel1::EditEnt {
                            ent_id: id as i32,
                            pos: sauer_proto::data::Vec3 {
                                x: 0.0,
                                y: 0.0,
                                z: 0.0,
                            },
                            type_: sauer_proto::data::EntityType::Empty,
                            attrs: [0; 5],
                        },
                    },
                );
            }
        }

        set_button_color(
            &server_clone,
            actor_cn,
            button_pos,
            button_size,
            button_orient,
            BUTTON_COLOR_READY,
        );

        if let Ok(mut state) = get_server_state(&server_clone).write() {
            if trap_idx < state.traps.len() {
                state.traps[trap_idx].is_active = false;
            }
            if is_edit_mode {
                state.active_traps.retain(|t| t.trap_idx != trap_idx);
                let active_traps = state.active_traps.clone();
                if let Some(map) = state.map.as_mut() {
                    let changed = crate::entity_store::write(
                        &mut map.world,
                        &active_traps,
                        crate::trap::ACTIVE_TRAP_STORE_VERSION,
                        crate::trap::ACTIVE_TRAP_DOMAIN,
                    )
                    .unwrap_or_default();
                    for (id, ent) in changed {
                        let ent_type = ent.ent_type;
                        server_clone.broadcast_chan1(&sauer_proto::Client {
                            client_num: actor_cn as i32,
                            packet: sauer_proto::channel1::EditEnt {
                                ent_id: id as i32,
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
                }
            }
        }
    });
}

pub fn set_button_color(
    server: &Server,
    actor_cn: u32,
    pos: [i32; 3],
    size: i32,
    orient: u8,
    color: [f32; 3],
) {
    let (adj_sel, _) = get_selections(pos, size, orient);
    let color_bytes: Vec<u8> = color.iter().flat_map(|f| f.to_le_bytes()).collect();
    let data = pack_vslot_bytes(&[(7, &color_bytes)]);
    let packet = channel1::EditVSlot {
        sel: adj_sel,
        delta: 0,
        all_faces: true,
        extra_data: data,
    };

    if let Ok(mut state) = get_server_state(server).write() {
        if let Some(map) = state.map.as_mut() {
            map.handle_edit_v_slot(&packet);
        }
    }

    server.broadcast_chan1(&Client {
        client_num: actor_cn as i32,
        packet,
    });
}

pub fn apply_and_broadcast(
    server: &Server,
    actor_cn: u32,
    sel: &SelInfo,
    unpack_len: i32,
    data: &[u8],
    cells: &[([i32; 3], i32, sauer_map::Node)],
    entities_mod: &[(usize, ogz_rs::data::Entity)],
    entities_add: &[ogz_rs::data::Entity],
) -> Vec<usize> {
    let mut state = get_server_state(server).write().unwrap();
    let map = match state.map.as_mut() {
        Some(m) => m,
        None => return Vec::new(),
    };

    for &(pos, size, ref node) in cells {
        map.world.set_node_at(pos, size, node.clone());
    }

    let mut assigned_ids = Vec::new();
    let mut broadcast_ents = entities_mod.to_vec();

    for &(ent_id, ref ent) in entities_mod {
        if ent_id < map.world.entities.len() {
            map.world.entities[ent_id] = ent.clone();
        }
    }

    for ent in entities_add {
        let mut found_empty = None;
        for (i, e) in map.world.entities.iter().enumerate() {
            if e.ent_type == ogz_rs::data::EntityType::Empty {
                found_empty = Some(i);
                break;
            }
        }

        let ent_id = if let Some(i) = found_empty {
            map.world.entities[i] = ent.clone();
            i
        } else {
            let i = map.world.entities.len();
            map.world.entities.push(ent.clone());
            i
        };

        assigned_ids.push(ent_id);
        broadcast_ents.push((ent_id, ent.clone()));
    }

    map.invalidate_cache();
    map.update_server_items(server);
    drop(state);

    if !data.is_empty() {
        server.broadcast_chan1(&Client {
            client_num: actor_cn as i32,
            packet: channel1::DelCube { sel: sel.clone() },
        });
        crate::geo_sync::broadcast_geometry(server, actor_cn as i32, sel, unpack_len, data);
    }

    for (ent_id, ent) in broadcast_ents {
        let ent_type = ent.ent_type;
        server.broadcast_chan1(&Client {
            client_num: actor_cn as i32,
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

    assigned_ids
}
