//! Where a player looks: their eye and view ray, and what it hits.

use sauer_map::Map;
use sauer_proto::data::{Direction, Vec3};

/// How far above a player's position their eye is.
pub const EYE_HEIGHT: f32 = 14.0;
/// How far aiming reaches.
pub const REACH: f32 = 2048.0;

/// A player's eye and the unit vector they look along.
#[derive(Clone, Copy, Debug)]
pub struct Aim {
    pub origin: [f32; 3],
    pub forward: [f32; 3],
}

/// The basis of a view: `forward`, plus `right` and `up` across it.
pub fn view_basis(yaw: f32, pitch: f32) -> [[f32; 3]; 3] {
    let (y, p) = (yaw.to_radians(), pitch.to_radians());
    [
        forward(yaw, pitch),
        [y.cos(), y.sin(), 0.0],
        [y.sin() * p.sin(), -y.cos() * p.sin(), p.cos()],
    ]
}

impl Aim {
    pub fn new(pos: Vec3<f32>, dir: Direction) -> Self {
        Self {
            origin: [pos.x, pos.y, pos.z + EYE_HEIGHT],
            forward: forward(dir.yaw as f32, dir.pitch as f32),
        }
    }

    /// The point `dist` along the ray.
    pub fn at(&self, dist: f32) -> [f32; 3] {
        std::array::from_fn(|i| self.origin[i] + self.forward[i] * dist)
    }

    /// The first geometry the ray hits within [`REACH`] (glass counts).
    pub fn hit<'a>(&self, map: &'a Map) -> Option<sauer_map::raycast::RayHit<'a>> {
        sauer_map::raycast::raycast_with_filter(map, self.origin, self.forward, REACH, |c| {
            !c.is_empty() || c.material.volume == sauer_core::Volume::Glass
        })
    }

    /// Where the ray first hits geometry, or [`REACH`] away.
    pub fn hit_point(&self, map: &Map) -> [f32; 3] {
        self.at(self.hit(map).map_or(REACH, |hit| hit.dist))
    }

    /// Whether `p` is within `radius` of the ray, in front of the eye.
    pub fn passes_near(&self, p: [f32; 3], radius: f32) -> bool {
        self.distance_sq(p).is_some_and(|d| d < radius * radius)
    }

    /// Squared distance from `p` to the ray, `None` if `p` is behind the eye.
    pub fn distance_sq(&self, p: [f32; 3]) -> Option<f32> {
        let v: [f32; 3] = std::array::from_fn(|i| p[i] - self.origin[i]);
        let t: f32 = (0..3).map(|i| v[i] * self.forward[i]).sum();
        (t > 0.0).then(|| (0..3).map(|i| (v[i] - t * self.forward[i]).powi(2)).sum())
    }
}

/// The unit vector for a view of `yaw` and `pitch` degrees, as the engine has it.
pub fn forward(yaw: f32, pitch: f32) -> [f32; 3] {
    let (yaw, pitch) = (yaw.to_radians(), pitch.to_radians());
    [
        -yaw.sin() * pitch.cos(),
        yaw.cos() * pitch.cos(),
        pitch.sin(),
    ]
}
