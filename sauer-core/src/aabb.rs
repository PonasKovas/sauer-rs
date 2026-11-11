/// A box of world space: `min` is inside it, `max` just past it.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
pub struct Aabb {
    pub min: [i32; 3],
    pub max: [i32; 3],
}

impl Aabb {
    pub const fn new(min: [i32; 3], max: [i32; 3]) -> Self {
        Self { min, max }
    }

    /// The box `extent` big with its corner at `origin`.
    pub fn sized(origin: [i32; 3], extent: [i32; 3]) -> Self {
        Self::new(origin, std::array::from_fn(|i| origin[i] + extent[i]))
    }

    /// The cube `size` big with its corner at `origin`.
    pub fn cube(origin: [i32; 3], size: i32) -> Self {
        Self::sized(origin, [size; 3])
    }

    pub fn extent(&self) -> [i32; 3] {
        std::array::from_fn(|i| self.max[i] - self.min[i])
    }

    pub fn is_empty(&self) -> bool {
        (0..3).any(|i| self.min[i] >= self.max[i])
    }

    /// Whether the two share some volume (touching faces don't count).
    pub fn overlaps(&self, other: &Aabb) -> bool {
        (0..3).all(|i| self.min[i] < other.max[i] && other.min[i] < self.max[i])
    }

    /// Whether `other` lies entirely inside this box.
    pub fn contains(&self, other: &Aabb) -> bool {
        (0..3).all(|i| self.min[i] <= other.min[i] && other.max[i] <= self.max[i])
    }

    /// Whether `p` is inside, with the box grown by `margin` on every side.
    pub fn contains_point(&self, p: [f32; 3], margin: f32) -> bool {
        (0..3).all(|i| p[i] >= self.min[i] as f32 - margin && p[i] < self.max[i] as f32 + margin)
    }

    /// The part both boxes share, if any.
    pub fn intersection(&self, other: &Aabb) -> Option<Aabb> {
        let b = Aabb::new(
            std::array::from_fn(|i| self.min[i].max(other.min[i])),
            std::array::from_fn(|i| self.max[i].min(other.max[i])),
        );
        (!b.is_empty()).then_some(b)
    }

    /// The smallest box holding both.
    pub fn union(&self, other: &Aabb) -> Aabb {
        Aabb::new(
            std::array::from_fn(|i| self.min[i].min(other.min[i])),
            std::array::from_fn(|i| self.max[i].max(other.max[i])),
        )
    }

    /// The smallest box holding all of `boxes`, `None` for none.
    pub fn enclosing(boxes: impl IntoIterator<Item = Aabb>) -> Option<Aabb> {
        boxes.into_iter().reduce(|a, b| a.union(&b))
    }
}
