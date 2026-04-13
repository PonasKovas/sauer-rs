//! Parser and writer for the Cube 2: Sauerbraten `.ogz` map format (version 33).
//!
//! This crate is IO-agnostic: it operates on plain byte slices.  GZIP
//! decompression is the caller's responsibility (e.g. `flate2`, `async-compression`).
//!
//! # Entry points
//!
//! ```ignore
//! // Parse a decompressed .ogz buffer:
//! let map = ogz_rs::parse(&decompressed_bytes)?;
//!
//! // Serialise back to bytes (ready for GZIP compression):
//! let bytes = ogz_rs::write(&map)?;
//! ```

mod error;
mod io;
pub mod types;

pub use error::OgzError;
pub use types::*;

pub use io::VERSION;
use io::{Reader, Writer};

/// Parse a fully decompressed `.ogz` byte slice into a [`Map`].
pub fn parse(data: &[u8]) -> Result<Map, OgzError> {
    let mut r = Reader::new(data);

    let (world_size, num_vars, num_vslots, num_ents, num_pvs, num_lightmaps, blendmap_flag, version) =
        io::read_header(&mut r)?;

    if world_size <= 0 || (world_size & (world_size - 1)) != 0 {
        return Err(OgzError::InvalidWorldSize(world_size));
    }

    let variables   = io::read_variables(&mut r, num_vars)?;
    let game_type   = io::read_game_type(&mut r, version)?;
    let entities    = io::read_entities(&mut r, num_ents, game_type.extra_entity_info_size, version)?;
    let vslots      = io::read_vslots(&mut r, num_vslots, version)?;
    let octree_root = io::read_octree_root(&mut r, version)?;
    let lightmaps   = io::read_lightmaps(&mut r, num_lightmaps, version)?;
    let pvs         = io::read_pvs(&mut r, num_pvs)?;
    let blendmap    = io::read_blendmap(&mut r, blendmap_flag, version)?;

    Ok(Map { world_size, variables, game_type, entities, vslots, octree_root, lightmaps, pvs, blendmap })
}

/// Serialise a [`Map`] into a decompressed `.ogz` byte vector.
///
/// Returns an error if the map is structurally invalid (see [`OgzError`] for
/// the full list).  The returned bytes are ready to be passed to a GZIP
/// compressor.
pub fn write(map: &Map) -> Result<Vec<u8>, OgzError> {
    validate(map)?;
    let mut w = Writer::new();
    io::write_header(&mut w, map);
    io::write_variables(&mut w, &map.variables);
    io::write_game_type(&mut w, &map.game_type);
    io::write_entities(&mut w, &map.entities);
    io::write_vslots(&mut w, &map.vslots);
    io::write_octree_root(&mut w, &map.octree_root);
    io::write_lightmaps(&mut w, &map.lightmaps);
    io::write_pvs(&mut w, &map.pvs);
    io::write_blendmap(&mut w, &map.blendmap);
    Ok(w.into_vec())
}

/// Validate a [`Map`] for write-time correctness.
///
/// Checks that all values fit in their on-disk representation and that internal
/// consistency constraints (e.g. entity payload lengths) are satisfied.
fn validate(map: &Map) -> Result<(), OgzError> {
    // world_size must be a positive power of 2.
    if map.world_size <= 0 || (map.world_size & (map.world_size - 1)) != 0 {
        return Err(OgzError::InvalidWorldSize(map.world_size));
    }

    // Top-level counts must fit in i32 (the on-disk type).
    check_count("entities", map.entities.len())?;
    check_count("variables", map.variables.len())?;
    check_count("vslots", map.vslots.len())?;
    check_count("lightmaps", map.lightmaps.len())?;
    if let Some(pvs) = &map.pvs {
        check_count("pvs_blocks", pvs.block_lengths.len())?;
    }

    // game_type string is stored with a u8 length prefix (max 255 bytes).
    let gt_len = map.game_type.game_type.as_bytes().len();
    if gt_len > u8::MAX as usize {
        return Err(OgzError::StringOverflow {
            field: "game_type",
            length: gt_len,
            bits: 8,
        });
    }

    // extra_game_data length is stored as u16 (max 65535 bytes).
    let egd_len = map.game_type.extra_game_data.len();
    if egd_len > u16::MAX as usize {
        return Err(OgzError::StringOverflow {
            field: "extra_game_data",
            length: egd_len,
            bits: 16,
        });
    }

    // texture_mru count is stored as u16.
    let mru_len = map.game_type.texture_mru.len();
    if mru_len > u16::MAX as usize {
        return Err(OgzError::CountOverflow { field: "texture_mru", count: mru_len });
    }

    // Every entity's extra_payload length must equal extra_entity_info_size.
    let expected = map.game_type.extra_entity_info_size as usize;
    for (i, e) in map.entities.iter().enumerate() {
        if e.extra_payload.len() != expected {
            return Err(OgzError::EntityPayloadLengthMismatch {
                index: i,
                expected,
                found: e.extra_payload.len(),
            });
        }
    }

    // Variable names and string values must fit in their u16 length prefix.
    for var in &map.variables {
        check_string_u16("variable_name", var.name.as_bytes())?;
        if let MapVariableValue::String(s) = &var.value {
            check_string_u16("variable_string_value", s.as_bytes())?;
        }
    }

    // Shader param names must fit in their u16 length prefix.
    for slot in &map.vslots {
        if let VSlot::Data(data) = slot {
            for param in &data.shader_params {
                check_string_u16("shader_param_name", param.name.as_bytes())?;
            }
        }
    }

    Ok(())
}

/// Returns `CountOverflow` if `count` does not fit in an `i32`.
#[inline]
fn check_count(field: &'static str, count: usize) -> Result<(), OgzError> {
    if count > i32::MAX as usize {
        return Err(OgzError::CountOverflow { field, count });
    }
    Ok(())
}

/// Returns `StringOverflow` if the byte slice length does not fit in a `u16`.
#[inline]
fn check_string_u16(field: &'static str, s: &[u8]) -> Result<(), OgzError> {
    if s.len() > u16::MAX as usize {
        return Err(OgzError::StringOverflow { field, length: s.len(), bits: 16 });
    }
    Ok(())
}
