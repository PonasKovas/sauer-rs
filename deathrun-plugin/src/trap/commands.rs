use super::{Trap, TrapEditState, TrapGeom, TrapId, TrapMode, TrapPhase, persist};
use crate::map_state::{DeathrunState, get_server_state};
use cube_string::IntoCubeStringLossy;
use sauer_map::Map;
use sauer_proto::{
    Client, Encode,
    channel1::{self, ServMsg},
    data::{ClipboardPayload, Vec3},
    route::{Chan1, S2C},
};
use sauer_server::Server;
use sauer_server::plugin::Context;
use std::sync::Arc;

use sauer_core::Aabb;

fn find_trap(state: &DeathrunState, hit_pt: [f32; 3]) -> Option<usize> {
    state
        .traps
        .iter()
        .position(|t| t.contains_button_point(hit_pt, 0.0))
}

fn not_looking(server: &Server, id: u64) {
    server.send(
        id,
        &ServMsg {
            text: "Not looking at a trap.".lossy_cubestring(),
        },
    );
}

fn say(server: &Server, id: u64, text: String) {
    server.send(
        id,
        &ServMsg {
            text: text.lossy_cubestring(),
        },
    );
}

/// Compressed size of a payload as it goes over the wire, i.e. what firing the trap costs.
pub fn payload_size(payload: &ClipboardPayload) -> usize {
    let packet = channel1::ClipboardServer {
        client_num: sauer_proto::data::Cn(0),
        payload: Some(payload.clone()),
    };
    let mut buf = Vec::new();
    <channel1::ClipboardServer as Encode<S2C<Chan1>>>::encode(&packet, &mut buf);
    buf.len()
}

/// Bounds of the geometry that differs between `original` and `current`,
/// grown to cover the trap's previous clip box when re-editing it.
pub fn geometry_bounds(original: &Map, current: &Map, old: Option<&TrapGeom>) -> Option<Aabb> {
    let diff = sauer_map::octree::diff_box(&original.root, &current.root, current.size);
    let b = Aabb::enclosing(diff.into_iter().chain(old.map(TrapGeom::clip_box)))?;
    Aabb::new([0; 3], [current.size; 3]).intersection(&b)
}

/// Copies the active-state geometry inside `bounds` out of `current`.
pub fn capture_geometry(current: &Map, bounds: Aabb) -> TrapGeom {
    let (clip_pos, clip_s, clip_grid, payload) =
        sauer_map::block::generate_region_clipboard(current, bounds);
    TrapGeom {
        clip_pos,
        clip_s,
        clip_grid,
        payload,
        shelf: None,
    }
}

/// Size of the trap being edited if it were recorded right now.
pub fn edit_size(state: &DeathrunState) -> usize {
    let (Some(edit), Some(current)) = (state.mode.editing(), state.map.as_ref()) else {
        return 0;
    };
    let old = state.trap(edit.trap).and_then(|t| t.geom.as_ref());
    geometry_bounds(&edit.original_map, current, old)
        .map_or(0, |b| payload_size(&capture_geometry(current, b).payload))
}

fn broadcast_ents(server: &Server, map: &Map, ids: impl IntoIterator<Item = usize>) {
    let cn = crate::SERVER_BOT_CN;
    let empty = ogz_rs::data::Entity {
        position: [0.0; 3],
        attrs: [0; 5],
        ent_type: ogz_rs::data::EntityType::Empty,
        extra_data: vec![],
    };
    let mut batch = Vec::new();
    for id in ids {
        let ent = map.entities.get(id).unwrap_or(&empty);
        batch.push(Client {
            client_num: cn,
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
            server.broadcast(std::mem::take(&mut batch));
        }
    }
    if !batch.is_empty() {
        server.broadcast(batch);
    }
}

fn spawn_payloads(server: &Arc<Server>, payloads: Vec<ClipboardPayload>) {
    if payloads.is_empty() {
        return;
    }
    let s = server.clone();
    tokio::spawn(async move {
        crate::geo_sync::broadcast_payloads(&s, crate::SERVER_BOT_CN, payloads, 0).await;
    });
}

/// Why `trap` can't be changed right now, if it can't.
fn locked(server: &Server, state: &DeathrunState, trap: TrapId) -> Option<String> {
    let p = state.mode.placing()?;
    (p.trap == Some(trap)).then(|| super::place::busy_message(server, state).unwrap_or_default())
}

/// Why traps can't be added or removed right now, if they can't.
fn busy(server: &Server, state: &DeathrunState) -> Option<String> {
    if let Some(e) = state.mode.editing() {
        return Some(format!(
            "Trap {} is being edited. Type #tedit to finish editing first.",
            state.trap_number(e.trap)
        ));
    }
    if let Some(msg) = super::place::busy_message(server, state) {
        return Some(msg);
    }
    (!state.active_trap_stack.is_empty()).then(|| "Wait for the active traps to reset.".to_string())
}

pub fn cmd_deltrap(ctx: &Context, hit_pt: [f32; 3]) {
    let id = ctx.id();
    let server = ctx.server();
    let mut state = get_server_state(server).write();

    if let Some(msg) = busy(server, &state) {
        crate::ui::send_warn(server, id, &msg);
        return;
    }
    let Some(trap_idx) = find_trap(&state, hit_pt) else {
        not_looking(server, id);
        return;
    };
    let trap = state.traps.remove(trap_idx);

    let mut sync = persist::Sync::default();
    let key = super::placement::button_key(&trap);
    if let Err(e) = super::place::set_button_cells(&mut state, Some(key), None, &mut sync) {
        state.traps.insert(trap_idx, trap);
        drop(state);
        crate::ui::send_warn(server, id, &format!("\x0C3[ERROR]\x0C7 {e}"));
        return;
    }
    if let Some(geom) = &trap.geom {
        persist::free_shelf(&mut state, geom, &mut sync);
    }

    let result = persist::save(&mut state, &mut sync);
    let out = persist::packets(&state, sync);
    drop(state);

    persist::send(server, out);
    if let Err(e) = result {
        crate::ui::send_warn(server, id, &format!("\x0C3[ERROR]\x0C7 {e}"));
    }
}

/// Removes what a trap does, keeping its button and settings.
pub fn cmd_cleartrap(ctx: &Context, hit_pt: [f32; 3]) {
    let id = ctx.id();
    let server = ctx.server();
    let mut state = get_server_state(server).write();

    if let Some(msg) = busy(server, &state) {
        crate::ui::send_warn(server, id, &msg);
        return;
    }
    let Some(trap_idx) = find_trap(&state, hit_pt) else {
        not_looking(server, id);
        return;
    };
    if state.traps[trap_idx].phase != TrapPhase::Ready {
        crate::ui::send_warn(server, id, "That trap is active. Wait for it to reset.");
        return;
    }
    if !state.traps[trap_idx].does_anything() {
        drop(state);
        say(server, id, format!("Trap {trap_idx} does nothing yet."));
        return;
    }

    let mut sync = persist::Sync::default();
    if let Some(geom) = state.traps[trap_idx].geom.take() {
        persist::free_shelf(&mut state, &geom, &mut sync);
    }
    state.traps[trap_idx].ents.clear();
    let result = persist::save(&mut state, &mut sync);
    let out = persist::packets(&state, sync);
    drop(state);

    persist::send(server, out);
    let name = crate::ui::player_name(server, id);
    server.broadcast(&ServMsg {
        text: format!("\x0C6[TRAP]\x0C7 {name} cleared trap {trap_idx}. Its button stays.")
            .lossy_cubestring(),
    });
    if let Err(e) = result {
        crate::ui::send_warn(server, id, &format!("\x0C3[ERROR]\x0C7 {e}"));
    }
}

fn set_trap_value(ctx: &Context, hit_pt: [f32; 3], f: impl FnOnce(&mut Trap) -> String) {
    let id = ctx.id();
    let server = ctx.server();
    let mut state = get_server_state(server).write();
    let Some(trap_idx) = find_trap(&state, hit_pt) else {
        not_looking(server, id);
        return;
    };
    if let Some(msg) = locked(server, &state, state.traps[trap_idx].id) {
        crate::ui::send_warn(server, id, &msg);
        return;
    }
    let msg = f(&mut state.traps[trap_idx]);
    drop(state);
    say(server, id, msg);
    persist::save_and_send(server, Some(id));
}

pub fn cmd_traptime(ctx: &Context, hit_pt: [f32; 3], time: u32) {
    set_trap_value(ctx, hit_pt, |trap| {
        trap.time = time;
        let mut msg = format!("Trap time set to {}ms.", time);
        if trap.timeout > 0 && time > trap.timeout {
            msg.push_str(&format!(
                " Warning: this is greater than the timeout ({}ms).",
                trap.timeout
            ));
        }
        msg
    });
}

pub fn cmd_traptimeout(ctx: &Context, hit_pt: [f32; 3], timeout: u32) {
    set_trap_value(ctx, hit_pt, |trap| {
        trap.timeout = timeout;
        let mut msg = format!("Trap timeout set to {}ms.", timeout);
        if timeout > 0 && timeout < trap.time {
            msg.push_str(&format!(
                " Warning: this is less than the trap time ({}ms).",
                trap.time
            ));
        }
        msg
    });
}

pub fn cmd_traptime_print(ctx: &Context, hit_pt: [f32; 3]) {
    let state = get_server_state(ctx.server()).read();
    match find_trap(&state, hit_pt) {
        Some(i) => say(
            ctx.server(),
            ctx.id(),
            format!("Trap time is {}ms.", state.traps[i].time),
        ),
        None => not_looking(ctx.server(), ctx.id()),
    }
}

pub fn cmd_traptimeout_print(ctx: &Context, hit_pt: [f32; 3]) {
    let state = get_server_state(ctx.server()).read();
    match find_trap(&state, hit_pt) {
        Some(i) => say(
            ctx.server(),
            ctx.id(),
            format!("Trap timeout is {}ms.", state.traps[i].timeout),
        ),
        None => not_looking(ctx.server(), ctx.id()),
    }
}

pub fn cmd_trapinfo(ctx: &Context, hit_pt: [f32; 3]) {
    let id = ctx.id();
    let server = ctx.server();
    let state = get_server_state(server).read();
    let Some(trap_idx) = find_trap(&state, hit_pt) else {
        not_looking(server, id);
        return;
    };
    let trap = &state.traps[trap_idx];

    let mut msg = format!("\x0C1[Trap {} Info]\n", trap_idx);
    match &trap.geom {
        Some(g) => {
            let e = g.extent();
            msg.push_str(&format!(
                "\x0C7Geometry: \x0C2{}x{}x{}\x0C7 at grid \x0C2{}\x0C7, \x0C2{:.1} KB\x0C7 of \x0C2{} KB\x0C7 budget\n",
                e[0],
                e[1],
                e[2],
                g.clip_grid,
                payload_size(&g.payload) as f64 / 1024.0,
                super::TRAP_BUDGET / 1024
            ));
        }
        None => msg.push_str("\x0C7Geometry: \x0C2none\n"),
    }
    let count = |f: fn(&super::EntChange) -> bool| trap.ents.iter().filter(|c| f(c)).count();
    msg.push_str(&format!(
        "\x0C7Entities: \x0C2{}\x0C7 created, \x0C2{}\x0C7 deleted, \x0C2{}\x0C7 changed\n",
        count(|c| matches!(c, super::EntChange::Added(_))),
        count(|c| matches!(c, super::EntChange::Removed(_))),
        count(|c| matches!(c, super::EntChange::Changed { .. })),
    ));
    msg.push_str(&format!(
        "\x0C7Time: \x0C2{}ms\x0C7, timeout: \x0C2{}ms",
        trap.time, trap.timeout
    ));
    say(server, id, msg);
}

/// `#tedit`: starts editing the aimed trap, or finishes the edit going on.
pub fn cmd_edittrap(ctx: &Context, hit_pt: Option<[f32; 3]>) {
    let id = ctx.id();
    let server = ctx.server();
    let mut state = get_server_state(server).write();
    let aimed = hit_pt.and_then(|p| find_trap(&state, p));

    if let Some(edit) = state.mode.editing() {
        // Someone aiming at another trap most likely meant to edit that one.
        if let Some(other) = aimed.filter(|&i| state.traps[i].id != edit.trap) {
            let msg = format!(
                "Trap {} is being edited, not trap {other}. Aim at it, or away from other traps, and type #tedit to finish.",
                state.trap_number(edit.trap)
            );
            drop(state);
            crate::ui::send_warn(server, id, &msg);
            return;
        }
        end_edit(server, id, state);
        return;
    }
    if let Some(msg) = busy(server, &state) {
        crate::ui::send_warn(server, id, &msg);
        return;
    }
    let Some(trap_idx) = aimed else {
        not_looking(server, id);
        return;
    };

    let original_map = Box::new(state.map.clone().unwrap());
    let trap = state.traps[trap_idx].clone();
    let pool = state.virt.slots().to_vec();
    let map = state.map.as_mut().unwrap();
    let touched = super::trigger::apply_active_state(map, &trap, &pool);
    map.invalidate_cache();
    crate::map_state::update_server_items(map, server);

    broadcast_ents(server, map, touched);
    let payloads = match &trap.geom {
        Some(g) => sauer_kit::paste::restore_payloads(map, g.clip_box()),
        None => Vec::new(),
    };
    state.mode = TrapMode::Editing(TrapEditState {
        trap: trap.id,
        original_map,
    });
    drop(state);

    spawn_payloads(server, payloads);
    crate::ui::broadcast_trap_edit_started(server, trap_idx);
}

/// Records the trap being edited and puts the map back to its base state.
fn end_edit(
    server: &Arc<Server>,
    id: u64,
    mut state: parking_lot::RwLockWriteGuard<'_, DeathrunState>,
) {
    let TrapMode::Editing(edit_state) = std::mem::take(&mut state.mode) else {
        return;
    };

    {
        let up = state.virt.clear(id, "edit_particle");
        up.send(server, id, crate::SERVER_BOT_CN);
    }

    let current_map = state.map.as_ref().unwrap();
    let original = &edit_state.original_map;
    let pool = state.virt.slots().to_vec();
    let old_geom = state.trap(edit_state.trap).and_then(|t| t.geom.clone());

    let geom_bounds = geometry_bounds(original, current_map, old_geom.as_ref());
    let ent_changes = super::ents::diff(&original.entities, &current_map.entities, &pool);
    let changed_ent_ids: Vec<usize> = (0..original.entities.len().max(current_map.entities.len()))
        .filter(|i| !pool.contains(i) && original.entities.get(*i) != current_map.entities.get(*i))
        .collect();

    let nothing = geom_bounds.is_none() && ent_changes.is_empty();
    let new_geom = geom_bounds.map(|b| capture_geometry(current_map, b));
    let size = new_geom.as_ref().map_or(0, |g| payload_size(&g.payload));
    let too_large = size > super::TRAP_BUDGET;

    // Back to the base state. Keep vslots created during the edit: the new
    // geometry may reference them.
    let merged_vslots = std::mem::take(&mut state.map.as_mut().unwrap().vslots);
    state.map = Some(*edit_state.original_map.clone());
    let crate::map_state::DeathrunState { map, virt, .. } = &mut *state;
    if let Some(map) = map.as_mut() {
        if merged_vslots.len() > map.vslots.len() {
            map.vslots = merged_vslots;
        }
        virt.reconcile(&mut map.entities);
        crate::map_state::update_server_items(map, server);
    }

    let mut sync = persist::Sync::default();
    let mut result = Ok(());
    if !nothing && !too_large {
        if let Some(g) = &old_geom {
            persist::free_shelf(&mut state, g, &mut sync);
        }
        if let Some(trap) = state.trap_mut(edit_state.trap) {
            trap.geom = new_geom;
            trap.ents = ent_changes;
        }
        result = persist::save(&mut state, &mut sync);
    }
    let out = persist::packets(&state, sync);
    let restore = geom_bounds
        .map(|b| sauer_kit::paste::restore_payloads(state.map.as_ref().unwrap(), b))
        .unwrap_or_default();
    let map_snapshot = state.map.as_ref().unwrap();
    broadcast_ents(server, map_snapshot, changed_ent_ids);
    drop(state);

    spawn_payloads(server, restore);
    persist::send(server, out);

    let text = if nothing {
        "Trap edit ended with no changes detected.".to_string()
    } else if too_large {
        format!(
            "\x0C3[ERROR]\x0C7 Trap edit discarded: its geometry is {} KB, over the {} KB limit.",
            size / 1024,
            super::TRAP_BUDGET / 1024
        )
    } else if let Err(e) = result {
        format!("\x0C3[ERROR]\x0C7 Trap recorded but could not be saved to the map: {e}")
    } else {
        "Trap recorded and area reverted successfully.".to_string()
    };
    server.broadcast(&ServMsg {
        text: text.lossy_cubestring(),
    });
}
