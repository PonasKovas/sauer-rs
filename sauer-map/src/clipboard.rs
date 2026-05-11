use crate::{Map, Node};
use ogz_rs::data::VSlot;
use sauer_proto::data::{ClipboardPayload};use sauer_proto::data::clipboard::VSlotData;

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

    let mut out = Vec::new();
    for idx in 0..used.len() {
        if used[idx] {
            if let Some(Some(vs)) = vslots.get(idx) {
                let slot = crate::vslot::root_base_of(vslots, idx as u16);
                out.push(VSlotData {
                    index: idx as u16,
                    slot: slot as u16,
                    vslot: vs.clone(),
                });
            }
        }
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
                let node = crate::octree::extract_node_at(&map.root, [0, 0, 0], map.size, cell_pos, grid);
                nodes.push(node);
            }
        }
    }
    
    let vs = extract_vslots(&nodes, &map.vslots);
    ClipboardPayload {
        unpack_len: 0,
        sel: sauer_core::Selection {
            o: sauer_core::Vec3 { x: padded_min[0], y: padded_min[1], z: padded_min[2] },
            s: sauer_core::Vec3 { x: sx, y: sy, z: sz },
            grid,
            orient: sauer_core::Orient::Left,
            cx: 0, cxs: 0, cy: 0, cys: 0, corner: 0,
        },
        nodes,
        vslots: vs,
    }
}

pub fn generate_region_clipboard(
    map: &Map,
    min: [i32; 3],
    max: [i32; 3],
) -> ([i32; 3], [i32; 3], i32, ClipboardPayload) {
    let spans = [max[0] - min[0], max[1] - min[1], max[2] - min[2]];
    let max_span = spans.iter().copied().max().unwrap_or(1).max(1);
    let mut grid = 1i32;
    while (max_span + grid - 1) / grid > 8 {
        grid <<= 1;
    }

    let padded_min: [i32; 3] = std::array::from_fn(|i| min[i].div_euclid(grid) * grid);
    let padded_max: [i32; 3] =
        std::array::from_fn(|i| (((max[i] + grid - 1).div_euclid(grid)) * grid).min(map.size));
    let sx = (padded_max[0] - padded_min[0]) / grid;
    let sy = (padded_max[1] - padded_min[1]) / grid;
    let sz = (padded_max[2] - padded_min[2]) / grid;
    let s = [sx, sy, sz];

    let payload = generate_clipboard_for_region(map, padded_min, s, grid);
    (padded_min, s, grid, payload)
}
