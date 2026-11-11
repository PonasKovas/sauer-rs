//! The vault: a sealed cube of rock at the far bottom corner of the world
//! (`x = y = world - size`, `z = 0`) holding each trap's active-state geometry
//! on "shelves", plus the anchor cube whose texture carries the trap record.
//!
//! Layout of a vault of `size` (a power of two, aligned to itself):
//! - everything is solid rock except a cavity of `size / 2` centred inside,
//!   aligned to `size / 4` so the walls stay a few big octree cubes;
//! - shelves float in the cavity with at least one unit of air around them,
//!   so all their outer faces count as visible and `remip` keeps them intact;
//! - the anchor is a 1-cube at the cavity's first free corner, all six faces
//!   textured with the record vslot.

use super::{Trap, TrapGeom};
use sauer_map::{Map, Selection, ValidSel};
use sauer_proto::data::{ClipboardPayload, Orient, VSlotProps, Vec3};

pub const MIN_SIZE: i32 = 64;
pub const MAX_SIZE: i32 = 1 << 15;
/// `DEFAULT_GEOM`, a texture slot every map has. The record vslot is a variant of it.
pub const ANCHOR_SLOT: u16 = 1;

use sauer_core::Aabb;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Vault {
    pub origin: [i32; 3],
    pub size: i32,
}

impl Vault {
    pub fn at_far_corner(world: i32, size: i32) -> Self {
        Self {
            origin: [world - size, world - size, 0],
            size,
        }
    }

    pub fn is_at_far_corner(&self, world: i32) -> bool {
        *self == Self::at_far_corner(world, self.size)
    }

    pub fn bounds(&self) -> Aabb {
        Aabb::cube(self.origin, self.size)
    }

    fn abs(&self, rel: [i32; 3]) -> [i32; 3] {
        std::array::from_fn(|i| self.origin[i] + rel[i])
    }

    pub fn anchor_box(&self) -> Aabb {
        Aabb::cube(self.abs(anchor_rel(self.size)), 1)
    }

    pub fn shelf_box(&self, geom: &TrapGeom) -> Option<Aabb> {
        Some(Aabb::sized(self.abs(geom.shelf?), geom.extent()))
    }
}

fn cavity_rel(size: i32) -> Aabb {
    Aabb::new([size / 4; 3], [size * 3 / 4; 3])
}

fn anchor_rel(size: i32) -> [i32; 3] {
    [size / 4 + 1; 3]
}

// ─── Packing ─────────────────────────────────────────────────────────────────

pub struct Item {
    pub extent: [i32; 3],
    pub grid: i32,
}

impl Item {
    pub fn of(geom: &TrapGeom) -> Self {
        Self {
            extent: geom.extent(),
            grid: geom.clip_grid,
        }
    }
}

/// At least one unit of air between the two boxes on some axis.
fn separated(a: &Aabb, b: &Aabb) -> bool {
    (0..3).any(|i| a.max[i] < b.min[i] || b.max[i] < a.min[i])
}

/// Places `items` (in order) into the cavity of a vault of `size`, around the
/// already occupied relative boxes in `taken`. Returns relative positions.
pub fn pack(size: i32, taken: &[Aabb], items: &[Item]) -> Option<Vec<[i32; 3]>> {
    let Aabb {
        min: cmin,
        max: cmax,
    } = cavity_rel(size);
    let lo = cmin.map(|v| v + 1);
    let hi = cmax.map(|v| v - 1);

    let mut placed: Vec<Aabb> = taken.to_vec();
    let anchor = anchor_rel(size);
    placed.push(Aabb::cube(anchor, 1));

    let mut out = Vec::with_capacity(items.len());
    for item in items {
        if item.grid > size / 4 {
            return None;
        }
        let mut candidates = vec![lo];
        for Aabb {
            min: bmin,
            max: bmax,
        } in &placed
        {
            for axis in 0..3 {
                let mut p = *bmin;
                p[axis] = bmax[axis] + 1;
                candidates.push(p);
                for reset in 0..3 {
                    if reset != axis {
                        let mut q = p;
                        q[reset] = lo[reset];
                        candidates.push(q);
                    }
                }
            }
        }
        candidates.sort_by_key(|p| (p[2], p[1], p[0]));

        let fit = candidates.into_iter().find_map(|c| {
            let p: [i32; 3] = std::array::from_fn(|i| {
                let v = c[i].max(lo[i]);
                (v + item.grid - 1).div_euclid(item.grid) * item.grid
            });
            let b = Aabb::sized(p, item.extent);
            let inside = (0..3).all(|i| b.max[i] <= hi[i]);
            (inside && placed.iter().all(|q| separated(&b, q))).then_some(b)
        })?;
        placed.push(fit);
        out.push(fit.min);
    }
    Some(out)
}

/// Smallest vault size (from `start`) that fits all `items` from scratch.
pub fn pack_fresh(start: i32, items: &[Item]) -> Option<(i32, Vec<[i32; 3]>)> {
    let mut size = start.max(MIN_SIZE);
    while size <= MAX_SIZE {
        if let Some(pos) = pack(size, &[], items) {
            return Some((size, pos));
        }
        size *= 2;
    }
    None
}

// ─── Map operations ──────────────────────────────────────────────────────────

/// A box the plugin builds itself, checked against the map. Those always
/// fit the world, so a `None` is a bug: debug builds stop, release builds
/// skip the edit.
pub fn sel(map: &Map, o: [i32; 3], s: [i32; 3], grid: i32) -> Option<ValidSel> {
    let valid = Selection::new(o, s, grid, Orient::Left).validate(map.size);
    debug_assert!(
        valid.is_some(),
        "selection {o:?} {s:?} {grid} doesn't fit the map"
    );
    valid
}

fn fill_rock(map: &mut Map, origin: [i32; 3], size: i32) {
    let Some(s) = sel(map, origin, [1; 3], size) else {
        return;
    };
    map.fill(&s, true);
    let rock = sauer_map::vslot::root_of_slot(&map.vslots, ANCHOR_SLOT);
    map.set_texture(&s, rock, true);
}

/// Removes a vault, leaving plain air (what enlarging adds).
pub fn erase(map: &mut Map, vault: &Vault) {
    if let Some(s) = sel(map, vault.origin, [1; 3], vault.size) {
        map.fill(&s, false);
    }
}

fn clear(map: &mut Map, min: [i32; 3], extent: [i32; 3], grid: i32) {
    if let Some(s) = sel(map, min, extent.map(|e| e / grid), grid) {
        map.fill(&s, false);
    }
}

pub fn write_anchor(map: &mut Map, vault: &Vault, record: &VSlotProps) {
    let min = vault.anchor_box().min;
    let Some(s) = sel(map, min, [1; 3], 1) else {
        return;
    };
    map.fill(&s, true);
    map.assign_vslot(&s, true, ANCHOR_SLOT, record);
}

pub fn write_shelf(map: &mut Map, vault: &Vault, geom: &TrapGeom) {
    if let Some(Aabb { min, .. }) = vault.shelf_box(geom)
        && let Some(s) = sel(map, min, geom.clip_s, geom.clip_grid)
    {
        map.apply_block(&s, geom.payload.nodes.clone(), false);
    }
}

pub fn clear_shelf(map: &mut Map, vault: &Vault, geom: &TrapGeom) -> Option<Aabb> {
    let b = vault.shelf_box(geom)?;
    clear(map, b.min, geom.extent(), geom.clip_grid);
    Some(b)
}

/// Writes the whole vault from scratch: rock, cavity, anchor and every shelf.
pub fn build(map: &mut Map, vault: &Vault, traps: &[Trap], record: &VSlotProps) {
    fill_rock(map, vault.origin, vault.size);
    let cmin = cavity_rel(vault.size).min;
    clear(map, vault.abs(cmin), [vault.size / 2; 3], vault.size / 4);
    write_anchor(map, vault, record);
    for geom in traps.iter().filter_map(|t| t.geom.as_ref()) {
        write_shelf(map, vault, geom);
    }
}

/// Copies a shelf back out as a payload aimed at the trap's clip box.
pub fn read_shelf(map: &Map, vault: &Vault, geom: &TrapGeom) -> Option<ClipboardPayload> {
    let Aabb { min, max } = vault.shelf_box(geom)?;
    if (0..3).any(|i| min[i] < 0 || max[i] > map.size) {
        return None;
    }
    let mut payload =
        sauer_map::block::generate_clipboard_for_region(map, min, geom.clip_s, geom.clip_grid);
    payload.sel.o = Vec3 {
        x: geom.clip_pos[0],
        y: geom.clip_pos[1],
        z: geom.clip_pos[2],
    };
    Some(payload)
}

/// True when the box holds only untouched air or untouched rock (each part
/// split off around `ours` may be either), ignoring the part covered by
/// `ours` (the current vault, which we are free to overwrite).
pub fn is_free(map: &Map, b: Aabb, ours: Option<Aabb>) -> bool {
    if (0..3).any(|i| b.min[i] < 0 || b.max[i] > map.size) {
        return false;
    }
    match ours {
        Some(o) if (0..3).all(|i| o.min[i] <= b.min[i] && b.max[i] <= o.max[i]) => true,
        Some(o) if !separated(&b, &o) && (0..3).all(|i| b.max[i] > b.min[i] + 1) => {
            // Partially ours: split into octants (everything here is power-of-two aligned).
            let half: [i32; 3] = std::array::from_fn(|i| (b.max[i] - b.min[i]) / 2);
            (0..8).all(|k| {
                let min: [i32; 3] = std::array::from_fn(|i| b.min[i] + ((k >> i) & 1) * half[i]);
                is_free(map, Aabb::sized(min, half), ours)
            })
        }
        _ => map.is_untouched_region(b),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn item(e: i32, grid: i32) -> Item {
        Item {
            extent: [e; 3],
            grid,
        }
    }

    fn check(size: i32, items: &[Item], pos: &[[i32; 3]]) {
        let Aabb {
            min: cmin,
            max: cmax,
        } = cavity_rel(size);
        let a = anchor_rel(size);
        let mut boxes: Vec<Aabb> = vec![Aabb::cube(a, 1)];
        for (it, p) in items.iter().zip(pos) {
            let b = Aabb::sized(*p, it.extent);
            assert!((0..3).all(|i| p[i] % it.grid == 0), "unaligned {p:?}");
            assert!(
                (0..3).all(|i| b.min[i] > cmin[i] && b.max[i] < cmax[i]),
                "outside cavity {b:?}"
            );
            assert!(boxes.iter().all(|q| separated(&b, q)), "touching {b:?}");
            boxes.push(b);
        }
    }

    #[test]
    fn packs_separated_aligned_boxes() {
        let items = vec![
            item(32, 16),
            item(8, 8),
            item(5, 1),
            item(16, 2),
            item(8, 8),
        ];
        let (size, pos) = pack_fresh(MIN_SIZE, &items).unwrap();
        check(size, &items, &pos);
    }

    #[test]
    fn grows_when_needed() {
        let items: Vec<Item> = (0..20).map(|_| item(16, 8)).collect();
        let (size, pos) = pack_fresh(MIN_SIZE, &items).unwrap();
        assert!(size > MIN_SIZE);
        check(size, &items, &pos);
    }

    #[test]
    fn incremental_respects_taken() {
        let size = 128;
        let first = pack(size, &[], &[item(16, 8)]).unwrap();
        let taken = vec![Aabb::cube(first[0], 16)];
        let second = pack(size, &taken, &[item(16, 8)]).unwrap();
        assert!(separated(&taken[0], &Aabb::cube(second[0], 16)));
    }

    #[test]
    fn vault_survives_build_and_read() {
        let mut map = Map::new_empty(2048);
        let vault = Vault::at_far_corner(2048, 128);
        assert!(is_free(&map, vault.bounds(), None));

        let mut src = Map::new_empty(2048);
        src.fill(&sel(&src, [100, 100, 600], [1; 3], 4).unwrap(), true);
        let payload =
            sauer_map::block::generate_clipboard_for_region(&src, [96, 96, 600], [2; 3], 8);
        let shelf = pack(
            vault.size,
            &[],
            &[Item {
                extent: [16; 3],
                grid: 8,
            }],
        )
        .unwrap()[0];
        let geom = TrapGeom {
            clip_pos: [96, 96, 600],
            clip_s: [2; 3],
            clip_grid: 8,
            payload: payload.clone(),
            shelf: Some(shelf),
        };
        let mut trap = Trap::new([0; 3], 8, 0);
        trap.geom = Some(geom.clone());

        let record = crate::trap::param_blob::encode("t", 1, b"hello");
        build(&mut map, &vault, &[trap], &record);
        assert!(!is_free(&map, vault.bounds(), None));
        assert!(is_free(&map, vault.bounds(), Some(vault.bounds())));
        assert_eq!(
            read_shelf(&map, &vault, &geom).unwrap().nodes,
            payload.nodes
        );
        assert_eq!(
            crate::trap::param_blob::find_latest(&map, "t"),
            Some((1, b"hello".to_vec()))
        );

        erase(&mut map, &vault);
        assert!(map.is_untouched_region(vault.bounds()));
        assert!(
            map.get_cube(vault.origin[0] as f32, vault.origin[1] as f32, 0.0)
                .unwrap()
                .0
                .is_empty()
        );
    }

    #[test]
    fn free_means_untouched_air_or_rock() {
        let mut map = Map::new_empty(1024);
        map.enlarge();
        let air = Vault::at_far_corner(2048, 128).bounds();
        assert!(is_free(&map, air, None));
        // A mapper's build in the corner makes it unusable.
        map.fill(&sel(&map, [1984, 1984, 0], [1; 3], 16).unwrap(), true);
        assert!(!is_free(&map, air, None));
        // Untouched rock (the bottom of a fresh map) is fine too.
        assert!(is_free(
            &map,
            Aabb::new([896, 896, 0], [1024, 1024, 128]),
            None
        ));
    }
}
