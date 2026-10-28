//! Entity changes a trap makes when it fires.
//!
//! Entities are matched by content, never by index: a client save drops empty
//! entities, so indices shift. Positions are compared with a 1/16 tolerance
//! because `N_EDITENT` truncates them to 1/16 units while the editing client
//! keeps (and saves) the exact float.

use ogz_rs::data::{Entity, EntityType};
use serde::{Deserialize, Serialize};
use std::collections::HashSet;

const POS_TOLERANCE: f32 = 1.0 / 16.0 + 1e-3;

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct EntRec {
    pub ty: u8,
    pub pos: [f32; 3],
    pub attrs: [i16; 5],
}

impl EntRec {
    pub fn of(e: &Entity) -> Self {
        Self {
            ty: i32::from(e.ent_type) as u8,
            pos: e.position,
            attrs: e.attrs,
        }
    }

    pub fn to_entity(&self) -> Entity {
        Entity {
            position: self.pos,
            attrs: self.attrs,
            ent_type: EntityType::try_from(self.ty as i32).unwrap_or(EntityType::Empty),
            extra_data: Vec::new(),
        }
    }

    pub fn matches(&self, e: &Entity) -> bool {
        e.ent_type != EntityType::Empty
            && i32::from(e.ent_type) as u8 == self.ty
            && e.attrs == self.attrs
            && (0..3).all(|i| (e.position[i] - self.pos[i]).abs() <= POS_TOLERANCE)
    }
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub enum EntChange {
    /// Appears when the trap fires.
    Added(EntRec),
    /// A base-state entity that disappears when the trap fires.
    Removed(EntRec),
    /// A base-state entity that is moved or altered when the trap fires.
    Changed { from: EntRec, to: EntRec },
}

impl EntChange {
    /// The base-state entity this change depends on, if any.
    pub fn base(&self) -> Option<&EntRec> {
        match self {
            EntChange::Added(_) => None,
            EntChange::Removed(r) => Some(r),
            EntChange::Changed { from, .. } => Some(from),
        }
    }

    /// Where the entity is while the trap is active, if it exists then.
    pub fn active(&self) -> Option<&EntRec> {
        match self {
            EntChange::Added(r) => Some(r),
            EntChange::Removed(_) => None,
            EntChange::Changed { to, .. } => Some(to),
        }
    }
}

fn empty() -> Entity {
    Entity {
        position: [0.0; 3],
        attrs: [0; 5],
        ent_type: EntityType::Empty,
        extra_data: Vec::new(),
    }
}

/// Compares two entity lists slot by slot. `skip` holds virtual entity slots.
pub fn diff(original: &[Entity], current: &[Entity], skip: &[usize]) -> Vec<EntChange> {
    let blank = empty();
    let mut out = Vec::new();
    for i in 0..original.len().max(current.len()) {
        if skip.contains(&i) {
            continue;
        }
        let o = original.get(i).unwrap_or(&blank);
        let c = current.get(i).unwrap_or(&blank);
        let (o_empty, c_empty) = (
            o.ent_type == EntityType::Empty,
            c.ent_type == EntityType::Empty,
        );
        let (or, cr) = (EntRec::of(o), EntRec::of(c));
        match (o_empty, c_empty) {
            (true, true) => {}
            (true, false) => out.push(EntChange::Added(cr)),
            (false, true) => out.push(EntChange::Removed(or)),
            (false, false) if or != cr => out.push(EntChange::Changed { from: or, to: cr }),
            _ => {}
        }
    }
    out
}

fn find(entities: &[Entity], rec: &EntRec, used: &HashSet<usize>, skip: &[usize]) -> Option<usize> {
    (0..entities.len())
        .find(|i| !used.contains(i) && !skip.contains(i) && rec.matches(&entities[*i]))
}

/// Applies a trap's changes to a base-state entity list. Returns the touched slots.
/// Changes whose base entity is missing are skipped.
pub fn apply(entities: &mut Vec<Entity>, changes: &[EntChange], skip: &[usize]) -> Vec<usize> {
    let mut used = HashSet::new();
    for change in changes {
        let Some(base) = change.base() else { continue };
        if let Some(i) = find(entities, base, &used, skip) {
            entities[i] = change.active().map_or_else(empty, EntRec::to_entity);
            used.insert(i);
        }
    }
    for change in changes {
        let EntChange::Added(rec) = change else {
            continue;
        };
        let slot = (0..entities.len()).find(|i| {
            !skip.contains(i) && !used.contains(i) && entities[*i].ent_type == EntityType::Empty
        });
        let i = slot.unwrap_or_else(|| {
            entities.push(empty());
            entities.len() - 1
        });
        entities[i] = rec.to_entity();
        used.insert(i);
    }
    let mut touched: Vec<usize> = used.into_iter().collect();
    touched.sort_unstable();
    touched
}

/// Removes every change that depends on `old` (a base entity that was just
/// edited or deleted). Returns the dropped base records.
pub fn unlink(changes: &mut Vec<EntChange>, old: &Entity) -> Vec<EntRec> {
    let mut dropped = Vec::new();
    changes.retain(|c| match c.base() {
        Some(base) if base.matches(old) => {
            dropped.push(base.clone());
            false
        }
        _ => true,
    });
    dropped
}

/// Removes changes whose base entity no longer exists in `entities`.
pub fn unlink_missing(
    changes: &mut Vec<EntChange>,
    entities: &[Entity],
    skip: &[usize],
) -> Vec<EntRec> {
    let mut used = HashSet::new();
    let mut dropped = Vec::new();
    changes.retain(|c| match c.base() {
        None => true,
        Some(base) => match find(entities, base, &used, skip) {
            Some(i) => {
                used.insert(i);
                true
            }
            None => {
                dropped.push(base.clone());
                false
            }
        },
    });
    dropped
}

#[cfg(test)]
mod tests {
    use super::*;

    fn ent(ty: EntityType, pos: [f32; 3], a: i16) -> Entity {
        Entity {
            position: pos,
            attrs: [a, 0, 0, 0, 0],
            ent_type: ty,
            extra_data: Vec::new(),
        }
    }

    #[test]
    fn diff_then_apply_reproduces_active_state() {
        let base = vec![
            ent(EntityType::Light, [10.0, 10.0, 10.0], 1),
            ent(EntityType::Light, [20.0, 20.0, 20.0], 2),
            ent(EntityType::Empty, [0.0; 3], 0),
            ent(EntityType::Light, [30.0, 30.0, 30.0], 3),
        ];
        let mut active = base.clone();
        active[0].ent_type = EntityType::Empty; // deleted
        active[1].position = [25.0, 20.0, 20.0]; // moved
        active[2] = ent(EntityType::Light, [5.0, 5.0, 5.0], 9); // created

        let changes = diff(&base, &active, &[]);
        assert_eq!(changes.len(), 3);

        // Indices shift after a client save drops the empty slot.
        let mut saved: Vec<Entity> = base
            .iter()
            .filter(|e| e.ent_type != EntityType::Empty)
            .cloned()
            .collect();
        apply(&mut saved, &changes, &[]);
        let live: Vec<EntRec> = saved
            .iter()
            .filter(|e| e.ent_type != EntityType::Empty)
            .map(EntRec::of)
            .collect();
        let want: Vec<EntRec> = active
            .iter()
            .filter(|e| e.ent_type != EntityType::Empty)
            .map(EntRec::of)
            .collect();
        for w in &want {
            assert!(live.contains(w), "missing {w:?}");
        }
        assert_eq!(live.len(), want.len());
    }

    #[test]
    fn tolerant_position_match() {
        let rec = EntRec::of(&ent(EntityType::Light, [10.03, 4.0, 4.0], 1));
        assert!(rec.matches(&ent(EntityType::Light, [10.0, 4.0, 4.0], 1)));
        assert!(!rec.matches(&ent(EntityType::Light, [10.2, 4.0, 4.0], 1)));
        assert!(!rec.matches(&ent(EntityType::Light, [10.0, 4.0, 4.0], 2)));
    }

    #[test]
    fn unlink_drops_both_halves_of_a_move() {
        let a = ent(EntityType::Light, [1.0; 3], 1);
        let mut changes = vec![
            EntChange::Changed {
                from: EntRec::of(&a),
                to: EntRec::of(&ent(EntityType::Light, [9.0; 3], 1)),
            },
            EntChange::Added(EntRec::of(&ent(EntityType::Light, [3.0; 3], 4))),
        ];
        assert_eq!(unlink(&mut changes, &a).len(), 1);
        assert_eq!(changes.len(), 1);
        assert!(matches!(changes[0], EntChange::Added(_)));

        let mut changes = vec![EntChange::Removed(EntRec::of(&a))];
        assert_eq!(unlink_missing(&mut changes, &[], &[]).len(), 1);
        assert!(changes.is_empty());
    }
}
