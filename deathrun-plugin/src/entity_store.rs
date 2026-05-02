use flate2::{Compression, read::ZlibDecoder, write::ZlibEncoder};
use ogz_rs::data::{Entity, EntityType};
use serde::{Serialize, de::DeserializeOwned};
use std::io::{Read, Write};
use thiserror::Error;

const SENTINEL_COORD: f32 = -10000.0;

#[derive(Debug, Error)]
pub enum StoreError {
    #[error("payload too large: {0} bytes")]
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

fn pack(compressed: &[u8], version: u8) -> Result<Vec<[i16; 5]>, StoreError> {
    if compressed.len() > 65533 {
        return Err(StoreError::TooLarge(compressed.len()));
    }
    let mut stream = Vec::with_capacity(3 + compressed.len());
    stream.push(version);
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
                i16::from_le_bytes([c[0], c[1]]),
                i16::from_le_bytes([c[2], c[3]]),
                i16::from_le_bytes([c[4], c[5]]),
                i16::from_le_bytes([c[6], c[7]]),
                i16::from_le_bytes([c[8], c[9]]),
            ]
        })
        .collect())
}

fn unpack(entities: &[&Entity]) -> Result<(u8, Vec<u8>), StoreError> {
    let mut stream: Vec<u8> = Vec::with_capacity(entities.len() * 10);
    for ent in entities {
        for &a in &ent.attrs {
            stream.extend_from_slice(&(a as u16).to_le_bytes());
        }
    }
    if stream.len() < 3 {
        return Err(StoreError::Corrupt("stream too short".into()));
    }
    let version = stream[0];
    let len = u16::from_le_bytes([stream[1], stream[2]]) as usize;
    if stream.len() < 3 + len {
        return Err(StoreError::Corrupt("truncated data".into()));
    }
    Ok((version, stream[3..3 + len].to_vec()))
}

fn is_store_entity(ent: &Entity, domain_id: u8) -> bool {
    if !matches!(ent.ent_type, EntityType::Particles) {
        return false;
    }
    if (ent.position[0] - SENTINEL_COORD).abs() > 0.1
        || (ent.position[1] - SENTINEL_COORD).abs() > 0.1
    {
        return false;
    }
    let expected_z = SENTINEL_COORD - (domain_id as f32);
    (ent.position[2] - expected_z).abs() < 0.1
        || (domain_id == 1 && (ent.position[2] - SENTINEL_COORD).abs() < 0.1)
}

fn build_entity(attrs: [i16; 5], domain_id: u8) -> Entity {
    Entity {
        position: [
            SENTINEL_COORD,
            SENTINEL_COORD,
            SENTINEL_COORD - (domain_id as f32),
        ],
        attrs,
        ent_type: EntityType::Particles,
        extra_data: vec![],
    }
}

pub fn get_store_entity_ids(world: &sauer_map::World, domain_id: u8) -> Vec<usize> {
    world
        .entities
        .iter()
        .enumerate()
        .filter_map(|(i, e)| {
            if is_store_entity(e, domain_id) {
                Some(i)
            } else {
                None
            }
        })
        .collect()
}

pub fn read<T: DeserializeOwned>(
    world: &sauer_map::World,
    version: u8,
    domain_id: u8,
) -> Result<Option<T>, StoreError> {
    let mut store_ents: Vec<(usize, &Entity)> = world
        .entities
        .iter()
        .enumerate()
        .filter(|(_, e)| is_store_entity(e, domain_id))
        .collect();
    if store_ents.is_empty() {
        return Ok(None);
    }

    // Sort entities by index to ensure we unpack chunks in the order they were written
    store_ents.sort_by_key(|&(i, _)| i);

    let refs: Vec<&Entity> = store_ents.iter().map(|(_, e)| *e).collect();
    let (found_version, compressed) = unpack(&refs)?;

    if found_version != version {
        return Err(StoreError::VersionMismatch {
            expected: version,
            found: found_version,
        });
    }
    Ok(Some(decompress(&compressed)?))
}

pub fn write<T: Serialize>(
    world: &mut sauer_map::World,
    data: &T,
    version: u8,
    domain_id: u8,
) -> Result<Vec<(usize, Entity)>, StoreError> {
    let compressed = compress(data)?;
    let chunks = pack(&compressed, version)?;
    let old_ids: Vec<usize> = world
        .entities
        .iter()
        .enumerate()
        .filter_map(|(i, e)| {
            if is_store_entity(e, domain_id) {
                Some(i)
            } else {
                None
            }
        })
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

    // Build the full set of target slots: keep existing slots, then
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

    // Sort so chunk index matches entity-list order
    slots.sort_unstable();

    for (i, &slot) in slots.iter().enumerate() {
        world.entities[slot] = build_entity(chunks[i], domain_id);
        changed.push((slot, world.entities[slot].clone()));
    }

    Ok(changed)
}
