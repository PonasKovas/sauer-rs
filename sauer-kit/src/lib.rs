//! Building blocks for custom servers on top of `sauer-server`, for things
//! the plain engine server doesn't do: per-player fake entities, keeping a
//! server-side copy of a coop-edited map, and pasting geometry to clients.

pub mod coop_edit;
pub mod paste;
pub mod virt_ents;
