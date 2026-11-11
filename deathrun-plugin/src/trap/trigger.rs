use super::{BUTTON_COLOR_ACTIVE, BUTTON_COLOR_READY, Trap, TrapId, TrapPhase};
use crate::map_state::get_server_state;
use sauer_proto::{
    Client, channel1,
    data::{GameMode, Vec3},
};
use sauer_server::Server;
use std::{sync::Arc, time::Duration};

/// Turns the base state of the trap's area into its active state. Returns the
/// entity slots it touched.
pub fn apply_active_state(
    map: &mut sauer_map::Map,
    trap: &Trap,
    virt_pool: &[usize],
) -> Vec<usize> {
    let touched = super::ents::apply(&mut map.entities, &trap.ents, virt_pool);
    if let Some(geom) = &trap.geom {
        geom.paste_into(map);
    }
    touched
}

pub fn recompose_traps(server: &std::sync::Arc<Server>) {
    let mut state = get_server_state(server).write();

    let base_map = match state.base_map.as_ref() {
        Some(b) => b.clone(),
        None => return, // Nothing to recompose
    };

    let virt_state = std::mem::take(&mut state.map.as_mut().unwrap().virt_state);
    let old_map = state.map.clone().unwrap();
    let mut new_map = *base_map;

    // Apply all active traps in chronological order
    for trap in state
        .active_trap_stack
        .iter()
        .filter_map(|&id| state.trap(id))
    {
        apply_active_state(&mut new_map, trap, &virt_state.global_pool);
    }

    // A trap's geometry can cover other traps' buttons and paste them back as
    // they were when it was recorded, so redo every button in its current color.
    for trap in &state.traps {
        let (pos, size, orient) = (trap.button_pos, trap.button_size, trap.button_orient);
        crate::trap::placement::build_button_geometry(&mut new_map, pos, size, orient);
        if trap.phase != TrapPhase::Ready {
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
    let actor_cn = crate::SERVER_BOT_CN;

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
            server.broadcast(batch.clone());
            batch.clear();
        }
    }
    if !batch.is_empty() {
        server.broadcast(batch);
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

pub fn trap_triggerable(state: &crate::map_state::DeathrunState, id: TrapId) -> bool {
    // A button being moved is off limits until it is placed.
    state.mode.editing().is_none() && state.mode.placing().is_none_or(|p| p.trap != Some(id))
}

pub fn trigger_trap(server: &Arc<Server>, id: TrapId, actor_id: u64) {
    let mut state = get_server_state(server).write();
    if !trap_triggerable(&state, id) {
        return;
    }
    let Some(trap) = state.trap_mut(id) else {
        return;
    };
    if trap.phase != TrapPhase::Ready || !trap.does_anything() {
        return;
    }
    trap.phase = TrapPhase::Firing;
    let button = (trap.button_pos, trap.button_size, trap.button_orient);
    let (time, timeout) = (trap.time, trap.timeout);
    if state.base_map.is_none() {
        state.base_map = state.map.clone().map(Box::new);
    }
    state.active_trap_stack.push(id);
    drop(state);

    let is_edit_mode = server.game.read().mode == GameMode::CoopEdit;
    let total = if is_edit_mode || timeout == 0 {
        time
    } else {
        timeout.max(time)
    };
    tokio::spawn(run_trap(
        Arc::clone(server),
        id,
        actor_id,
        button,
        time,
        total,
    ));
}

/// A fired trap: its active state for `time` ms, then a cooldown until
/// `total` ms, with a progress bar by its button for whoever may trigger it.
async fn run_trap(
    server: Arc<Server>,
    id: TrapId,
    actor_id: u64,
    (pos, size, orient): (ButtonPos, i32, u8),
    time: u32,
    total: u32,
) {
    let namespace = format!("trap_prog_{id}");
    let bar_pos = progress_bar_pos(pos, size, orient);
    // In play only the terrorist triggers traps, so only they see the bar.
    let targets = if server.game.read().mode == GameMode::CoopEdit {
        server.client_ids()
    } else {
        let maps = server
            .extra
            .get::<Arc<parking_lot::RwLock<crate::play::PlayMaps>>>()
            .expect("DeathrunPlay::init sets up PlayMaps");
        maps.read().evil_id.into_iter().collect()
    };

    // The full bar goes out before the trap's paste, which freezes clients a moment.
    show_progress(
        &server,
        &targets,
        &namespace,
        vec![progress_ent(bar_pos, 100)],
    );
    recompose_traps(&server);
    if let Some(trigger_cn) = server.get_cn(actor_id) {
        server.broadcast(&Client {
            client_num: trigger_cn,
            packet: channel1::Sound { sound_id: 64 },
        });
    }
    set_button_color(&server, pos, size, orient, BUTTON_COLOR_ACTIVE);

    let start = tokio::time::Instant::now();
    let total_dur = Duration::from_millis(total as u64);
    let revert_dur = Duration::from_millis(time as u64);
    let mut last_progress = 100;
    let mut interval = tokio::time::interval(Duration::from_millis(33));
    loop {
        // Gone when it was deleted or the map was reloaded.
        let phase = get_server_state(&server).read().trap(id).map(|t| t.phase);
        let Some(phase @ (TrapPhase::Firing | TrapPhase::Cooldown)) = phase else {
            show_progress(&server, &targets, &namespace, Vec::new());
            return;
        };

        let elapsed = start.elapsed();
        if elapsed >= total_dur {
            break;
        }
        if phase == TrapPhase::Firing && elapsed >= revert_dur {
            revert(&server, id);
            // The button stays dark through the cooldown.
            set_button_color(&server, pos, size, orient, BUTTON_COLOR_ACTIVE);
        }

        let remaining = total_dur.saturating_sub(elapsed).as_millis() as i32;
        let progress = (remaining * 100 / total as i32).clamp(0, 100);
        if progress != last_progress {
            last_progress = progress;
            show_progress(
                &server,
                &targets,
                &namespace,
                vec![progress_ent(bar_pos, progress)],
            );
        }
        interval.tick().await;
    }

    // With no cooldown the trap is still firing here.
    revert(&server, id);
    if let Some(trap) = get_server_state(&server).write().trap_mut(id) {
        trap.phase = TrapPhase::Ready;
    }
    show_progress(&server, &targets, &namespace, Vec::new());
    set_button_color(&server, pos, size, orient, BUTTON_COLOR_READY);
}

/// Takes a firing trap's active state out of the map; it cools down next.
fn revert(server: &Arc<Server>, id: TrapId) {
    {
        let mut state = get_server_state(server).write();
        match state.trap_mut(id) {
            Some(trap) if trap.phase == TrapPhase::Firing => trap.phase = TrapPhase::Cooldown,
            _ => return,
        }
        state.active_trap_stack.retain(|&i| i != id);
    }
    recompose_traps(server);
    let mut state = get_server_state(server).write();
    if state.active_trap_stack.is_empty() {
        state.base_map = None;
    }
}

type ButtonPos = [i32; 3];

/// Half a unit off the button's face, in the middle.
fn progress_bar_pos(pos: ButtonPos, size: i32, orient: u8) -> [f32; 3] {
    let d = (orient >> 1) as usize;
    let mut p = pos.map(|v| v as f32 + size as f32 / 2.0);
    p[d] = pos[d] as f32
        + if orient & 1 == 0 {
            -0.5
        } else {
            size as f32 + 0.5
        };
    p
}

/// The progress bar: a particle meter, green when full, red when empty.
fn progress_ent(pos: [f32; 3], progress: i32) -> sauer_map::virt_ents::FakeEnt {
    let g = (progress * 15 / 100).clamp(0, 15);
    let r = ((100 - progress) * 15 / 100).clamp(0, 15);
    sauer_map::virt_ents::FakeEnt {
        ent_type: sauer_proto::data::EntityType::Particles,
        position: pos,
        attrs: [5, progress, (r << 8) | (g << 4), 1337, 7331],
    }
}

/// Shows `ents` in `namespace` to each target (none clears it).
fn show_progress(
    server: &Server,
    targets: &[u64],
    namespace: &str,
    ents: Vec<sauer_map::virt_ents::FakeEnt>,
) {
    let updates: Vec<_> = {
        let mut state = get_server_state(server).write();
        let Some(map) = state.map.as_mut() else {
            return;
        };
        targets
            .iter()
            .map(|&id| (id, map.update_virt_ents(id, namespace, ents.clone())))
            .collect()
    };
    for (id, up) in updates {
        super::place::send_virt(server, id, up);
    }
}

pub fn set_button_color(server: &Server, pos: [i32; 3], size: i32, orient: u8, color: [f32; 3]) {
    let actor_cn = crate::SERVER_BOT_CN;
    let mut state = get_server_state(server).write();

    for packet in crate::trap::placement::get_outer_faces_vslot_packets(pos, size, orient, color) {
        if let Some(map) = state.map.as_mut() {
            map.apply_vslot_edit(&packet);
        }
        if let Some(base) = state.base_map.as_mut() {
            base.apply_vslot_edit(&packet);
        }
        server.broadcast(&Client {
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
