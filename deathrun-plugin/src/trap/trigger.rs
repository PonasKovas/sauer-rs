use crate::map_state::get_server_state;
use sauer_proto::{Client, channel1, data::Vec3};
use sauer_server::Server;

use super::{BUTTON_COLOR_ACTIVE, BUTTON_COLOR_READY, Trap};

/// Turns the base state of the trap's area into its active state. Returns the
/// entity slots it touched.
pub fn apply_active_state(
    map: &mut sauer_map::Map,
    trap: &Trap,
    virt_pool: &[usize],
) -> Vec<usize> {
    let touched = super::ents::apply(&mut map.entities, &trap.ents, virt_pool);
    if let Some(geom) = &trap.geom {
        let mut sel = geom.payload.sel.clone();
        sel.o = Vec3 {
            x: geom.clip_pos[0],
            y: geom.clip_pos[1],
            z: geom.clip_pos[2],
        };
        map.apply_block(&sel, geom.payload.nodes.clone(), false);
    }
    touched
}

pub fn recompose_traps(server: &std::sync::Arc<Server>) {
    let mut state = get_server_state(server).write().unwrap();

    let base_map = match state.base_map.as_ref() {
        Some(b) => b.clone(),
        None => return, // Nothing to recompose
    };

    let virt_state = std::mem::take(&mut state.map.as_mut().unwrap().virt_state);
    let old_map = state.map.clone().unwrap();
    let mut new_map = *base_map;

    // Apply all active traps in chronological order
    for &t_idx in &state.active_trap_stack {
        apply_active_state(&mut new_map, &state.traps[t_idx], &virt_state.global_pool);
    }

    // A trap's geometry can cover other traps' buttons and paste them back as
    // they were when it was recorded, so redo every button in its current color.
    for trap in &state.traps {
        let (pos, size, orient) = (trap.button_pos, trap.button_size, trap.button_orient);
        crate::trap::placement::build_button_geometry(&mut new_map, pos, size, orient);
        if trap.is_active {
            for pkt in crate::trap::placement::get_outer_faces_vslot_packets(
                pos,
                size,
                orient,
                BUTTON_COLOR_ACTIVE,
            ) {
                new_map.apply_vslot_edit(&pkt);
            }
        }
    }

    new_map.invalidate_cache();
    let world_size = new_map.size;

    // Changed areas, one per trap frame or button, so a button far from its
    // trap doesn't make everything between them get pasted.
    let areas: Vec<_> = state
        .traps
        .iter()
        .flat_map(|t| {
            t.geom
                .as_ref()
                .map(|g| g.clip_box())
                .into_iter()
                .chain([t.button_box()])
        })
        .collect();
    let changed = changed_boxes(&old_map.root, &new_map.root, world_size, &areas);

    // Compute entity changes
    let mut diff_ent_ids = std::collections::HashSet::new();
    let empty_ent = ogz_rs::data::Entity {
        position: [0.0; 3],
        attrs: [0; 5],
        ent_type: ogz_rs::data::EntityType::Empty,
        extra_data: vec![],
    };
    for i in 0..new_map.entities.len().max(old_map.entities.len()) {
        if virt_state.global_pool.contains(&i) {
            continue;
        } // Ignore virtual entities
        let c = new_map.entities.get(i).unwrap_or(&empty_ent);
        let o = old_map.entities.get(i).unwrap_or(&empty_ent);
        if c != o {
            diff_ent_ids.insert(i);
        }
    }

    new_map.virt_state = virt_state;
    new_map.reconcile_virtual_entities();
    state.map = Some(new_map);
    crate::map_state::update_server_items(state.map.as_ref().unwrap(), server);

    // Network the changes
    let actor_cn = crate::SERVER_BOT_CN as i32;

    let mut batch = Vec::new();
    for &id in &diff_ent_ids {
        let ent = state
            .map
            .as_ref()
            .unwrap()
            .entities
            .get(id)
            .unwrap_or(&empty_ent);
        batch.push(Client {
            client_num: actor_cn,
            packet: channel1::EditEnt {
                ent_id: id as i32,
                pos: Vec3 {
                    x: ent.position[0],
                    y: ent.position[1],
                    z: ent.position[2],
                },
                type_: ent.ent_type,
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

    if !changed.is_empty() {
        let map = state.map.as_ref().unwrap();
        let payloads = changed
            .into_iter()
            .flat_map(|(min, max)| sauer_map::chunking::chunk_region(map, min, max, usize::MAX))
            .collect();

        let s_clone = server.clone();
        tokio::spawn(async move {
            crate::geo_sync::broadcast_payloads(&s_clone, actor_cn, payloads, 0).await;
        });
    }
}

pub fn trap_triggerable(state: &crate::map_state::DeathrunState, trap_idx: usize) -> bool {
    // A button being moved is off limits until it is placed.
    state.mode.editing().is_none()
        && state
            .mode
            .placing()
            .is_none_or(|p| p.trap != Some(trap_idx))
}

pub fn trigger_trap(server: &std::sync::Arc<Server>, trap_idx: usize, actor_id: u64) {
    let mut state = get_server_state(server).write().unwrap();
    if !trap_triggerable(&state, trap_idx) {
        return;
    }

    if trap_idx >= state.traps.len() {
        return;
    }

    if state.traps[trap_idx].is_active || !state.traps[trap_idx].does_anything() {
        return;
    }

    state.traps[trap_idx].is_active = true;
    if state.base_map.is_none() {
        state.base_map = Some(Box::new(state.map.as_ref().unwrap().clone()));
    }
    state.active_trap_stack.push(trap_idx);

    let button_pos = state.traps[trap_idx].button_pos;
    let button_size = state.traps[trap_idx].button_size;
    let button_orient = state.traps[trap_idx].button_orient;
    let trap_time = state.traps[trap_idx].time;
    let trap_timeout = state.traps[trap_idx].timeout;

    let is_edit_mode = *server.game_mode.read().unwrap() == sauer_proto::data::GameMode::CoopEdit;
    let effective_timeout = if is_edit_mode || trap_timeout == 0 {
        trap_time
    } else {
        trap_timeout.max(trap_time)
    };

    drop(state);

    let server_clone = std::sync::Arc::clone(server);
    tokio::spawn(async move {
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

        let namespace = format!("trap_prog_{}", trap_idx);
        let actor_cn = crate::SERVER_BOT_CN as i32;

        let target_ids = {
            let is_play_mode =
                *server_clone.game_mode.read().unwrap() != sauer_proto::data::GameMode::CoopEdit;
            if is_play_mode {
                let maps = server_clone
                    .extra
                    .get::<std::sync::Arc<std::sync::RwLock<crate::play::PlayMaps>>>()
                    .unwrap();
                let lock = maps.read().unwrap();
                lock.evil_id.into_iter().collect::<Vec<_>>()
            } else {
                server_clone.client_ids()
            }
        };

        // Send initial 100% progress BEFORE applying trigger (beats client PasteServer freeze)
        let color = (0 << 8) | (15 << 4); // Full green
        let initial_fake_ent = sauer_map::virt_ents::FakeEnt {
            ent_type: sauer_proto::data::EntityType::Particles,
            position: [px, py, pz],
            attrs: [5, 100, color as i32, 1337, 7331],
        };
        {
            let mut state = get_server_state(&server_clone).write().unwrap();
            if let Some(map) = state.map.as_mut() {
                let mut glob = Vec::new();
                let mut pers = std::collections::HashMap::new();
                for &target_id in &target_ids {
                    let mut up =
                        map.update_virt_ents(target_id, &namespace, vec![initial_fake_ent.clone()]);
                    glob.append(&mut up.global_broadcast);
                    pers.insert(target_id, up.personal);
                }
                if !glob.is_empty() {
                    server_clone.broadcast_chan1(
                        glob.into_iter()
                            .map(|p| sauer_proto::Client {
                                client_num: actor_cn,
                                packet: p,
                            })
                            .collect::<Vec<_>>(),
                    );
                }
                for (tid, pkts) in pers {
                    if !pkts.is_empty() {
                        server_clone.send_chan1(
                            tid,
                            pkts.into_iter()
                                .map(|p| sauer_proto::Client {
                                    client_num: actor_cn,
                                    packet: p,
                                })
                                .collect::<Vec<_>>(),
                        );
                    }
                }
            }
        }

        // --- 1. APPLY ACTIVE ---
        recompose_traps(&server_clone);

        if let Some(trigger_cn) = server_clone.get_cn(actor_id) {
            server_clone.broadcast_chan1(&sauer_proto::Client {
                client_num: trigger_cn as i32,
                packet: sauer_proto::channel1::Sound { sound_id: 64 },
            });
        }

        set_button_color(
            &server_clone,
            actor_id,
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
                if let Some(t) = state.traps.get(trap_idx) {
                    t.is_active // Retains is_active == true during cooldown phase
                } else {
                    false
                }
            };
            if !is_valid {
                return; // Abort immediately, map changed!
            }

            let elapsed = start.elapsed();
            if elapsed >= total_dur {
                break;
            }

            if !reverted && elapsed >= revert_dur {
                {
                    let mut state = get_server_state(&server_clone).write().unwrap();
                    // We intentionally keep `is_active = true` to enforce the timeout cooldown
                    state.active_trap_stack.retain(|&id| id != trap_idx);
                }

                recompose_traps(&server_clone);

                {
                    let mut state = get_server_state(&server_clone).write().unwrap();
                    if state.active_trap_stack.is_empty() {
                        state.base_map = None;
                    }
                }

                if effective_timeout > trap_time {
                    // Force button color back to ACTIVE for the duration of the timeout cooldown.
                    set_button_color(
                        &server_clone,
                        actor_id,
                        button_pos,
                        button_size,
                        button_orient,
                        BUTTON_COLOR_ACTIVE,
                    );
                }

                reverted = true;
            }

            let remaining = total_dur.saturating_sub(elapsed).as_millis() as i32;
            let progress = (remaining * 100 / effective_timeout as i32).clamp(0, 100);

            if progress != last_progress {
                last_progress = progress;

                let g = (progress * 15 / 100).clamp(0, 15);
                let r = ((100 - progress) * 15 / 100).clamp(0, 15);
                let color = (r << 8) | (g << 4);
                let prog_fake_ent = sauer_map::virt_ents::FakeEnt {
                    ent_type: sauer_proto::data::EntityType::Particles,
                    position: [px, py, pz],
                    attrs: [5, progress as i32, color as i32, 1337, 7331],
                };

                let mut glob = Vec::new();
                let mut pers = std::collections::HashMap::new();
                {
                    let mut state = get_server_state(&server_clone).write().unwrap();
                    if let Some(map) = state.map.as_mut() {
                        for &target_id in &target_ids {
                            let mut up = map.update_virt_ents(
                                target_id,
                                &namespace,
                                vec![prog_fake_ent.clone()],
                            );
                            glob.append(&mut up.global_broadcast);
                            pers.insert(target_id, up.personal);
                        }
                    }
                }
                if !glob.is_empty() {
                    server_clone.broadcast_chan1(
                        glob.into_iter()
                            .map(|p| sauer_proto::Client {
                                client_num: actor_cn,
                                packet: p,
                            })
                            .collect::<Vec<_>>(),
                    );
                }
                for (tid, pkts) in pers {
                    if !pkts.is_empty() {
                        server_clone.send_chan1(
                            tid,
                            pkts.into_iter()
                                .map(|p| sauer_proto::Client {
                                    client_num: actor_cn,
                                    packet: p,
                                })
                                .collect::<Vec<_>>(),
                        );
                    }
                }
            }
            interval.tick().await;
        }

        if !reverted {
            {
                let mut state = get_server_state(&server_clone).write().unwrap();
                if state.traps[trap_idx].is_active {
                    state.traps[trap_idx].is_active = false;
                    state.active_trap_stack.retain(|&id| id != trap_idx);
                }
            }

            recompose_traps(&server_clone);

            {
                let mut state = get_server_state(&server_clone).write().unwrap();
                if state.active_trap_stack.is_empty() {
                    state.base_map = None;
                }
            }
        } else {
            // End of the timeout cooldown
            let mut state = get_server_state(&server_clone).write().unwrap();
            if state.traps.get(trap_idx).map_or(false, |t| t.is_active) {
                state.traps[trap_idx].is_active = false;
            }
        }

        {
            let mut glob = Vec::new();
            let mut pers = std::collections::HashMap::new();
            let mut state = get_server_state(&server_clone).write().unwrap();
            if let Some(map) = state.map.as_mut() {
                for &target_id in &target_ids {
                    let mut up = map.clear_virt_ents(target_id, &namespace);
                    glob.append(&mut up.global_broadcast);
                    pers.insert(target_id, up.personal);
                }
            }
            drop(state);
            if !glob.is_empty() {
                server_clone.broadcast_chan1(
                    glob.into_iter()
                        .map(|p| sauer_proto::Client {
                            client_num: actor_cn,
                            packet: p,
                        })
                        .collect::<Vec<_>>(),
                );
            }
            for (tid, pkts) in pers {
                if !pkts.is_empty() {
                    server_clone.send_chan1(
                        tid,
                        pkts.into_iter()
                            .map(|p| sauer_proto::Client {
                                client_num: actor_cn,
                                packet: p,
                            })
                            .collect::<Vec<_>>(),
                    );
                }
            }
        }

        set_button_color(
            &server_clone,
            actor_id,
            button_pos,
            button_size,
            button_orient,
            BUTTON_COLOR_READY,
        );
    });
}

pub fn set_button_color(
    server: &Server,
    _actor_id: u64,
    pos: [i32; 3],
    size: i32,
    orient: u8,
    color: [f32; 3],
) {
    let actor_cn = crate::SERVER_BOT_CN as i32;
    let mut state = get_server_state(server).write().unwrap();

    for packet in crate::trap::placement::get_outer_faces_vslot_packets(pos, size, orient, color) {
        if let Some(map) = state.map.as_mut() {
            map.apply_vslot_edit(&packet);
        }
        if let Some(base) = state.base_map.as_mut() {
            base.apply_vslot_edit(&packet);
        }
        server.broadcast_chan1(&Client {
            client_num: actor_cn,
            packet,
        });
    }
}

type Box3 = ([i32; 3], [i32; 3]);

/// Tight boxes around what differs between the trees: one per known area
/// with changes inside it, plus one around changes outside all of them.
fn changed_boxes(
    old: &sauer_map::Node,
    new: &sauer_map::Node,
    world_size: i32,
    areas: &[Box3],
) -> Vec<Box3> {
    let mut cells = Vec::new();
    sauer_map::octree::collect_diff_cells(old, new, [0; 3], world_size, &mut cells);
    let empty = ([i32::MAX; 3], [i32::MIN; 3]);
    let mut tight = vec![empty; areas.len() + 1];
    let grow = |b: &mut Box3, min: [i32; 3], max: [i32; 3]| {
        for k in 0..3 {
            b.0[k] = b.0[k].min(min[k]);
            b.1[k] = b.1[k].max(max[k]);
        }
    };
    for (pos, size, _) in &cells {
        let (cmin, cmax) = (*pos, pos.map(|v| v + size));
        let mut inside = false;
        for (i, (amin, amax)) in areas.iter().enumerate() {
            let lo: [i32; 3] = std::array::from_fn(|k| cmin[k].max(amin[k]));
            let hi: [i32; 3] = std::array::from_fn(|k| cmax[k].min(amax[k]));
            if (0..3).all(|k| lo[k] < hi[k]) {
                grow(&mut tight[i], lo, hi);
                inside |= lo == cmin && hi == cmax;
            }
        }
        if !inside {
            grow(&mut tight[areas.len()], cmin, cmax);
        }
    }
    let mut out: Vec<Box3> = Vec::new();
    for b in tight.into_iter().filter(|b| b.0[0] != i32::MAX) {
        // Overlapping areas (a button inside a trap frame) would paste twice.
        if !out
            .iter()
            .any(|o| (0..3).all(|k| o.0[k] <= b.0[k] && b.1[k] <= o.1[k]))
        {
            out.retain(|o| !(0..3).all(|k| b.0[k] <= o.0[k] && o.1[k] <= b.1[k]));
            out.push(b);
        }
    }
    out
}
