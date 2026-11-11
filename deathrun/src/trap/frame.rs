//! Box outlines drawn with particle lines, as fake entities.

use crate::aim::Aim;
use sauer_core::Aabb;
use sauer_kit::virt_ents::FakeEnt;
use sauer_proto::data::EntityType;

/// One particle line: from `(x, y, z)` along `dir` (the particle entity's
/// direction attribute) for `len` units.
pub type Edge = (i32, i32, i32, i32, i32);

/// Corners closer than this to the view ray get no particle on them, which
/// would blind the player.
const LOOK_RADIUS: f32 = 4.0;

/// The 12 edges of `b`. Lines ending at a corner the player looks at start
/// from the other end instead.
pub fn box_edges(b: Aabb, eye: Option<&Aim>) -> Vec<Edge> {
    let Aabb { min, max } = b;
    let corner = |i: usize| -> [i32; 3] {
        std::array::from_fn(|a| if (i >> a) & 1 == 1 { max[a] } else { min[a] })
    };
    let hot: [bool; 8] = std::array::from_fn(|i| {
        eye.is_some_and(|e| e.passes_near(corner(i).map(|v| v as f32), LOOK_RADIUS))
    });
    let mut out = Vec::with_capacity(12);
    // (axis bit in the corner index, particle dir)
    for (bit, dir) in [(0usize, 1), (1, 2), (2, 0)] {
        let len = max[bit] - min[bit];
        for i in (0..8).filter(|i| (i >> bit) & 1 == 0) {
            let j = i | (1 << bit);
            let (from, dir) = match (hot[i], hot[j]) {
                (true, true) => continue,
                (true, false) => (corner(j), dir + 3),
                _ => (corner(i), dir),
            };
            out.push((from[0], from[1], from[2], dir, len));
        }
    }
    out
}

/// The particle lines outlining `b` in `color` (0xRGB, 4 bits each).
pub fn outline(b: Aabb, eye: Option<&Aim>, color: i32) -> Vec<FakeEnt> {
    lines(box_edges(b, eye), color)
}

pub fn lines(edges: Vec<Edge>, color: i32) -> Vec<FakeEnt> {
    edges
        .into_iter()
        .map(|(x, y, z, dir, len)| FakeEnt {
            ent_type: EntityType::Particles,
            position: [x as f32, y as f32, z as f32],
            attrs: [7, dir, len, color, 0],
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn no_particle_on_a_looked_at_corner() {
        let b = Aabb::cube([0; 3], 16);
        assert_eq!(box_edges(b, None).len(), 12);

        // Looking straight at the max corner from far away along the diagonal.
        let f = 1.0 / 3f32.sqrt();
        let eye = Aim {
            origin: [116.0; 3],
            forward: [-f; 3],
        };
        let edges = box_edges(b, Some(&eye));
        assert_eq!(edges.len(), 12);
        assert!(edges.iter().all(|e| [e.0, e.1, e.2] != b.max));
        // The three edges ending at that corner now start at their other end.
        assert_eq!(edges.iter().filter(|e| e.3 >= 3).count(), 3);
    }
}
