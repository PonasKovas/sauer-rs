use crate::{Map, Node};
use ogz_rs::data::{VSlot, VSlotProps};
use sauer_proto::data::ClipboardPayload;
use sauer_proto::data::VSlotData;

pub(crate) fn extract_vslots(nodes: &[Node], vslots: &[Option<VSlot>]) -> Vec<VSlotData> {
    let mut used = vec![false; vslots.len().max(65536)];
    fn collect(node: &Node, used: &mut Vec<bool>) {
        match node {
            Node::Leaf(cube) => {
                for &tex in &cube.textures {
                    used[tex as usize] = true;
                }
            }
            Node::Branch(children) => {
                for child in children.as_ref() {
                    collect(child, used);
                }
            }
        }
    }
    for node in nodes {
        collect(node, &mut used);
    }

    // Slots go by the engine's slot number, not our base index (see
    // `vslot::slot_of_root`). Plain textures are sent too where the two
    // differ: clients that compacted their vslots number bases by slot.
    let mut out = Vec::new();
    for idx in 1..used.len() {
        if !used[idx] {
            continue;
        }
        let vslot = match vslots.get(idx) {
            Some(Some(vs)) => vs.props.clone(),
            _ if crate::vslot::slot_of_root(vslots, idx as i32) == idx as i32 => continue,
            _ => VSlotProps::default(),
        };
        let root = crate::vslot::root_base_of(vslots, idx as u16);
        out.push(VSlotData {
            index: idx as u16,
            slot: crate::vslot::slot_of_root(vslots, root) as u16,
            vslot,
        });
    }
    out
}

pub fn generate_clipboard_for_region(
    map: &Map,
    padded_min: [i32; 3],
    s: [i32; 3],
    grid: i32,
) -> ClipboardPayload {
    let [sx, sy, sz] = s;
    let mut nodes = Vec::with_capacity((sx * sy * sz) as usize);
    for xi in 0..sx {
        for zi in 0..sz {
            for yi in 0..sy {
                let cell_pos = [
                    padded_min[0] + xi * grid,
                    padded_min[1] + yi * grid,
                    padded_min[2] + zi * grid,
                ];
                let node =
                    crate::octree::extract_node_at(&map.root, [0, 0, 0], map.size, cell_pos, grid);
                nodes.push(node);
            }
        }
    }

    let vs = extract_vslots(&nodes, &map.vslots);
    ClipboardPayload {
        sel: sauer_core::Selection {
            o: sauer_core::Vec3 {
                x: padded_min[0],
                y: padded_min[1],
                z: padded_min[2],
            },
            s: sauer_core::Vec3 {
                x: sx,
                y: sy,
                z: sz,
            },
            grid,
            orient: sauer_core::Orient::Left,
            cx: 0,
            cxs: 0,
            cy: 0,
            cys: 0,
            corner: 0,
        },
        nodes,
        vslots: vs,
    }
}

/// The grid-aligned box a region is copied as: the region clipped to the
/// world, padded out to a grid coarse enough that it spans at most 8 cubes
/// and doesn't split a shaped leaf. Returns `(origin, size in cubes, grid)`.
pub(crate) fn region_layout(map: &Map, min: [i32; 3], max: [i32; 3]) -> ([i32; 3], [i32; 3], i32) {
    let min = min.map(|v| v.clamp(0, map.size));
    let max: [i32; 3] = std::array::from_fn(|i| max[i].clamp(min[i], map.size));
    let max_span = (0..3).map(|i| max[i] - min[i]).max().unwrap_or(1).max(1);
    let mut grid = 1i32;
    while (max_span + grid - 1) / grid > 8 {
        grid <<= 1;
    }
    let shaped = crate::octree::max_shaped_leaf(&map.root, [0; 3], map.size, min, max);
    let grid = grid.max(shaped).min(map.size);

    let padded_min: [i32; 3] = std::array::from_fn(|i| min[i].div_euclid(grid) * grid);
    let padded_max: [i32; 3] =
        std::array::from_fn(|i| (((max[i] + grid - 1).div_euclid(grid)) * grid).min(map.size));
    let s = std::array::from_fn(|i| (padded_max[i] - padded_min[i]) / grid);
    (padded_min, s, grid)
}

pub fn generate_region_clipboard(
    map: &Map,
    min: [i32; 3],
    max: [i32; 3],
) -> ([i32; 3], [i32; 3], i32, ClipboardPayload) {
    let (o, s, grid) = region_layout(map, min, max);
    let payload = generate_clipboard_for_region(map, o, s, grid);
    (o, s, grid, payload)
}
