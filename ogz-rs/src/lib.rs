//! `ogz-rs` is a strictly-typed parser and serializer for the Cube 2: Sauerbraten `.ogz` map format.
//! It seamlessly handles all map versions up to 33, resolving all engine quirks including the historic
//! broken GZIP trailer issue via lenient decoding.

pub mod data;
mod de;
mod error;
mod ser;

pub use error::OgzError;

use cube_string::CubeString;
use data::*;

/// The top-level representation of an `.ogz` map.
#[derive(Debug, Clone, PartialEq, PartialOrd)]
pub struct OgzMap {
    pub version: i32,
    pub world_size: i32,
    pub config: MapConfig,

    // Game Extras
    pub game_type: Option<CubeString>,
    pub eif: u16, // Extra entity info size
    pub extra_game_data: Vec<u8>,

    pub mru_textures: Vec<u16>,
    pub entities: Vec<Entity>,
    pub vslots: Vec<Option<VSlot>>,
    pub octree: OctreeNode,

    pub lightmaps: Vec<Lightmap>,
    pub pvs: Vec<u8>,

    pub blendmap_resolution: i32,
    pub blendmap_data: Option<Vec<u8>>,
}

impl OgzMap {
    /// Deserializes a map from a raw byte slice (the contents of the `.ogz` file).
    pub fn parse(data: &[u8]) -> Result<Self, crate::error::OgzError> {
        crate::de::parse_map(data)
    }

    /// Serializes the map to the modern Version 33 `.ogz` byte array.
    pub fn serialize(&self) -> Result<Vec<u8>, crate::error::OgzError> {
        crate::ser::serialize_map(self)
    }
}
