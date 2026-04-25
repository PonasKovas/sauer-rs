//! Abstract entity-based data storage for Sauerbraten maps.
//!
//! Arbitrary `serde`-serializable types can be encoded (bitcode + zlib) and
//! stored inside map entities of type Particles (5) placed at sentinel positions.
//!
//! ## Entity layout
//!
//! ```text
//!   x = -(100 + version)   ← version identifier (version 0 → x=-100, version 2 → x=-102)
//!   y = -100               ← visual sentinel, not validated on read
//!   z = -100               ← visual sentinel, not validated on read
//! ```
//!
//! Coordinates are exact integers and survive the DMF×16 wire quantisation
//! without loss (−100 through −355 cover versions 0–255).
//!
//! ## Byte stream (packed into 5 × u16 attrs per entity, 10 bytes each)
//!
//! ```text
//!   bytes 0–1  : compressed payload length, u16 LE
//!   bytes 2..  : bitcode-serialised, zlib-compressed payload
//! ```
//!
//! No magic bytes are needed in the stream; entity identity is determined
//! solely by the sentinel coordinates.
//!
//! ## Error behaviour
//!
//! [`read`] returns `Err([StoreError::VersionMismatch])` when store entities are
//! present but carry a different version than requested, so callers can detect
//! stale or migrated data explicitly.

use std::io::{Read, Write};

use flate2::{Compression, read::ZlibDecoder, write::ZlibEncoder};
use ogz_rs::data::{Entity, EntityType};
use sauer_map::World;
use sauer_proto::{Client, channel1, data::Vec3};
use sauer_server::Server;
use serde::{Serialize, de::DeserializeOwned};
use thiserror::Error;
use tracing::info;

// ─── constants ───────────────────────────────────────────────────────────────

// Particles (5) in both ogz and protocol — a non-zero type that maps 1:1
// between ogz and the network protocol with no filtering or special handling.

const SENTINEL_Y: f32 = -100.0;
const SENTINEL_Z: f32 = -100.0;

#[inline]
fn sentinel_x(version: u8) -> f32 {
    -(100.0 + version as f32)
}

// ─── error type ──────────────────────────────────────────────────────────────

#[derive(Debug, Error)]
pub enum StoreError {
    #[error("payload too large: {0} bytes (max 65533)")]
    TooLarge(usize),

    #[error("serialise error: {0}")]
    Serialise(String),

    #[error("version mismatch: map contains version {found}, requested version {expected}")]
    VersionMismatch { expected: u8, found: u8 },

    #[error("corrupt data: {0}")]
    Corrupt(String),
}

// ─── encoding pipeline ───────────────────────────────────────────────────────

fn compress<T: Serialize>(data: &T) -> Result<Vec<u8>, StoreError> {
    let raw = bitcode::serialize(data).map_err(|e| StoreError::Serialise(e.to_string()))?;
    let mut enc = ZlibEncoder::new(Vec::new(), Compression::default());
    enc.write_all(&raw)
        .map_err(|e| StoreError::Serialise(e.to_string()))?;
    enc.finish()
        .map_err(|e| StoreError::Serialise(e.to_string()))
}

fn decompress<T: DeserializeOwned>(compressed: &[u8]) -> Result<T, StoreError> {
    let mut dec = ZlibDecoder::new(compressed);
    let mut raw = Vec::new();
    dec.read_to_end(&mut raw)
        .map_err(|e| StoreError::Corrupt(e.to_string()))?;
    bitcode::deserialize(&raw).map_err(|e| StoreError::Corrupt(e.to_string()))
}

/// Lay out `[len_lo, len_hi, payload...]` into consecutive 10-byte (5 × u16) chunks.
fn pack(compressed: &[u8]) -> Result<Vec<[u16; 5]>, StoreError> {
    if compressed.len() > 65533 {
        return Err(StoreError::TooLarge(compressed.len()));
    }
    let mut stream = Vec::with_capacity(2 + compressed.len());
    stream.extend_from_slice(&(compressed.len() as u16).to_le_bytes());
    stream.extend_from_slice(compressed);
    let rem = stream.len() % 10;
    if rem != 0 {
        stream.resize(stream.len() + (10 - rem), 0);
    }
    Ok(stream
        .chunks_exact(10)
        .map(|c| {
            [
                u16::from_le_bytes([c[0], c[1]]),
                u16::from_le_bytes([c[2], c[3]]),
                u16::from_le_bytes([c[4], c[5]]),
                u16::from_le_bytes([c[6], c[7]]),
                u16::from_le_bytes([c[8], c[9]]),
            ]
        })
        .collect())
}

/// Reconstruct the compressed byte stream from a set of store entities (in index order).
fn unpack(entities: &[&Entity]) -> Result<Vec<u8>, StoreError> {
    let mut stream: Vec<u8> = Vec::with_capacity(entities.len() * 10);
    for ent in entities {
        for &a in &ent.attrs {
            stream.extend_from_slice(&(a as u16).to_le_bytes()); // bit-exact i16→u16
        }
    }
    if stream.len() < 2 {
        return Err(StoreError::Corrupt(
            "stream too short for length header".into(),
        ));
    }
    let len = u16::from_le_bytes([stream[0], stream[1]]) as usize;
    if stream.len() < 2 + len {
        return Err(StoreError::Corrupt(format!(
            "truncated: need {} bytes, have {}",
            2 + len,
            stream.len()
        )));
    }
    Ok(stream[2..2 + len].to_vec())
}

// ─── entity helpers ──────────────────────────────────────────────────────────

/// Return the store version encoded in `ent`'s X coordinate, or `None` if `ent`
/// is not a store entity.
fn entity_version(ent: &Entity) -> Option<u8> {
    if !matches!(ent.ent_type, EntityType::Particles) {
        return None;
    }
    let x = ent.position[0];
    if x > -100.0 || x < -355.0 {
        return None;
    }
    let v = -x - 100.0;
    if v >= 0.0 && v <= 255.0 && (v - v.round()).abs() < 1e-3 {
        Some(v.round() as u8)
    } else {
        None
    }
}

fn build_entity(attrs: [u16; 5], version: u8) -> Entity {
    Entity {
        position: [sentinel_x(version), SENTINEL_Y, SENTINEL_Z],
        attrs: attrs.map(|v| v as i16),
        ent_type: EntityType::Particles,
        extra_data: vec![],
    }
}

fn broadcast_ent(server: &Server, cn: u32, id: usize, attrs: [u16; 5], version: u8) {
    server.broadcast_chan1(&Client {
        client_num: cn as i32,
        packet: channel1::EditEnt {
            ent_id: id as i32,
            pos: Vec3 {
                x: sentinel_x(version),
                y: SENTINEL_Y,
                z: SENTINEL_Z,
            },
            type_: sauer_proto::data::EntityType::Particles,
            attrs: attrs.map(|v| v as i32),
        },
    });
}

fn broadcast_ent_clear(server: &Server, cn: u32, id: usize) {
    server.broadcast_chan1(&Client {
        client_num: cn as i32,
        packet: channel1::EditEnt {
            ent_id: id as i32,
            pos: Vec3 {
                x: 0.0,
                y: 0.0,
                z: 0.0,
            },
            type_: sauer_proto::data::EntityType::Empty,
            attrs: [0; 5],
        },
    });
}

// ─── public API ──────────────────────────────────────────────────────────────

/// Read data of type `T` from the entity store embedded in `world`.
///
/// | Return value            | Meaning                                               |
/// |-------------------------|-------------------------------------------------------|
/// | `Ok(None)`              | No store entities found — world has no stored data.   |
/// | `Ok(Some(data))`        | Data decoded successfully.                            |
/// | `Err(VersionMismatch)`  | Store entities exist but carry a different version.   |
/// | `Err(Corrupt)`          | Entities at the correct version but decode failed.    |
pub fn read<T: DeserializeOwned>(world: &World, version: u8) -> Result<Option<T>, StoreError> {
    // Collect all store entities in index order.
    let store_ents: Vec<(usize, &Entity)> = world
        .entities
        .iter()
        .enumerate()
        .filter_map(|(i, e)| entity_version(e).map(|v| (i, v, e)))
        .map(|(i, _v, e)| (i, e))
        .collect();

    if store_ents.is_empty() {
        return Ok(None);
    }

    // All store entities must agree on version; report the first mismatch.
    for (_, ent) in &store_ents {
        let found = entity_version(ent).unwrap();
        if found != version {
            return Err(StoreError::VersionMismatch {
                expected: version,
                found,
            });
        }
    }

    let refs: Vec<&Entity> = store_ents.iter().map(|(_, e)| *e).collect();
    let compressed = unpack(&refs)?;
    Ok(Some(decompress(&compressed)?))
}

/// Encode `data` (bitcode + zlib) and write it as store entities into `world`,
/// then broadcast `N_EDITENT` packets to all clients.
///
/// Existing store entities (any version) are reused where possible to keep
/// entity indices stable; excess old slots are cleared; additional slots are
/// appended when the new data requires more entities than were previously stored.
///
/// `cn` is used as the apparent sender in the `N_CLIENT` wrapper (use the
/// client number of whoever triggered the write, e.g. the player who `/sendmap`-ed).
pub fn write<T: Serialize>(
    world: &mut World,
    server: &Server,
    cn: u32,
    data: &T,
    version: u8,
) -> Result<(), StoreError> {
    let compressed = compress(data)?;
    let chunks = pack(&compressed)?;

    // Indices of existing store entities (any version), in order.
    let old_ids: Vec<usize> = world
        .entities
        .iter()
        .enumerate()
        .filter_map(|(i, e)| entity_version(e).map(|_| i))
        .collect();

    let n_old = old_ids.len();
    let n_new = chunks.len();

    // Reuse overlapping slots.
    for i in 0..n_old.min(n_new) {
        let id = old_ids[i];
        world.entities[id] = build_entity(chunks[i], version);
        broadcast_ent(server, cn, id, chunks[i], version);
    }

    // Clear any leftover old slots that are no longer needed.
    for i in n_new..n_old {
        let id = old_ids[i];
        world.entities[id] = Entity {
            position: [0.0, 0.0, 0.0],
            attrs: [0; 5],
            ent_type: EntityType::Empty,
            extra_data: vec![],
        };
        broadcast_ent_clear(server, cn, id);
    }

    // Append any additional slots required.
    let append_start = world.entities.len();
    for i in n_old..n_new {
        world.entities.push(build_entity(chunks[i], version));
        broadcast_ent(server, cn, append_start + (i - n_old), chunks[i], version);
    }

    info!(
        "[entity_store] wrote {} entit{} (v{version}, {} compressed bytes); \
         reused={} cleared={} appended={}",
        n_new,
        if n_new == 1 { "y" } else { "ies" },
        compressed.len(),
        n_old.min(n_new),
        n_old.saturating_sub(n_new),
        n_new.saturating_sub(n_old),
    );

    Ok(())
}
