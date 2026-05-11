mod edit;
mod geom;
pub mod map;
pub mod octree;
pub mod virt_ents;

pub mod chunking;
pub mod clipboard;
pub mod raycast;
pub mod vslot;
pub(crate) mod vslot_db;

pub mod db {
    //! Data storage subsystem built on top of the Map's VSlot structures.
    pub use crate::vslot_db::{DbEntry, FaceInstance, StoreError, StoreStats};
}

pub use edit::{apply_region, fill, set_material, set_texture};
pub use geom::subdivide;
pub use map::Map;
pub use sauer_core::{Cube, Node, Selection};
