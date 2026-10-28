use super::{BUTTON_SIZE, Trap, TrapEditState, TrapGeom, persist};
use crate::map_state::{DeathrunState, get_server_state};
use cube_string::IntoCubeStringLossy;
use sauer_map::{Map, Selection};
use sauer_proto::{
    Client, Encode,
    channel1::{self, ServMsg},
    data::{ClipboardPayload, Vec3},
    route::{Chan1, S2C},
};
use sauer_server::Server;
use sauer_server::plugin::Context;
use std::sync::Arc;

type Box3 = ([i32; 3], [i32; 3]);

fn find_trap(state: &DeathrunState, hit_pt: [f32; 3]) -> Option<usize> {
    state
        .traps
        .iter()
        .position(|t| t.contains_button_point(hit_pt, 0.0))
}

fn not_looking(server: &Server, id: u64) {
    server.send_chan1(
        id,
        &ServMsg {
            text: "Not looking at a trap.".lossy_cubestring(),
        },
    );
}

fn say(server: &Server, id: u64, text: String) {
    server.send_chan1(
        id,
        &ServMsg {
            text: text.lossy_cubestring(),
        },
    );
}

/// Compressed size of a payload as it goes over the wire, i.e. what firing the trap costs.
pub fn payload_size(payload: &ClipboardPayload) -> usize {
    let packet = channel1::ClipboardServer {
        client_num: 0,
        payload: payload.clone(),
    };
    let mut buf = Vec::new();
    <channel1::ClipboardServer as Encode<S2C<Chan1>>>::encode(&packet, &mut buf);
    buf.len()
}

/// Bounds of the geometry that differs between `original` and `current`,
/// grown to cover the trap's previous clip box when re-editing it.
pub fn geometry_bounds(original: &Map, current: &Map, old: Option<&TrapGeom>) -> Option<Box3> {
    let mut min = [i32::MAX; 3];
    let mut max = [i32::MIN; 3];
    sauer_map::octree::diff_bounds(
        &original.root,
        &current.root,
        [0, 0, 0],
        current.size,
        &mut min,
        &mut max,
    );
    if let Some(g) = old {
        let (o_min, o_max) = g.clip_box();
        for i in 0..3 {
            min[i] = min[i].min(o_min[i]);
            max[i] = max[i].max(o_max[i]);
        }
    }
    if min[0] == i32::MAX {
        return None;
    }
    Some((min.map(|v| v.max(0)), max.map(|v| v.min(current.size))))
}

/// Copies the active-state geometry inside `bounds` out of `current`.
pub fn capture_geometry(current: &Map, (min, max): Box3) -> TrapGeom {
    let (clip_pos, clip_s, clip_grid, payload) =
        sauer_map::clipboard::generate_region_clipboard(current, min, max);
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
    let (Some(edit), Some(current)) = (state.trap_edit.as_ref(), state.map.as_ref()) else {
        return 0;
    };
    let old = edit
        .is_edit
        .then(|| state.traps.get(edit.trap_idx).and_then(|t| t.geom.as_ref()))
        .flatten();
    geometry_bounds(&edit.original_map, current, old)
        .map_or(0, |b| payload_size(&capture_geometry(current, b).payload))
}

fn broadcast_ents(server: &Server, map: &Map, ids: impl IntoIterator<Item = usize>) {
    let cn = crate::SERVER_BOT_CN as i32;
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
            server.broadcast_chan1(std::mem::take(&mut batch));
        }
    }
    if !batch.is_empty() {
        server.broadcast_chan1(batch);
    }
}

fn spawn_payloads(server: &Arc<Server>, payloads: Vec<ClipboardPayload>) {
    if payloads.is_empty() {
        return;
    }
    let s = server.clone();
    tokio::spawn(async move {
        crate::geo_sync::broadcast_payloads(&s, crate::SERVER_BOT_CN as i32, payloads, 0).await;
    });
}

pub fn cmd_deltrap(ctx: &Context, hit_pt: [f32; 3]) {
    let id = ctx.id();
    let server = ctx.server();
    let mut state = get_server_state(server).write().unwrap();

    let Some(trap_idx) = find_trap(&state, hit_pt) else {
        not_looking(server, id);
        return;
    };
    let trap = state.traps.remove(trap_idx);

    let mut sync = persist::Sync::default();
    if let Some(geom) = &trap.geom {
        persist::free_shelf(&mut state, geom, &mut sync);
    }

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
    let (b_min, b_max) = trap.button_box();
    map.unprotect_region(b_min, b_max);
    map.fill(&sel, false);
    map.set_material(&sel, Default::default());
    let payloads = map.generate_restore_packets(b_min, b_max);

    let result = persist::save(&mut state, &mut sync);
    let out = persist::packets(&state, sync);
    drop(state);

    spawn_payloads(server, payloads);
    persist::send(server, out);
    if let Err(e) = result {
        crate::ui::send_warn(server, id, &format!("\x0C3[ERROR]\x0C7 {e}"));
    }
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
    let b_max = adj.map(|v| v + BUTTON_SIZE);
    if map.is_protected(adj, b_max) {
        crate::ui::send_warn(&server, id, "A trap can't be placed here.");
        return;
    }

    crate::trap::placement::build_button_geometry(map, adj, BUTTON_SIZE, hit_orient);
    map.protect_region(adj, b_max);
    let payloads = map.generate_restore_packets(adj, b_max);

    state.traps.push(Trap::new(adj, BUTTON_SIZE, hit_orient));

    let mut sync = persist::Sync::default();
    let result = persist::save(&mut state, &mut sync);
    let out = persist::packets(&state, sync);
    drop(state);

    persist::send(&server, out);
    spawn_payloads(&server, payloads);
    if let Err(e) = result {
        crate::ui::send_warn(&server, id, &format!("\x0C3[ERROR]\x0C7 {e}"));
    }
}

fn set_trap_value(ctx: &Context, hit_pt: [f32; 3], f: impl FnOnce(&mut Trap) -> String) {
    let id = ctx.id();
    let server = ctx.server();
    let mut state = get_server_state(server).write().unwrap();
    let Some(trap_idx) = find_trap(&state, hit_pt) else {
        not_looking(server, id);
        return;
    };
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
    let state = get_server_state(ctx.server()).read().unwrap();
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
    let state = get_server_state(ctx.server()).read().unwrap();
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
    let state = get_server_state(server).read().unwrap();
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

pub fn cmd_edittrap(ctx: &Context, hit_pt: [f32; 3]) {
    let id = ctx.id();
    let server = ctx.server();
    let mut state = get_server_state(server).write().unwrap();

    if state.trap_edit.is_some() {
        say(
            server,
            id,
            "A trap is already being edited. Use #endtrap first.".into(),
        );
        return;
    }
    let Some(trap_idx) = find_trap(&state, hit_pt) else {
        not_looking(server, id);
        return;
    };

    let original_map = Box::new(state.map.clone().unwrap());
    let trap = state.traps[trap_idx].clone();
    let map = state.map.as_mut().unwrap();
    let pool = map.virt_state.global_pool.clone();
    let touched = super::trigger::apply_active_state(map, &trap, &pool);
    map.invalidate_cache();
    crate::map_state::update_server_items(map, server);

    broadcast_ents(server, map, touched);
    let payloads = match &trap.geom {
        Some(g) => {
            let (min, max) = g.clip_box();
            map.generate_restore_packets(min, max)
        }
        None => Vec::new(),
    };
    state.trap_edit = Some(TrapEditState {
        trap_idx,
        original_map,
        is_edit: true,
    });
    drop(state);

    spawn_payloads(server, payloads);
    crate::ui::broadcast_trap_edit_started(server, trap_idx);
}

pub fn cmd_settrap(ctx: &Context, hit_pt: [f32; 3]) {
    let id = ctx.id();
    let server = ctx.server();
    let mut state = get_server_state(server).write().unwrap();

    if state.trap_edit.is_some() {
        say(
            server,
            id,
            "A trap is already being edited. Use #endtrap first.".into(),
        );
        return;
    }
    let Some(trap_idx) = find_trap(&state, hit_pt) else {
        not_looking(server, id);
        return;
    };

    let original_map = Box::new(state.map.clone().unwrap());
    state.trap_edit = Some(TrapEditState {
        trap_idx,
        original_map,
        is_edit: false,
    });
    drop(state);

    crate::ui::broadcast_trap_edit_started(server, trap_idx);
}

pub fn cmd_endtrap(ctx: &Context) {
    let id = ctx.id();
    let server = ctx.server();
    let mut state = get_server_state(server).write().unwrap();

    let Some(edit_state) = state.trap_edit.take() else {
        say(server, id, "No trap is currently being edited.".into());
        return;
    };

    if let Some(map) = state.map.as_mut() {
        let up = map.clear_virt_ents(id, "edit_particle");
        let cn = crate::SERVER_BOT_CN as i32;
        let wrap = |pkts: Vec<channel1::EditEnt>| {
            pkts.into_iter()
                .map(|p| Client {
                    client_num: cn,
                    packet: p,
                })
                .collect::<Vec<_>>()
        };
        if !up.global_broadcast.is_empty() {
            server.broadcast_chan1(wrap(up.global_broadcast));
        }
        if !up.personal.is_empty() {
            server.send_chan1(id, wrap(up.personal));
        }
    }

    let current_map = state.map.as_ref().unwrap();
    let original = &edit_state.original_map;
    let pool = current_map.virt_state.global_pool.clone();
    let old_geom = state.traps[edit_state.trap_idx].geom.clone();

    let geom_bounds = geometry_bounds(
        original,
        current_map,
        old_geom.as_ref().filter(|_| edit_state.is_edit),
    );
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
    let virt_state = std::mem::take(&mut state.map.as_mut().unwrap().virt_state);
    state.map = Some(*edit_state.original_map.clone());
    if let Some(map) = state.map.as_mut() {
        if merged_vslots.len() > map.vslots.len() {
            map.vslots = merged_vslots;
        }
        map.virt_state = virt_state;
        map.reconcile_virtual_entities();
        crate::map_state::update_server_items(map, server);
    }

    let mut sync = persist::Sync::default();
    let mut result = Ok(());
    if !nothing && !too_large {
        if let Some(g) = &old_geom {
            persist::free_shelf(&mut state, g, &mut sync);
        }
        let trap = &mut state.traps[edit_state.trap_idx];
        trap.geom = new_geom;
        trap.ents = ent_changes;
        result = persist::save(&mut state, &mut sync);
    }
    let out = persist::packets(&state, sync);
    let restore = geom_bounds
        .map(|(min, max)| {
            state
                .map
                .as_ref()
                .unwrap()
                .generate_restore_packets(min, max)
        })
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
    server.broadcast_chan1(&ServMsg {
        text: text.lossy_cubestring(),
    });
}
