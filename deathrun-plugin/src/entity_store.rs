use flate2::{Compression, read::ZlibDecoder, write::ZlibEncoder};
use ogz_rs::data::{Entity, EntityType};
use serde::{Serialize, de::DeserializeOwned};
use std::io::{Read, Write};
use thiserror::Error;

const SENTINEL_Z: f32 = -134217728.0; // minimum possible value due to DMF quantization in packets

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

fn unpack(entities: &[&Entity]) -> Result<Vec<u8>, StoreError> {
    let mut stream: Vec<u8> = Vec::with_capacity(entities.len() * 10);
    for ent in entities {
        for &a in &ent.attrs {
            stream.extend_from_slice(&(a as u16).to_le_bytes());
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

pub fn entity_version(ent: &Entity, domain_id: u8) -> Option<u8> {
    if !matches!(ent.ent_type, EntityType::Particles) {
        return None;
    }
    if (ent.position[1] - -(100.0 + domain_id as f32)).abs() > 1e-3 {
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

fn build_entity(attrs: [u16; 5], version: u8, domain_id: u8) -> Entity {
    Entity {
        position: [
            -(100.0 + version as f32),
            -(100.0 + domain_id as f32),
            SENTINEL_Z,
        ],
        attrs: attrs.map(|v| v as i16),
        ent_type: EntityType::Particles,
        extra_data: vec![],
    }
}

pub fn get_store_entity_ids(world: &sauer_map::World, domain_id: u8) -> Vec<usize> {
    world.entities.iter().enumerate()
        .filter_map(|(i, e)| entity_version(e, domain_id).map(|_| i))
        .collect()
}

pub fn read<T: DeserializeOwned>(
    world: &sauer_map::World,
    version: u8,
    domain_id: u8,
) -> Result<Option<T>, StoreError> {
    let store_ents: Vec<(usize, &Entity)> = world
        .entities
        .iter()
        .enumerate()
        .filter_map(|(i, e)| entity_version(e, domain_id).map(|v| (i, v, e)))
        .map(|(i, _v, e)| (i, e))
        .collect();
    if store_ents.is_empty() {
        return Ok(None);
    }
    for (_, ent) in &store_ents {
        let found = entity_version(ent, domain_id).unwrap();
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

pub fn write<T: Serialize>(
    world: &mut sauer_map::World,
    data: &T,
    version: u8,
    domain_id: u8,
) -> Result<Vec<(usize, Entity)>, StoreError> {
    let compressed = compress(data)?;
    let chunks = pack(&compressed)?;
    let old_ids: Vec<usize> = world
        .entities
        .iter()
        .enumerate()
        .filter_map(|(i, e)| entity_version(e, domain_id).map(|_| i))
        .collect();
    let n_old = old_ids.len();
    let n_new = chunks.len();
    let mut changed = Vec::new();

    // Release excess old slots back to the empty pool first.
    for i in n_new..n_old {
        let id = old_ids[i];
        world.entities[id] = Entity {
            position: [0.0; 3],
            attrs: [0; 5],
            ent_type: EntityType::Empty,
            extra_data: vec![],
        };
        changed.push((id, world.entities[id].clone()));
    }

    // Build the full set of target slots: keep existing domain slots, then
    // fill from empty slots, then push new entities as a last resort.
    let mut slots: Vec<usize> = old_ids[..n_old.min(n_new)].to_vec();
    if n_new > n_old {
        let mut empty_iter = world
            .entities
            .iter()
            .enumerate()
            .filter(|(_, e)| matches!(e.ent_type, EntityType::Empty))
            .map(|(i, _)| i)
            .collect::<Vec<_>>()
            .into_iter();
        for _ in n_old..n_new {
            if let Some(slot) = empty_iter.next() {
                slots.push(slot);
            } else {
                let id = world.entities.len();
                world.entities.push(Entity {
                    position: [0.0; 3],
                    attrs: [0; 5],
                    ent_type: EntityType::Empty,
                    extra_data: vec![],
                });
                slots.push(id);
            }
        }
    }

    // Sort so chunk index matches entity-list order, which is what unpack
    // assumes when reading.
    slots.sort_unstable();

    for (i, &slot) in slots.iter().enumerate() {
        world.entities[slot] = build_entity(chunks[i], version, domain_id);
        changed.push((slot, world.entities[slot].clone()));
    }

    Ok(changed)
}
