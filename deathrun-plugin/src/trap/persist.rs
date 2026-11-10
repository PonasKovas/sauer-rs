//! Keeps the vault in the map in step with `DeathrunState::traps`.
//!
//! Every change goes to all server-side copies of the map (live map, base map
//! while traps are active, and the snapshot of an ongoing trap edit), and is
//! described by a [`Sync`] that tells clients what to reload.

use super::ents::{self, EntRec};
use super::record::{GeomRecord, TAG, TrapRecord, VaultRecord};
use super::vault::{self, Item, Vault};
use super::{Trap, TrapGeom};
use crate::map_state::DeathrunState;
use sauer_map::Map;
use sauer_proto::data::{ClipboardPayload, VSlotDelta};
use sauer_proto::{Client, channel1};
use sauer_server::Server;
use std::sync::Arc;

type Box3 = ([i32; 3], [i32; 3]);

/// Highest world size the engine allows (`mapscale` 16).
const MAX_WORLD: i32 = 1 << 16;

/// What clients need to be sent after the vault changed.
#[derive(Default)]
pub struct Sync {
    pub enlarges: u32,
    pub dirty: Vec<Box3>,
}

impl Sync {
    fn mark(&mut self, b: Box3) {
        self.dirty.push(b);
    }

    pub fn is_empty(&self) -> bool {
        self.enlarges == 0 && self.dirty.is_empty()
    }
}

fn maps(state: &mut DeathrunState) -> Vec<&mut Map> {
    let DeathrunState {
        map,
        base_map,
        trap_edit,
        ..
    } = state;
    map.iter_mut()
        .chain(base_map.iter_mut().map(|m| &mut **m))
        .chain(trap_edit.iter_mut().map(|e| &mut *e.original_map))
        .collect()
}

fn record(vault: &Vault, traps: &[Trap], generation: u32) -> VSlotDelta {
    let rec = VaultRecord {
        origin: vault.origin,
        size: vault.size,
        traps: traps
            .iter()
            .map(|t| TrapRecord {
                button_pos: t.button_pos,
                button_size: t.button_size,
                button_orient: t.button_orient,
                time: t.time,
                timeout: t.timeout,
                geom: t.geom.as_ref().and_then(|g| {
                    Some(GeomRecord {
                        clip_pos: g.clip_pos,
                        clip_s: g.clip_s,
                        clip_grid: g.clip_grid,
                        shelf: g.shelf?,
                    })
                }),
                ents: t.ents.clone(),
            })
            .collect(),
    };
    sauer_map::param_blob::encode(TAG, generation, &rec.to_bytes())
}

/// Enlarges every server copy of the map. Clients run the engine's enlarge,
/// which fills the new bottom octants with rock, so those are resent as air.
fn enlarge_maps(state: &mut DeathrunState, sync: &mut Sync) {
    for m in maps(state) {
        m.enlarge();
    }
    if let Some(map) = state.map.as_ref() {
        sync.dirty.extend(map.enlarged_octants());
    }
}

/// Enlarges the map from the server side (clients are told with `N_NEWMAP`).
fn enlarge(state: &mut DeathrunState, sync: &mut Sync) -> Result<(), String> {
    if state.map.as_ref().map_or(MAX_WORLD, |m| m.size) >= MAX_WORLD {
        return Err("the map is already at the maximum size, so the trap vault can't grow".into());
    }
    enlarge_maps(state, sync);
    sync.enlarges += 1;
    Ok(())
}

/// A client enlarged the map (already relayed to everyone): follow it, turn
/// the new rock into air for everyone, and move the vault to the new corner.
pub fn client_enlarged(state: &mut DeathrunState, sync: &mut Sync) -> Result<(), String> {
    if state.map.as_ref().map_or(MAX_WORLD, |m| m.size) >= MAX_WORLD {
        return Ok(()); // the engine refuses this too
    }
    enlarge_maps(state, sync);
    relocate_if_needed(state, sync)
}

/// Clears a trap's old shelf before its geometry is replaced or removed.
pub fn free_shelf(state: &mut DeathrunState, geom: &TrapGeom, sync: &mut Sync) {
    let Some(v) = state.vault else { return };
    let mut freed = None;
    for m in maps(state) {
        freed = vault::clear_shelf(m, &v, geom);
    }
    if let Some(b) = freed {
        sync.mark(b);
    }
}

/// Vault spot for a max size map, which can't grow to make room: the bottom
/// corner farthest from where people play, overwriting whatever is there.
/// "Where people play" is the per-axis median of spawns and teleports, so a
/// few far-off ones don't pull it around. An existing vault keeps its corner,
/// so it never moves and leaves a hole behind.
fn corner_away_from_play(map: &Map, size: i32, old: Option<Vault>) -> Vault {
    use ogz_rs::data::EntityType;
    let far = map.size - size;
    if let Some(o) = old {
        let side = |v: i32| if v == 0 { 0 } else { far };
        return Vault {
            origin: [side(o.origin[0]), side(o.origin[1]), 0],
            size,
        };
    }

    let (mut xs, mut ys): (Vec<f32>, Vec<f32>) = map
        .entities
        .iter()
        .filter(|e| {
            matches!(
                e.ent_type,
                EntityType::PlayerStart | EntityType::Teleport | EntityType::TeleDest
            )
        })
        .map(|e| (e.position[0], e.position[1]))
        .unzip();
    let median = |v: &mut Vec<f32>| {
        if v.is_empty() {
            return map.size as f32 / 2.0;
        }
        v.sort_by(f32::total_cmp);
        v[v.len() / 2]
    };
    let center = [median(&mut xs), median(&mut ys)];

    [[0, 0], [far, 0], [0, far], [far, far]]
        .into_iter()
        .map(|[x, y]| Vault {
            origin: [x, y, 0],
            size,
        })
        .max_by(|a, b| {
            let dist = |v: &Vault| {
                let mid = v.origin.map(|o| (o + v.size / 2) as f32);
                (mid[0] - center[0]).powi(2) + (mid[1] - center[1]).powi(2)
            };
            dist(a).total_cmp(&dist(b))
        })
        .unwrap()
}

/// Lays every shelf out again in a vault at the far corner, growing it and
/// enlarging the world as needed, then rebuilds it there.
fn rebuild(state: &mut DeathrunState, sync: &mut Sync, fresh_octant: bool) -> Result<(), String> {
    let old = state.vault;
    let mut idx: Vec<usize> = (0..state.traps.len())
        .filter(|&i| state.traps[i].geom.is_some())
        .collect();
    let volume = |i: &usize| {
        let e = state.traps[*i].geom.as_ref().unwrap().extent();
        -(e[0] as i64 * e[1] as i64 * e[2] as i64)
    };
    idx.sort_by_key(volume);
    let items: Vec<Item> = idx
        .iter()
        .map(|&i| Item::of(state.traps[i].geom.as_ref().unwrap()))
        .collect();
    let start = old.map_or(vault::MIN_SIZE, |v| v.size);
    let (size, positions) = vault::pack_fresh(start, &items)
        .ok_or("the traps' geometry is too big to fit in the vault")?;

    let at_max = |state: &DeathrunState| state.map.as_ref().map_or(true, |m| m.size >= MAX_WORLD);
    if fresh_octant && !at_max(state) {
        enlarge(state, sync)?;
    }
    let target = loop {
        let map = state.map.as_ref().ok_or("no map loaded")?;
        if at_max(state) && size <= map.size / 2 {
            break corner_away_from_play(map, size, old);
        }
        let target = Vault::at_far_corner(map.size, size);
        if size <= map.size / 2 && vault::is_free(map, target.bounds(), old.map(|v| v.bounds())) {
            break target;
        }
        enlarge(state, sync)?;
    };

    for (&i, pos) in idx.iter().zip(positions) {
        state.traps[i].geom.as_mut().unwrap().shelf = Some(pos);
    }
    if let Some(o) = old {
        for m in maps(state) {
            vault::erase(m, &o);
            m.unprotect_region(o.bounds().0, o.bounds().1);
        }
        sync.mark(o.bounds());
    }
    state.vault = Some(target);
    state.record_generation += 1;
    let rec = record(&target, &state.traps, state.record_generation);
    let traps = state.traps.clone();
    for m in maps(state) {
        vault::build(m, &target, &traps, &rec);
        m.protect_region(target.bounds().0, target.bounds().1);
    }
    sync.mark(target.bounds());
    Ok(())
}

/// Writes any new shelves and the trap record into the map, creating the
/// vault on first use. Must be called after every change to `state.traps`.
pub fn save(state: &mut DeathrunState, sync: &mut Sync) -> Result<(), String> {
    let Some(v) = state.vault else {
        if state.traps.is_empty() {
            return Ok(());
        }
        // First trap in this map: enlarge so the vault sits in fresh space, away from the map.
        return rebuild(state, sync, true);
    };

    let unplaced: Vec<usize> = (0..state.traps.len())
        .filter(|&i| {
            state.traps[i]
                .geom
                .as_ref()
                .is_some_and(|g| g.shelf.is_none())
        })
        .collect();
    if !unplaced.is_empty() {
        let taken: Vec<Box3> = state
            .traps
            .iter()
            .filter_map(|t| t.geom.as_ref())
            .filter_map(|g| {
                let shelf = g.shelf?;
                let ext = g.extent();
                Some((shelf, std::array::from_fn(|i| shelf[i] + ext[i])))
            })
            .collect();
        let items: Vec<Item> = unplaced
            .iter()
            .map(|&i| Item::of(state.traps[i].geom.as_ref().unwrap()))
            .collect();
        let Some(positions) = vault::pack(v.size, &taken, &items) else {
            return rebuild(state, sync, false);
        };
        for (&i, pos) in unplaced.iter().zip(positions) {
            let geom = state.traps[i].geom.as_mut().unwrap();
            geom.shelf = Some(pos);
            let geom = geom.clone();
            for m in maps(state) {
                vault::write_shelf(m, &v, &geom);
            }
            sync.mark(v.shelf_box(&geom).unwrap());
        }
    }

    state.record_generation += 1;
    let rec = record(&v, &state.traps, state.record_generation);
    for m in maps(state) {
        vault::write_anchor(m, &v, &rec);
    }
    sync.mark(v.anchor_box());
    Ok(())
}

/// Moves the vault back to the far corner after the world was enlarged.
pub fn relocate_if_needed(state: &mut DeathrunState, sync: &mut Sync) -> Result<(), String> {
    let (Some(v), Some(map)) = (state.vault, state.map.as_ref()) else {
        return Ok(());
    };
    if v.is_at_far_corner(map.size) {
        return Ok(());
    }
    let mut target = Vault::at_far_corner(map.size, v.size);
    while !vault::is_free(state.map.as_ref().unwrap(), target.bounds(), None) {
        enlarge(state, sync)?;
        target = Vault::at_far_corner(state.map.as_ref().unwrap().size, v.size);
    }
    for m in maps(state) {
        vault::erase(m, &v);
        m.unprotect_region(v.bounds().0, v.bounds().1);
    }
    sync.mark(v.bounds());
    state.vault = Some(target);
    state.record_generation += 1;
    let rec = record(&target, &state.traps, state.record_generation);
    let traps = state.traps.clone();
    for m in maps(state) {
        vault::build(m, &target, &traps, &rec);
        m.protect_region(target.bounds().0, target.bounds().1);
    }
    sync.mark(target.bounds());
    Ok(())
}

/// Builds the packets for `sync` from the current map. Call under the state lock.
pub fn packets(state: &DeathrunState, sync: Sync) -> (u32, Vec<ClipboardPayload>) {
    let Some(map) = state.map.as_ref() else {
        return (sync.enlarges, Vec::new());
    };
    let mut boxes: Vec<Box3> = Vec::new();
    for b in sync.dirty {
        let inside = |o: &Box3| (0..3).all(|i| o.0[i] <= b.0[i] && b.1[i] <= o.1[i]);
        if !boxes.iter().any(inside) {
            boxes.retain(|o| !(0..3).all(|i| b.0[i] <= o.0[i] && o.1[i] <= b.1[i]));
            boxes.push(b);
        }
    }
    let payloads = boxes
        .into_iter()
        .flat_map(|(min, max)| map.generate_restore_packets(min, max))
        .collect();
    (sync.enlarges, payloads)
}

/// Sends what `packets` produced to every client, in order.
pub fn send(server: &Arc<Server>, (enlarges, payloads): (u32, Vec<ClipboardPayload>)) {
    if enlarges == 0 && payloads.is_empty() {
        return;
    }
    let cn = crate::SERVER_BOT_CN as i32;
    for _ in 0..enlarges {
        server.broadcast_chan1(&Client {
            client_num: cn,
            packet: channel1::NewMap { size: -1 },
        });
    }
    let server = server.clone();
    tokio::spawn(async move {
        crate::geo_sync::broadcast_payloads(&server, cn, payloads, 0).await;
    });
}

/// Saves under the lock and sends the result. Errors are reported to `report_to`.
pub fn save_and_send(server: &Arc<Server>, report_to: Option<u64>) {
    let mut state = crate::map_state::get_server_state(server).write().unwrap();
    let mut sync = Sync::default();
    let result = save(&mut state, &mut sync);
    let out = packets(&state, sync);
    drop(state);
    send(server, out);
    if let Err(e) = result {
        let msg = format!("\x0C3[ERROR]\x0C7 Could not save traps to the map: {e}");
        match report_to {
            Some(id) => crate::ui::send_warn(server, id, &msg),
            None => tracing::error!("{msg}"),
        }
    }
}

// ─── Loading ─────────────────────────────────────────────────────────────────

pub struct Loaded {
    pub traps: Vec<Trap>,
    pub vault: Option<Vault>,
    pub generation: u32,
    pub warnings: Vec<String>,
}

/// Reads the trap record and shelves out of a freshly loaded map.
pub fn load(map: &Map) -> Loaded {
    let mut out = Loaded {
        traps: Vec::new(),
        vault: None,
        generation: 0,
        warnings: Vec::new(),
    };
    let Some((generation, bytes)) = sauer_map::param_blob::find_latest(map, TAG) else {
        // Chunks of the pre-vault store had names starting with byte 1.
        let old_store = map.vslots.iter().flatten().any(|vs| {
            vs.params
                .iter()
                .any(|p| p.name.as_bytes().first() == Some(&1))
        });
        if old_store {
            out.warnings.push(
                "This map has traps saved in the old format, which is no longer supported. They were not loaded.".into(),
            );
        }
        return out;
    };
    let rec = match VaultRecord::from_bytes(&bytes) {
        Ok(r) => r,
        Err(e) => {
            out.warnings
                .push(format!("Trap data in this map is unreadable ({e})."));
            return out;
        }
    };
    let v = Vault {
        origin: rec.origin,
        size: rec.size,
    };
    out.vault = Some(v);
    out.generation = generation;

    for tr in rec.traps {
        let mut trap = Trap::new(tr.button_pos, tr.button_size, tr.button_orient);
        trap.time = tr.time;
        trap.timeout = tr.timeout;
        trap.ents = tr.ents;
        if let Some(g) = tr.geom {
            let mut geom = TrapGeom {
                clip_pos: g.clip_pos,
                clip_s: g.clip_s,
                clip_grid: g.clip_grid,
                payload: ClipboardPayload {
                    unpack_len: 0,
                    sel: vault::sel(g.clip_pos, g.clip_s, g.clip_grid),
                    nodes: Vec::new(),
                    vslots: Vec::new(),
                },
                shelf: Some(g.shelf),
            };
            match vault::read_shelf(map, &v, &geom) {
                Some(payload) => {
                    geom.payload = payload;
                    trap.geom = Some(geom);
                }
                None => out.warnings.push(format!(
                    "The geometry of {} is missing from the vault.",
                    describe_trap(&trap)
                )),
            }
        }
        for rec in ents::unlink_missing(&mut trap.ents, &map.entities, &[]) {
            out.warnings.push(unlinked_message(&trap, &rec));
        }
        out.traps.push(trap);
    }
    out
}

pub fn describe_trap(trap: &Trap) -> String {
    let p = trap.button_pos;
    format!("the trap at ({}, {}, {})", p[0], p[1], p[2])
}

pub fn unlinked_message(trap: &Trap, rec: &EntRec) -> String {
    let ty = rec.to_entity().ent_type;
    format!(
        "Entity {ty:?} at ({:.0}, {:.0}, {:.0}) changed, so it is no longer part of {}. Use #edittrap on it if the entity should still be involved.",
        rec.pos[0],
        rec.pos[1],
        rec.pos[2],
        describe_trap(trap)
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::trap::commands::capture_geometry;
    use crate::trap::ents::EntChange;
    use ogz_rs::data::{Entity, EntityType};

    fn light(pos: [f32; 3]) -> Entity {
        Entity {
            position: pos,
            attrs: [64, 255, 255, 255, 0],
            ent_type: EntityType::Light,
            extra_data: vec![],
        }
    }

    /// A map with a pillar to turn into trap geometry and a light the trap deletes.
    fn setup() -> DeathrunState {
        let mut map = Map::new_empty(1024);
        map.entities.push(light([100.0, 100.0, 600.0]));
        let mut active = map.clone();
        active.fill(&vault::sel([96, 96, 512], [1, 1, 4], 8), true);
        let geom = capture_geometry(&active, ([96, 96, 512], [104, 104, 544]));

        let mut trap = Trap::new([8, 8, 512], 8, 0);
        trap.geom = Some(geom);
        trap.ents = vec![EntChange::Removed(ents::EntRec::of(&map.entities[0]))];
        DeathrunState {
            map: Some(map),
            traps: vec![trap],
            ..Default::default()
        }
    }

    fn reload(state: &DeathrunState) -> (Map, Loaded) {
        let bytes = state.map.as_ref().unwrap().to_ogz_bytes().unwrap();
        let map = Map::from_bytes(&bytes).unwrap();
        let loaded = load(&map);
        (map, loaded)
    }

    #[test]
    fn first_save_enlarges_and_survives_ogz_roundtrip() {
        let mut state = setup();
        let mut sync = Sync::default();
        save(&mut state, &mut sync).unwrap();
        assert_eq!(sync.enlarges, 1);
        let v = state.vault.unwrap();
        assert!(v.is_at_far_corner(2048));
        // Fresh octant only: nowhere near the original 1024 map.
        assert!(v.origin[0] >= 1024 && v.origin[1] >= 1024);

        let (_, loaded) = reload(&state);
        assert!(loaded.warnings.is_empty(), "{:?}", loaded.warnings);
        assert_eq!(loaded.vault, Some(v));
        assert_eq!(loaded.traps.len(), 1);
        let (a, b) = (&loaded.traps[0], &state.traps[0]);
        assert_eq!(a.ents, b.ents);
        let (ga, gb) = (a.geom.as_ref().unwrap(), b.geom.as_ref().unwrap());
        assert_eq!(ga.clip_box(), gb.clip_box());
        assert_eq!(ga.payload.nodes, gb.payload.nodes);
    }

    #[test]
    fn metadata_save_only_touches_anchor() {
        let mut state = setup();
        save(&mut state, &mut Sync::default()).unwrap();
        state.traps[0].time = 1234;
        let mut sync = Sync::default();
        save(&mut state, &mut sync).unwrap();
        assert_eq!(sync.enlarges, 0);
        assert_eq!(sync.dirty, vec![state.vault.unwrap().anchor_box()]);
        assert_eq!(reload(&state).1.traps[0].time, 1234);
    }

    #[test]
    fn relocates_after_enlarge() {
        let mut state = setup();
        save(&mut state, &mut Sync::default()).unwrap();
        let old = state.vault.unwrap();

        let mut sync = Sync::default();
        client_enlarged(&mut state, &mut sync).unwrap();
        assert_eq!(sync.enlarges, 0, "the client's enlarge was already relayed");
        let map = state.map.as_ref().unwrap();
        for octant in map.enlarged_octants() {
            assert!(sync.dirty.contains(&octant), "new octant not resent as air");
        }
        let new = state.vault.unwrap();
        assert!(new.is_at_far_corner(4096));
        let empty = |p: [i32; 3]| {
            map.get_cube(p[0] as f32, p[1] as f32, p[2] as f32)
                .unwrap()
                .0
                .is_empty()
        };
        assert!(empty(old.origin), "old vault left behind");
        assert!(
            empty([3000, 100, 10]) && empty([100, 3000, 10]),
            "new bottom octants not air"
        );

        let (_, loaded) = reload(&state);
        assert_eq!(loaded.vault, Some(new));
        assert_eq!(
            loaded.traps[0].geom.as_ref().unwrap().payload.nodes,
            state.traps[0].geom.as_ref().unwrap().payload.nodes
        );
    }

    #[test]
    fn grows_when_shelves_do_not_fit() {
        let mut state = setup();
        save(&mut state, &mut Sync::default()).unwrap();
        let small = state.vault.unwrap().size;
        let template = state.traps[0].clone();
        for i in 0..40 {
            let mut t = template.clone();
            t.button_pos = [16 + 8 * i, 8, 512];
            t.geom.as_mut().unwrap().shelf = None;
            state.traps.push(t);
        }
        save(&mut state, &mut Sync::default()).unwrap();
        assert!(state.vault.unwrap().size > small);
        let (_, loaded) = reload(&state);
        assert!(loaded.warnings.is_empty(), "{:?}", loaded.warnings);
        assert_eq!(loaded.traps.len(), 41);
        assert!(loaded.traps.iter().all(|t| t.geom.is_some()));
    }

    #[test]
    fn missing_base_entity_is_unlinked_on_load() {
        let mut state = setup();
        save(&mut state, &mut Sync::default()).unwrap();
        state.map.as_mut().unwrap().entities[0].ent_type = EntityType::Empty;
        let (_, loaded) = reload(&state);
        assert!(loaded.traps[0].ents.is_empty());
        assert_eq!(loaded.warnings.len(), 1);
    }
}
