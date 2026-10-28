//! Rules applied to coop edits: protected areas, the per-trap size budget
//! while a trap is being edited, and keeping trap entity links honest when
//! base-state entities change.

use super::ents::{self, EntChange};
use super::persist;
use crate::map_state::{DeathrunState, get_server_state};
use cube_string::IntoCubeStringLossy;
use sauer_map::Map;
use sauer_map::virt_ents::FakeEnt;
use sauer_proto::{Client, channel1, data::EntityType};
use sauer_server::plugin::Context;

type Box3 = ([i32; 3], [i32; 3]);

const PROTECTED_MSG: &str = "That area is protected (a trap button or the trap vault).";

/// Tells the editor why their edit was undone and resends the server's
/// version of the area to everyone (the edit was already relayed).
pub fn reject(ctx: &Context, (min, max): Box3, msg: &str) {
    crate::ui::send_warn(ctx.server(), ctx.id(), msg);
    let server = ctx.server().clone();
    let payloads = {
        let state = get_server_state(&server).read().unwrap();
        state
            .map
            .as_ref()
            .map(|m| m.generate_restore_packets(min, max))
            .unwrap_or_default()
    };
    tokio::spawn(async move {
        crate::geo_sync::broadcast_payloads(&server, crate::SERVER_BOT_CN as i32, payloads, 0)
            .await;
    });
}

/// Applies a geometry edit to the server maps. Rejects it if it touches a
/// protected area, and reverts it if it pushes the trap being edited over
/// the budget.
pub fn geometry_edit(ctx: &Context, bounds: Box3, mut apply: impl FnMut(&mut Map) -> bool) {
    let mut state = get_server_state(ctx.server()).write().unwrap();
    let snapshot = match (&state.trap_edit, &state.map) {
        (Some(_), Some(map)) => Some(sauer_map::clipboard::generate_region_clipboard(
            map, bounds.0, bounds.1,
        )),
        _ => None,
    };

    let mut ok = true;
    if let Some(base) = state.base_map.as_mut() {
        ok = apply(base);
    }
    if let Some(map) = state.map.as_mut() {
        ok = apply(map);
    }
    if !ok {
        drop(state);
        reject(ctx, bounds, PROTECTED_MSG);
        return;
    }

    let Some((pos, s, grid, payload)) = snapshot else {
        return;
    };
    let size = super::commands::edit_size(&state);
    if size <= super::TRAP_BUDGET {
        return;
    }
    let map = state.map.as_mut().unwrap();
    map.apply_block(&super::vault::sel(pos, s, grid), payload.nodes, false);
    drop(state);
    let msg = format!(
        "\x0C3[ERROR]\x0C7 Edit reverted: the trap would be {} KB, over the {} KB limit.",
        size / 1024,
        super::TRAP_BUDGET / 1024
    );
    reject(ctx, bounds, &msg);
}

fn base_entities(state: &DeathrunState) -> Option<&Vec<ogz_rs::data::Entity>> {
    state
        .base_map
        .as_deref()
        .or(state.map.as_ref())
        .map(|m| &m.entities)
}

/// Applies an entity edit to the server maps. Rejects it in protected areas,
/// and unlinks traps from base-state entities that were changed or deleted.
pub fn entity_edit(ctx: &Context, ids: &[usize], mut apply: impl FnMut(&mut Map) -> bool) {
    let server = ctx.server();
    let mut state = get_server_state(server).write().unwrap();
    let watch = state.trap_edit.is_none();
    let before: Vec<Option<ogz_rs::data::Entity>> = ids
        .iter()
        .map(|&i| base_entities(&state).and_then(|e| e.get(i).cloned()))
        .collect();

    let mut ok = true;
    if let Some(base) = state.base_map.as_mut() {
        ok = apply(base);
    }
    if let Some(map) = state.map.as_mut() {
        ok = apply(map);
    }
    if !ok {
        // The edit was already relayed: put the server's entities back everywhere.
        let map = state.map.as_ref().unwrap();
        let cn = crate::SERVER_BOT_CN as i32;
        let restore: Vec<_> = ids
            .iter()
            .map(|&i| {
                let e = map
                    .entities
                    .get(i)
                    .cloned()
                    .unwrap_or(ogz_rs::data::Entity {
                        position: [0.0; 3],
                        attrs: [0; 5],
                        ent_type: EntityType::Empty,
                        extra_data: vec![],
                    });
                Client {
                    client_num: cn,
                    packet: channel1::EditEnt {
                        ent_id: i as i32,
                        pos: sauer_proto::data::Vec3 {
                            x: e.position[0],
                            y: e.position[1],
                            z: e.position[2],
                        },
                        type_: e.ent_type,
                        attrs: e.attrs.map(|x| x as i32),
                    },
                }
            })
            .collect();
        drop(state);
        server.broadcast_chan1(restore);
        crate::ui::send_warn(server, ctx.id(), PROTECTED_MSG);
        return;
    }
    if !watch {
        return;
    }

    let mut messages = Vec::new();
    for (&i, old) in ids.iter().zip(&before) {
        let Some(old) = old.as_ref().filter(|e| e.ent_type != EntityType::Empty) else {
            continue;
        };
        if base_entities(&state).and_then(|e| e.get(i)) == Some(old) {
            continue;
        }
        for trap in &mut state.traps {
            for rec in ents::unlink(&mut trap.ents, old) {
                messages.push(persist::unlinked_message(trap, &rec));
            }
        }
    }
    if messages.is_empty() {
        return;
    }
    drop(state);
    for text in messages {
        server.broadcast_chan1(&channel1::ServMsg {
            text: format!("\x0C6[TRAP]\x0C7 {text}").lossy_cubestring(),
        });
    }
    persist::save_and_send(server, Some(ctx.id()));
}

// ─── Indicators ──────────────────────────────────────────────────────────────

/// Disappears when the trap fires.
const GONE_COLOR: i32 = 0xFF3030;
/// Appears when the trap fires.
const NEW_COLOR: i32 = 0x30FF30;
const MAX_MARKED: usize = 20;
const HALF: i32 = 2;

/// Entity changes of a trap, live while it is being edited.
pub fn trap_ent_changes(state: &DeathrunState, idx: usize) -> Vec<EntChange> {
    match (&state.trap_edit, &state.map) {
        (Some(edit), Some(map)) if edit.trap_idx == idx => ents::diff(
            &edit.original_map.entities,
            &map.entities,
            &map.virt_state.global_pool,
        ),
        _ => state
            .traps
            .get(idx)
            .map(|t| t.ents.clone())
            .unwrap_or_default(),
    }
}

/// Small coloured frames around the entities a trap deletes (red) and
/// creates (green); a move shows both.
pub fn entity_frames(changes: &[EntChange]) -> Vec<FakeEnt> {
    let mut marks = Vec::new();
    for c in changes {
        if let Some(b) = c.base() {
            marks.push((b.pos, GONE_COLOR));
        }
        if let Some(a) = c.active() {
            marks.push((a.pos, NEW_COLOR));
        }
    }
    marks
        .into_iter()
        .take(MAX_MARKED)
        .flat_map(|(pos, color)| {
            let c = pos.map(|v| v.round() as i32);
            box_edges(c.map(|v| v - HALF), c.map(|v| v + HALF))
                .into_iter()
                .map(move |(x, y, z, axis, len)| FakeEnt {
                    ent_type: EntityType::Particles,
                    position: [x as f32, y as f32, z as f32],
                    attrs: [7, axis, len, color, 0],
                })
        })
        .collect()
}

/// The 12 edges of a box as `(x, y, z, axis, length)`, in the particle
/// encoding the trap frame uses (axis 1 = x, 2 = y, 0 = z).
pub fn box_edges(min: [i32; 3], max: [i32; 3]) -> Vec<(i32, i32, i32, i32, i32)> {
    let len = [max[0] - min[0], max[1] - min[1], max[2] - min[2]];
    let mut out = Vec::with_capacity(12);
    for a in [min[1], max[1]] {
        for b in [min[2], max[2]] {
            out.push((min[0], a, b, 1, len[0]));
        }
    }
    for a in [min[0], max[0]] {
        for b in [min[2], max[2]] {
            out.push((a, min[1], b, 2, len[1]));
        }
    }
    for a in [min[0], max[0]] {
        for b in [min[1], max[1]] {
            out.push((a, b, min[2], 0, len[2]));
        }
    }
    out
}

/// Bounds touched by an `N_EDITF` (extruding reaches past the selection).
pub fn edit_face_bounds(packet: &channel1::EditFace) -> Box3 {
    let sel = &packet.sel;
    let mut min = [sel.o.x, sel.o.y, sel.o.z];
    let mut max = [
        min[0] + sel.s.x * sel.grid,
        min[1] + sel.s.y * sel.grid,
        min[2] + sel.s.z * sel.grid,
    ];
    let d = (sel.orient as usize) >> 1;
    let dc = (sel.orient as usize) & 1;
    let (dir, grid) = (packet.dir, sel.grid);
    let extrude = packet.mode as i32 == 1
        && sel.cx <= 0
        && sel.cy <= 0
        && sel.cxs & 1 == 0
        && sel.cys & 1 == 0;
    if extrude {
        let (old_face, new_face) = if dc == 1 {
            (max[d], max[d] - dir * grid)
        } else {
            (min[d], min[d] + dir * grid)
        };
        min[d] = old_face.min(new_face);
        max[d] = old_face.max(new_face);
    } else if dc == 1 {
        min[d] = max[d] - grid;
    } else {
        max[d] = min[d] + grid;
    }
    (min, max)
}
