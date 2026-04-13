//! `ogz-rs` is a strictly-typed parser and serializer for the Cube 2: Sauerbraten `.ogz` map format.
//! It seamlessly handles all map versions up to 33, resolving all engine quirks including the historic
//! broken GZIP trailer issue via lenient decoding.

pub mod data;
pub mod de;
pub mod error;
pub mod ser;

pub use data::OgzMap;
pub use error::OgzError;

// Re-export common types
pub use data::{CubeInfo, OctreeNode, Surfaces};
pub use data::{CubeString, Entity, VSlot, VSlotParam, Variable};

#[cfg(test)]
mod tests {
    // Standard test stubs could reside here
}
