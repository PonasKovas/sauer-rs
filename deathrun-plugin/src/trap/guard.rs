//! Rules applied to coop edits: protected areas, the per-trap size budget
//! while a trap is being edited, and keeping trap entity links honest when
//! base-state entities change.

use super::ents::{self, EntChange};
use super::frame::outline;
use super::persist;
use crate::aim::Aim;
use crate::map_state::{DeathrunState, get_server_state};
use cube_string::IntoCubeStringLossy;
use sauer_kit::virt_ents::FakeEnt;
use sauer_map::Map;
use sauer_proto::{Client, channel1, data::EntityType};
use sauer_server::plugin::Context;

use sauer_core::Aabb;

const PROTECTED_MSG: &str = "That area is protected (a trap button or the trap vault).";

/// Tells the editor why their edit was undone and resends the server's
/// version of the area to everyone (the edit was already relayed).
pub fn reject(ctx: &Context, bounds: Aabb, msg: &str) {
    crate::ui::send_warn(ctx.server(), ctx.id(), msg);
    let server = ctx.server().clone();
    let payloads = {
        let state = get_server_state(&server).read();
        state
            .map
            .as_ref()
            .map(|m| sauer_kit::paste::restore_payloads(m, bounds))
            .unwrap_or_default()
    };
    crate::map_sync::paste_everywhere(&server, payloads);
}

/// Why an edit of `bounds` may not happen, if it may not.
fn blocked(state: &DeathrunState, bounds: Aabb) -> Option<&'static str> {
    if state.protected.blocks(&bounds) {
        Some(PROTECTED_MSG)
    } else if super::place::covers_preview(state, bounds) {
        Some("A trap button is being placed there.")
    } else {
        None
    }
}

/// For edits the server doesn't apply: undoes one that touches a
/// protected area or the placement preview.
pub fn refuse_if_blocked(ctx: &Context, bounds: Aabb) {
    let why = blocked(&get_server_state(ctx.server()).read(), bounds);
    if let Some(why) = why {
        reject(ctx, bounds, why);
    }
}

/// Applies a geometry edit to the server maps. Rejects it if it touches a
/// protected area, and reverts it if it pushes the trap being edited over
/// the budget.
pub fn geometry_edit(ctx: &Context, bounds: Aabb, mut apply: impl FnMut(&mut Map)) {
    let mut state = get_server_state(ctx.server()).write();
    if let Some(why) = blocked(&state, bounds) {
        drop(state);
        reject(ctx, bounds, why);
        return;
    }
    let snapshot = match (state.mode.editing(), &state.map) {
        (Some(_), Some(map)) => Some(sauer_map::block::generate_region_clipboard(map, bounds)),
        _ => None,
    };

    if let Some(base) = state.base_map.as_mut() {
        apply(base);
    }
    if let Some(map) = state.map.as_mut() {
        apply(map);
    }

    let Some((pos, s, grid, payload)) = snapshot else {
        return;
    };
    let size = super::commands::edit_size(&state);
    if size <= super::TRAP_BUDGET {
        return;
    }
    let map = state.map.as_mut().unwrap();
    if let Some(sel) = super::vault::sel(map, pos, s, grid) {
        map.apply_block(&sel, payload.nodes, false);
    }
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
/// `boxes` are where the edited entities end up.
pub fn entity_edit(ctx: &Context, ids: &[usize], boxes: &[Aabb], mut apply: impl FnMut(&mut Map)) {
    let server = ctx.server();
    let mut state = get_server_state(server).write();

    // Virtual entities (frames, progress bars) are server-drawn and personal.
    // Someone grabbed one: undo it for everyone and keep it out of the map.
    let virt: Vec<usize> = ids
        .iter()
        .copied()
        .filter(|&i| state.virt.is_virtual(i))
        .collect();
    if !virt.is_empty() {
        let cn = crate::SERVER_BOT_CN;
        for client in server.client_ids() {
            let fixes: Vec<_> = virt
                .iter()
                .filter_map(|&i| state.virt.view(client, i))
                .map(|packet| Client {
                    client_num: cn,
                    packet,
                })
                .collect();
            server.send(client, fixes);
        }
        if virt.len() == ids.len() {
            return;
        }
    }
    let saved: Vec<(
        usize,
        Option<ogz_rs::data::Entity>,
        Option<ogz_rs::data::Entity>,
    )> = virt
        .iter()
        .map(|&i| {
            (
                i,
                state.map.as_ref().and_then(|m| m.entities.get(i).cloned()),
                state
                    .base_map
                    .as_ref()
                    .and_then(|m| m.entities.get(i).cloned()),
            )
        })
        .collect();

    let watch = state.mode.editing().is_none();
    let before: Vec<Option<ogz_rs::data::Entity>> = ids
        .iter()
        .map(|&i| base_entities(&state).and_then(|e| e.get(i).cloned()))
        .collect();

    let ok = !boxes.iter().any(|b| state.protected.blocks(b));
    if ok {
        if let Some(base) = state.base_map.as_mut() {
            apply(base);
        }
        if let Some(map) = state.map.as_mut() {
            apply(map);
        }
    }
    for (i, live, base) in saved {
        if let (Some(e), Some(m)) = (live, state.map.as_mut()) {
            m.entities[i] = e;
        }
        if let (Some(e), Some(m)) = (base, state.base_map.as_mut()) {
            m.entities[i] = e;
        }
    }
    // An undo can bring back entities from before a virtual slot was taken.
    let DeathrunState {
        map,
        base_map,
        virt,
        ..
    } = &mut *state;
    for m in map.iter_mut().chain(base_map.as_deref_mut()) {
        virt.reconcile(&mut m.entities);
    }
    if !ok {
        // The edit was already relayed: put the server's entities back everywhere.
        crate::map_sync::broadcast_ents(server, state.map.as_ref().unwrap(), ids.iter().copied());
        drop(state);
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
        server.broadcast(&channel1::ServMsg {
            text: format!("\x0C6[TRAP]\x0C7 {text}").lossy_cubestring(),
        });
    }
    persist::save_and_send(server, Some(ctx.id()));
}

// ─── Indicators ──────────────────────────────────────────────────────────────

// Particle colors are 12-bit 0xRGB (engine `colorfromattr`); entity attrs
// are 16-bit, so anything wider gets cut off.
/// The trap's geometry box.
pub const GEOMETRY_COLOR: i32 = 0x0FF;
/// Disappears when the trap fires.
const GONE_COLOR: i32 = 0xF00;
/// Appears when the trap fires.
const NEW_COLOR: i32 = 0x0F0;
const MAX_MARKED: usize = 20;
/// Half the frame size: corners end up ~14 units from the entity, well clear of LOOK_RADIUS.
const HALF: i32 = 8;

/// Entity changes of a trap, live while it is being edited.
pub fn trap_ent_changes(state: &DeathrunState, trap: super::TrapId) -> Vec<EntChange> {
    match (state.mode.editing(), &state.map) {
        (Some(edit), Some(map)) if edit.trap == trap => ents::diff(
            &edit.original_map.entities,
            &map.entities,
            state.virt.slots(),
        ),
        _ => state.trap(trap).map(|t| t.ents.clone()).unwrap_or_default(),
    }
}

/// Coloured frames around the entities a trap deletes (red) and creates
/// (green); a move shows both. Big enough that looking at the entity itself
/// never lands on a frame corner.
pub fn entity_frames(changes: &[EntChange], eye: Option<&Aim>) -> Vec<FakeEnt> {
    let mut marks = Vec::new();
    for c in changes {
        if let Some(b) = c.base() {
            marks.push((b.pos, GONE_COLOR));
        }
        if let Some(a) = c.active() {
            marks.push((a.pos, NEW_COLOR));
        }
    }
    // Frame the ones in front of the player closest to the view ray (through
    // walls); nothing behind them.
    if let Some(eye) = eye {
        let mut keyed: Vec<_> = marks
            .into_iter()
            .filter_map(|m| eye.distance_sq(m.0).map(|d| (d, m)))
            .collect();
        keyed.sort_by(|a, b| a.0.total_cmp(&b.0));
        marks = keyed.into_iter().map(|(_, m)| m).collect();
    }
    marks
        .into_iter()
        .take(MAX_MARKED)
        .flat_map(|(pos, color)| {
            let c = pos.map(|v| v.round() as i32);
            outline(Aabb::cube(c.map(|v| v - HALF), 2 * HALF), eye, color)
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn looking_at_entity_misses_frame_corners() {
        let changes = [EntChange::Added(ents::EntRec {
            ty: 1,
            pos: [100.0, 100.0, 100.0],
            attrs: [0; 5],
        })];
        let eye = Aim {
            origin: [100.0, 0.0, 100.0],
            forward: [0.0, 1.0, 0.0],
        };
        let frames = entity_frames(&changes, Some(&eye));
        assert_eq!(frames.len(), 12);
        assert!(
            frames.iter().all(|f| f.attrs[1] < 3),
            "a corner counted as looked at"
        );
    }
}
