use crate::map_state::Trap;
use sauer_proto::data::SelInfo;

pub fn intersects_bounds(min: [i32; 3], max: [i32; 3], trap: &Trap) -> bool {
    let t_min = trap.button_pos;
    let t_max = [
        trap.button_pos[0] + trap.button_size,
        trap.button_pos[1] + trap.button_size,
        trap.button_pos[2] + trap.button_size,
    ];
    (0..3).all(|i| min[i] < t_max[i] && max[i] > t_min[i])
}

pub fn intersects_sel(sel: &SelInfo, trap: &Trap) -> bool {
    let min = [sel.o.x, sel.o.y, sel.o.z];
    let max = [
        min[0] + sel.s.x * sel.grid,
        min[1] + sel.s.y * sel.grid,
        min[2] + sel.s.z * sel.grid,
    ];
    intersects_bounds(min, max, trap)
}

pub fn intersects_edit_face(sel: &SelInfo, dir: i32, trap: &Trap) -> bool {
    let mut min = [sel.o.x, sel.o.y, sel.o.z];
    let mut max = [
        min[0] + sel.s.x * sel.grid,
        min[1] + sel.s.y * sel.grid,
        min[2] + sel.s.z * sel.grid,
    ];

    let d = (sel.orient as usize) >> 1;
    let expansion = dir.abs() * sel.grid;
    min[d] -= expansion;
    max[d] += expansion;

    intersects_bounds(min, max, trap)
}
