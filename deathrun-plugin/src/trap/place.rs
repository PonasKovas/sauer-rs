//! Choosing a spot for a trap button: `#tnew` and `#tmove`.
//!
//! One player aims around while everyone sees a see-through copy of the
//! button where it would go: green if it can go there, yellow if it would sit
//! inside a trap's frame (the frame is shown too), red if it can't. Typing the
//! command again (or shooting) puts it there. A move is cancelled by aiming back at the
//! button, a new trap by typing the command where the button can't go.
//!
//! The copy is never part of the server's map. It is only pasted on clients,
//! and the server's version is pasted back over it when it moves on.

use super::placement::{
    ButtonKey, build_button, build_button_geometry, button_key, clear_button_cell,
    get_outer_face_selections,
};
use super::{BUTTON_SIZE, Trap, TrapGeom, TrapMode, persist};
use crate::map_state::{DeathrunState, get_server_state};
use cube_string::IntoCubeStringLossy;
use sauer_map::Map;
use sauer_map::virt_ents::{FakeEnt, VirtEntUpdates};
use sauer_proto::data::{ClipboardPayload, EntityType, Vec3};
use sauer_proto::{Client, channel1};
use sauer_server::Server;
use std::sync::Arc;

type Box3 = ([i32; 3], [i32; 3]);

const FRAMES: &str = "place_frames";
/// The frames of traps the preview sits in, in the preview's yellow.
const FRAME_COLOR: i32 = 0xFF0;
const MAX_REACH: f32 = 2048.0;

#[derive(Clone, Copy, PartialEq, Eq)]
pub enum Kind {
    New,
    Move,
}

impl Kind {
    fn command(self) -> &'static str {
        match self {
            Kind::New => "#tnew",
            Kind::Move => "#tmove",
        }
    }
}

pub struct Placing {
    pub player: u64,
    /// The trap whose button is moving, `None` for a new trap.
    pub trap: Option<usize>,
    /// Where the player aims, as shown to everyone.
    target: Target,
}

impl Placing {
    fn kind(&self) -> Kind {
        if self.trap.is_some() {
            Kind::Move
        } else {
            Kind::New
        }
    }

    /// The cell the preview is drawn in, if any.
    pub fn preview_box(&self) -> Option<Box3> {
        match self.target {
            Target::Cell(key, _) => Some(cell_box(key)),
            _ => None,
        }
    }
}

#[derive(Clone, Copy, PartialEq, Debug)]
enum Target {
    /// No surface in reach, or the spot is outside the world.
    Nowhere,
    /// The moving button itself: placing here cancels the move.
    Back,
    Cell(ButtonKey, Verdict),
}

#[derive(Clone, Copy, PartialEq, Debug)]
enum Verdict {
    Good,
    /// Allowed, but a trap's geometry covers the button while that trap is active.
    InFrame,
    Bad(&'static str),
}

impl Verdict {
    fn color(self) -> [f32; 3] {
        match self {
            Verdict::Good => super::BUTTON_COLOR_GOOD,
            Verdict::InFrame => super::BUTTON_COLOR_WARN,
            Verdict::Bad(_) => super::BUTTON_COLOR_BAD,
        }
    }
}

/// Whether an edit of `bounds` would touch the placement preview. Those get
/// rejected: the edit lands on the preview on clients, and would be lost
/// under the next repaint anyway.
pub fn covers_preview(state: &DeathrunState, bounds: Box3) -> bool {
    state
        .mode
        .placing()
        .and_then(Placing::preview_box)
        .is_some_and(|cell| overlaps(cell, bounds))
}

fn cell_box((pos, size, _): ButtonKey) -> Box3 {
    (pos, pos.map(|v| v + size))
}

fn overlaps(a: Box3, b: Box3) -> bool {
    (0..3).all(|i| a.0[i] < b.1[i] && b.0[i] < a.1[i])
}

/// The cell in front of the surface hit at `hit_pt`, for a button facing
/// `orient`. `None` if it would stick out of the world.
fn cell_at(hit_pt: [f32; 3], orient: u8, world_size: i32) -> Option<[i32; 3]> {
    let d = (orient >> 1) as usize;
    let dc = orient & 1;
    let r_axis = [1usize, 2, 0][d];
    let c_axis = [2usize, 0, 1][d];
    let size = BUTTON_SIZE as f32;
    let snap = |v: f32| (v / size).floor() as i32 * BUTTON_SIZE;

    let mut adj = [0i32; 3];
    adj[d] = if dc == 0 {
        snap(hit_pt[d]) - BUTTON_SIZE
    } else {
        ((hit_pt[d] - 0.001) / size).ceil() as i32 * BUTTON_SIZE
    };
    adj[r_axis] = snap(hit_pt[r_axis]);
    adj[c_axis] = snap(hit_pt[c_axis]);
    adj.iter()
        .all(|&v| v >= 0 && v + BUTTON_SIZE <= world_size)
        .then_some(adj)
}

/// The trap whose button the ray hits, counting only the button itself, not
/// the air around it in its cell.
pub fn button_hit(state: &DeathrunState, origin: [f32; 3], ray: [f32; 3]) -> Option<usize> {
    let (hit_pt, _, _) = raycast(state, origin, ray)?;
    // Just past the surface, inside whatever was hit.
    let inside: [f32; 3] = std::array::from_fn(|i| hit_pt[i] + ray[i] * 0.01);
    state.traps.iter().position(|t| {
        let (min, max) =
            get_outer_face_selections(t.button_pos, t.button_size, t.button_orient)[0].bounds();
        (0..3).all(|i| inside[i] >= min[i] as f32 - 0.05 && inside[i] <= max[i] as f32 + 0.05)
    })
}

/// Hit point, face orient and world size of the first surface along the ray.
fn raycast(state: &DeathrunState, origin: [f32; 3], ray: [f32; 3]) -> Option<([f32; 3], u8, i32)> {
    let map = state.map.as_ref()?;
    let hit = sauer_map::raycast::raycast_with_filter(map, origin, ray, MAX_REACH, |c| {
        !c.is_empty() || c.material.volume == sauer_core::Volume::Glass
    })?;
    let pt = std::array::from_fn(|i| origin[i] + ray[i] * hit.dist);
    Some((pt, hit.orient, map.size))
}

/// Frames of the traps whose geometry covers `cell`.
fn frames_around(state: &DeathrunState, cell: Box3) -> Vec<Box3> {
    state
        .traps
        .iter()
        .filter_map(|t| t.geom.as_ref().map(|g| g.clip_box()))
        .filter(|&frame| overlaps(frame, cell))
        .collect()
}

fn verdict(state: &DeathrunState, moving: Option<usize>, key: ButtonKey) -> Verdict {
    let cell = cell_box(key);
    if !state.active_trap_stack.is_empty() {
        return Verdict::Bad("Wait for the active traps to reset.");
    }
    if state.vault.is_some_and(|v| overlaps(cell, v.bounds())) {
        return Verdict::Bad("That is the trap vault.");
    }
    let taken = state
        .traps
        .iter()
        .enumerate()
        .any(|(i, t)| Some(i) != moving && overlaps(cell, t.button_box()));
    if taken {
        return Verdict::Bad("Another trap's button is in the way.");
    }
    if frames_around(state, cell).is_empty() {
        Verdict::Good
    } else {
        Verdict::InFrame
    }
}

/// Where the button would go for a player aiming along `ray`.
fn aim(state: &DeathrunState, moving: Option<usize>, origin: [f32; 3], ray: [f32; 3]) -> Target {
    // Other buttons count as any surface: the new one goes on top.
    if moving.is_some() && button_hit(state, origin, ray) == moving {
        return Target::Back;
    }
    let Some((hit_pt, orient, world_size)) = raycast(state, origin, ray) else {
        return Target::Nowhere;
    };
    let Some(pos) = cell_at(hit_pt, orient, world_size) else {
        return Target::Nowhere;
    };
    let key = (pos, BUTTON_SIZE, orient);
    if moving.is_some_and(|i| button_key(&state.traps[i]) == key) {
        return Target::Back;
    }
    Target::Cell(key, verdict(state, moving, key))
}

// ─── Showing the preview ─────────────────────────────────────────────────────

/// Paste packets drawing the preview of `target`. Builds it in the server's
/// map just long enough to copy it out.
fn ghost(map: &mut Map, target: Target) -> Vec<ClipboardPayload> {
    let Target::Cell(key, verdict) = target else {
        return Vec::new();
    };
    let (pos, size, orient) = key;
    let (min, max) = cell_box(key);
    let (o, s, grid, saved) = sauer_map::clipboard::generate_region_clipboard(map, min, max);
    build_button(map, pos, size, orient, verdict.color(), true);
    let out = map.generate_restore_packets(min, max);
    map.apply_block(&super::vault::sel(o, s, grid), saved.nodes, false);
    out
}

/// Paste packets putting the server's version back where `target` was drawn.
fn unghost(map: &Map, target: Target) -> Vec<ClipboardPayload> {
    match target {
        Target::Cell(key, _) => {
            let (min, max) = cell_box(key);
            map.generate_restore_packets(min, max)
        }
        _ => Vec::new(),
    }
}

fn frames(state: &DeathrunState, target: Target) -> Vec<FakeEnt> {
    let Target::Cell(key, Verdict::InFrame) = target else {
        return Vec::new();
    };
    frames_around(state, cell_box(key))
        .into_iter()
        .flat_map(|(min, max)| super::guard::box_edges(min, max, None))
        .map(|(x, y, z, dir, len)| FakeEnt {
            ent_type: EntityType::Particles,
            position: [x as f32, y as f32, z as f32],
            attrs: [7, dir, len, FRAME_COLOR, 0],
        })
        .collect()
}

fn send_virt(server: &Server, id: u64, up: VirtEntUpdates) {
    let cn = crate::SERVER_BOT_CN;
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

fn send_frames(server: &Server, state: &mut DeathrunState, target: Target) {
    let ents = frames(state, target);
    let Some(map) = state.map.as_mut() else {
        return;
    };
    for id in server.client_ids() {
        let up = map.update_virt_ents(id, FRAMES, ents.clone());
        send_virt(server, id, up);
    }
}

/// Moves the preview everyone sees from `old` to `new`.
fn show(server: &Server, state: &mut DeathrunState, old: Target, new: Target) {
    let Some(map) = state.map.as_mut() else {
        return;
    };
    let mut payloads = unghost(map, old);
    payloads.extend(ghost(map, new));
    crate::geo_sync::broadcast_now(server, crate::SERVER_BOT_CN, payloads);
    send_frames(server, state, new);
}

/// Draws the preview again, after map geometry was pasted over it.
pub fn repaint(server: &Server) {
    let mut state = get_server_state(server).write();
    let Some(target) = state.mode.placing().map(|p| p.target) else {
        return;
    };
    if let Some(map) = state.map.as_mut() {
        let payloads = ghost(map, target);
        crate::geo_sync::broadcast_now(server, crate::SERVER_BOT_CN, payloads);
    }
}

/// For a player who just loaded the map, which has neither the preview nor
/// the frames.
pub fn reshow_for(server: &Server, id: u64) {
    {
        let mut state = get_server_state(server).write();
        let Some(target) = state.mode.placing().map(|p| p.target) else {
            return;
        };
        if let Some(map) = state.map.as_mut() {
            let up = map.clear_virt_ents(id, FRAMES);
            send_virt(server, id, up);
        }
        send_frames(server, &mut state, target);
    }
    repaint(server);
}

// ─── Commands ────────────────────────────────────────────────────────────────

fn name(server: &Server, id: u64) -> String {
    server
        .players
        .with(id, |p| p.name.strip_colors().to_utf8())
        .unwrap_or_else(|| "Someone".to_string())
}

fn announce(server: &Server, text: String) {
    server.broadcast_chan1(&channel1::ServMsg {
        text: format!("\x0C6[TRAP]\x0C7 {text}").lossy_cubestring(),
    });
}

/// Who is placing a button, for commands that have to wait for it.
pub fn busy_message(server: &Server, state: &DeathrunState) -> Option<String> {
    let p = state.mode.placing()?;
    Some(format!(
        "{} is placing a trap button. Wait until they finish.",
        name(server, p.player)
    ))
}

/// `#tnew` and `#tmove`: starts placing a button, places it, or cancels.
pub fn command(server: &Arc<Server>, id: u64, kind: Kind, origin: [f32; 3], ray: [f32; 3]) {
    let mut state = get_server_state(server).write();
    let refusal = match &state.mode {
        TrapMode::Editing(e) => Some(format!(
            "Trap {} is being edited. Type #tedit to finish editing first.",
            e.trap_idx
        )),
        TrapMode::Placing(p) if p.player != id => busy_message(server, &state),
        TrapMode::Placing(p) if p.kind() != kind => Some(format!(
            "You are placing a button already. Type {} to place it or cancel.",
            p.kind().command()
        )),
        _ => None,
    };
    if let Some(text) = refusal {
        crate::ui::send_warn(server, id, &text);
    } else if state.mode.placing().is_some() {
        finish(server, &mut state, id, origin, ray);
    } else {
        start(server, state, id, kind, origin, ray);
    }
}

fn start(
    server: &Arc<Server>,
    mut state: parking_lot::RwLockWriteGuard<'_, DeathrunState>,
    id: u64,
    kind: Kind,
    origin: [f32; 3],
    ray: [f32; 3],
) {
    let moving = match kind {
        Kind::New => None,
        Kind::Move => {
            let Some(i) = button_hit(&state, origin, ray) else {
                crate::ui::send_warn(server, id, "Aim at a trap button to move it.");
                return;
            };
            if state.traps[i].is_active {
                crate::ui::send_warn(server, id, "That trap is active. Wait for it to reset.");
                return;
            }
            Some(i)
        }
    };
    let target = aim(&state, moving, origin, ray);
    state.mode = TrapMode::Placing(Placing {
        player: id,
        trap: moving,
        target,
    });
    show(server, &mut state, Target::Nowhere, target);
    drop(state);

    let cmd = kind.command();
    let what = match moving {
        Some(i) => format!("moving the button of trap {i}"),
        None => "placing a new trap button".to_string(),
    };
    announce(server, format!("{} is {what}.", name(server, id)));
    let back = match kind {
        Kind::New => "aim where it can't go (red)",
        Kind::Move => "aim back at the button",
    };
    crate::ui::send(
        server,
        id,
        crate::ui::body(&format!(
            "  Aim where it should go and shoot or type {cmd} again. Green: fine. Yellow: inside a trap's frame (shown), its geometry covers the button while it is active. Red: can't go there. To cancel, {back} and shoot or type {cmd}."
        )),
    );
}

fn finish(
    server: &Arc<Server>,
    state: &mut DeathrunState,
    id: u64,
    origin: [f32; 3],
    ray: [f32; 3],
) {
    let Some(p) = state.mode.placing() else {
        return;
    };
    let moving = p.trap;
    let target = aim(state, moving, origin, ray);
    let refused = match target {
        Target::Cell(key, Verdict::Good | Verdict::InFrame) => Ok(key),
        Target::Back => Err(None),
        Target::Cell(_, Verdict::Bad(why)) => Err(Some(why)),
        Target::Nowhere => Err(Some("Aim at a surface inside the world.")),
    };
    let key = match refused {
        Ok(key) => key,
        // A move goes on until it is put down or aimed back; there is no
        // spot to aim back at for a new trap, so a refused spot cancels it.
        Err(Some(why)) if moving.is_some() => {
            follow_locked(server, state, target);
            crate::ui::send_warn(server, id, why);
            return;
        }
        Err(_) => {
            end(server, state);
            announce(
                server,
                format!("{} cancelled placing the button.", name(server, id)),
            );
            return;
        }
    };

    let mut sync = persist::Sync::default();
    let old = moving.map(|i| button_key(&state.traps[i]));
    if let Err(e) = set_button_cells(state, old, Some(key), &mut sync) {
        crate::ui::send_warn(server, id, &format!("\x0C3[ERROR]\x0C7 {e}"));
        return;
    }
    let idx = match moving {
        Some(i) => {
            let t = &mut state.traps[i];
            (t.button_pos, t.button_size, t.button_orient) = key;
            i
        }
        None => {
            state.traps.push(Trap::new(key.0, key.1, key.2));
            state.traps.len() - 1
        }
    };
    // Back to idle first: the preview cell is in `sync`, so the server's
    // version (now the button) is pasted over it.
    if let Some(cell) = state.mode.placing().and_then(Placing::preview_box) {
        sync.dirty.push(cell);
    }
    state.mode = TrapMode::Idle;
    send_frames(server, state, Target::Nowhere);
    let result = persist::save(state, &mut sync);
    let out = persist::packets(state, sync);
    persist::send(server, out);

    let verb = if moving.is_some() { "moved" } else { "placed" };
    let mut text = format!("{} {verb} the button of trap {idx}.", name(server, id));
    if matches!(target, Target::Cell(_, Verdict::InFrame)) {
        text.push_str(" It is inside a trap's frame.");
    }
    announce(server, text);
    if let Err(e) = result {
        crate::ui::send_warn(server, id, &format!("\x0C3[ERROR]\x0C7 {e}"));
    }
}

/// Ends placing without changing anything. Call with the state locked.
fn end(server: &Server, state: &mut DeathrunState) {
    let Some(shown) = state.mode.placing().map(|p| p.target) else {
        return;
    };
    state.mode = TrapMode::Idle;
    show(server, state, shown, Target::Nowhere);
}

/// Cancels placing if `id` is the one doing it (they left or went spectator).
pub fn cancel_for(server: &Server, id: u64, why: &str) {
    if !is_placer(&get_server_state(server).read(), id) {
        return;
    }
    let mut state = get_server_state(server).write();
    if !is_placer(&state, id) {
        return;
    }
    end(server, &mut state);
    drop(state);
    announce(
        server,
        format!("Placing the trap button was cancelled: {why}."),
    );
}

/// What `id` is placing, if they are the one placing a button.
pub fn placing_kind(server: &Server, id: u64) -> Option<Kind> {
    let state = get_server_state(server).read();
    state
        .mode
        .placing()
        .filter(|p| p.player == id)
        .map(Placing::kind)
}

fn is_placer(state: &DeathrunState, id: u64) -> bool {
    state.mode.placing().is_some_and(|p| p.player == id)
}

fn follow_locked(server: &Server, state: &mut DeathrunState, target: Target) {
    let TrapMode::Placing(p) = &mut state.mode else {
        return;
    };
    let shown = std::mem::replace(&mut p.target, target);
    if shown != target {
        show(server, state, shown, target);
    }
}

/// Moves the preview along with the placing player's aim.
pub fn follow(server: &Server, id: u64, origin: [f32; 3], ray: [f32; 3]) {
    // Checked under the read lock first: every player's position update gets here.
    if !is_placer(&get_server_state(server).read(), id) {
        return;
    }
    let mut state = get_server_state(server).write();
    if !is_placer(&state, id) {
        return;
    }
    let moving = state.mode.placing().and_then(|p| p.trap);
    let target = aim(&state, moving, origin, ray);
    follow_locked(server, &mut state, target);
}

// ─── Button cells and trap recordings ────────────────────────────────────────

/// Turns the `old` button cell into air and builds a button in the `new` one,
/// in every server copy of the map. Traps whose frame covers either cell get
/// their recorded geometry updated to match, so firing or editing them never
/// brings back a button that moved away or covers one that moved in.
/// Changes nothing if that would put a trap over its size limit.
pub fn set_button_cells(
    state: &mut DeathrunState,
    old: Option<ButtonKey>,
    new: Option<ButtonKey>,
    sync: &mut persist::Sync,
) -> Result<(), String> {
    let cells: Vec<Box3> = old.iter().chain(new.iter()).map(|&k| cell_box(k)).collect();

    let mut updates = Vec::new();
    let DeathrunState { map, traps, .. } = &mut *state;
    let map = map.as_mut().ok_or("No map is loaded.")?;
    for (i, t) in traps.iter().enumerate() {
        let Some(g) = &t.geom else { continue };
        if !cells.iter().any(|&c| overlaps(c, g.clip_box())) {
            continue;
        }
        let geom = rerecord(map, g, old, new);
        let size = super::commands::payload_size(&geom.payload);
        if size > super::TRAP_BUDGET {
            return Err(format!(
                "Trap {i} would go over its size limit ({} KB of {} KB) with the button there.",
                size / 1024,
                super::TRAP_BUDGET / 1024
            ));
        }
        updates.push((i, geom));
    }

    for (i, geom) in updates {
        if let Some(g) = state.traps[i].geom.take() {
            persist::free_shelf(state, &g, sync);
        }
        state.traps[i].geom = Some(geom);
    }
    for m in persist::maps(state) {
        if let Some(key) = old {
            let (min, max) = cell_box(key);
            m.unprotect_region(min, max);
            clear_button_cell(m, key.0, key.1);
        }
        if let Some(key) = new {
            let (min, max) = cell_box(key);
            build_button_geometry(m, key.0, key.1, key.2);
            m.protect_region(min, max);
        }
        m.invalidate_cache();
    }
    sync.dirty.extend(cells);
    Ok(())
}

/// `g` recorded again with the `old` button cell as air and a button in the
/// `new` one, where they fall in its frame.
fn rerecord(
    map: &mut Map,
    g: &TrapGeom,
    old: Option<ButtonKey>,
    new: Option<ButtonKey>,
) -> TrapGeom {
    let clip = g.clip_box();
    let touched = |k: &ButtonKey| overlaps(cell_box(*k), clip);
    let old = old.filter(touched);
    let new = new.filter(touched);

    // Everything changed below gets put back from this.
    let mut region = clip;
    for (min, max) in old.iter().chain(new.iter()).map(|&k| cell_box(k)) {
        for i in 0..3 {
            region.0[i] = region.0[i].min(min[i]);
            region.1[i] = region.1[i].max(max[i]);
        }
    }
    let (o, s, grid, saved) =
        sauer_map::clipboard::generate_region_clipboard(map, region.0, region.1);

    let mut sel = g.payload.sel.clone();
    sel.o = Vec3 {
        x: g.clip_pos[0],
        y: g.clip_pos[1],
        z: g.clip_pos[2],
    };
    map.apply_block(&sel, g.payload.nodes.clone(), false);
    if let Some((pos, size, _)) = old {
        clear_button_cell(map, pos, size);
    }
    if let Some((pos, size, orient)) = new {
        build_button_geometry(map, pos, size, orient);
    }
    let geom = super::commands::capture_geometry(map, clip);

    map.apply_block(&super::vault::sel(o, s, grid), saved.nodes, false);
    geom
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::trap::commands::capture_geometry;
    use crate::trap::vault;

    fn solid(map: &Map, p: [i32; 3]) -> bool {
        let c = p.map(|v| v as f32 + 0.5);
        !map.get_cube(c[0], c[1], c[2]).unwrap().0.is_empty()
    }

    /// What the trap's recording puts in the map when it fires.
    fn fired(state: &DeathrunState) -> Map {
        let mut map = state.map.clone().unwrap();
        super::super::trigger::apply_active_state(&mut map, &state.traps[0], &[]);
        map
    }

    #[test]
    fn recordings_follow_buttons_in_and_out_of_frames() {
        let map = Map::new_empty(1024);
        let mut active = map.clone();
        active.fill(&vault::sel([64, 64, 512], [1, 1, 1], 16), true);
        let mut trap = Trap::new([0, 0, 512], BUTTON_SIZE, 0);
        trap.geom = Some(capture_geometry(&active, ([64, 64, 512], [128, 128, 576])));
        let mut state = DeathrunState {
            map: Some(map),
            traps: vec![trap],
            ..Default::default()
        };

        // On the floor of the frame, facing up: the slab is the cell's bottom.
        let inside = ([96, 96, 520], BUTTON_SIZE, 5);
        set_button_cells(&mut state, None, Some(inside), &mut Default::default()).unwrap();
        assert!(solid(state.map.as_ref().unwrap(), [96, 96, 520]));
        let f = fired(&state);
        assert!(solid(&f, [96, 96, 520]), "firing covers the button");
        assert!(!solid(&f, [96, 96, 526]), "button cell not air when fired");
        assert!(solid(&f, [70, 70, 520]), "trap geometry lost");

        let outside = ([0, 8, 512], BUTTON_SIZE, 0);
        set_button_cells(
            &mut state,
            Some(inside),
            Some(outside),
            &mut Default::default(),
        )
        .unwrap();
        assert!(!solid(state.map.as_ref().unwrap(), [96, 96, 520]));
        let f = fired(&state);
        assert!(
            !solid(&f, [96, 96, 520]),
            "firing brings the old button back"
        );
        assert!(solid(&f, [70, 70, 520]), "trap geometry lost");
    }
}
