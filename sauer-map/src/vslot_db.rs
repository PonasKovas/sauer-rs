use crate::{Node, World};
use cube_string::CubeString;
use flate2::{read::ZlibDecoder, write::ZlibEncoder, Compression};
use ogz_rs::data::VSlot;
use sauer_proto::data::{VSlotDelta, VSlotDeltaParam};
use serde::{de::DeserializeOwned, Serialize};
use std::collections::HashMap;
use std::io::{Read, Write};

#[derive(Debug, thiserror::Error)]
pub enum StoreError {
    #[error("serialise error: {0}")]
    Serialise(String),
    #[error("corrupt data: {0}")]
    Corrupt(String),
}

pub fn encode_chunk_name(mut idx: usize) -> Vec<u8> {
    let mut name = Vec::new();
    name.push(1); // Magic prefix to guarantee no collision with real shader params
    loop {
        let rem = (idx % 254) as u8;
        name.push(rem + 2); // Avoid 0
        idx /= 254;
        if idx == 0 {
            break;
        }
    }
    name
}

pub fn decode_chunk_name(name: &[u8]) -> Option<usize> {
    if name.is_empty() || name[0] != 1 {
        return None;
    }
    let mut idx = 0;
    let mut mult = 1;
    for &b in &name[1..] {
        if b < 2 {
            return None;
        }
        idx += (b - 2) as usize * mult;
        mult *= 254;
    }
    Some(idx)
}

pub fn compress_payload<T: Serialize>(
    data: &T,
    magic: [u8; 2],
    version: u8,
    domain_id: u8,
) -> Result<Vec<u8>, StoreError> {
    let raw = bitcode::serialize(data).map_err(|e| StoreError::Serialise(e.to_string()))?;

    let mut zlib = Vec::new();
    let mut enc = ZlibEncoder::new(&mut zlib, Compression::default());
    enc.write_all(&raw)
        .map_err(|e| StoreError::Serialise(e.to_string()))?;
    enc.finish()
        .map_err(|e| StoreError::Serialise(e.to_string()))?;

    let mut payload = Vec::with_capacity(6 + zlib.len());
    payload.extend_from_slice(&magic);
    payload.push(version);
    payload.push(domain_id);
    payload.extend_from_slice(&(zlib.len() as u16).to_le_bytes());
    payload.extend_from_slice(&zlib);

    Ok(payload)
}

pub fn chunk_payload(payload: &[u8]) -> Vec<VSlotDelta> {
    let total_chunks = (payload.len() + 15) / 16;
    let mut extra_data_blocks = Vec::new();
    let mut current_delta = VSlotDelta::default();
    let mut chunks_in_block = 0;

    for idx in 0..total_chunks {
        let start = idx * 16;
        let end = (start + 16).min(payload.len());
        let mut chunk = [0u8; 16];
        chunk[..end - start].copy_from_slice(&payload[start..end]);

        let mut values = [0.0; 4];
        for i in 0..4 {
            values[i] = f32::from_le_bytes(chunk[i * 4..(i + 1) * 4].try_into().unwrap());
        }

        current_delta.params.push(VSlotDeltaParam {
            name: CubeString::from_bytes(encode_chunk_name(idx)),
            values,
        });

        chunks_in_block += 1;
        if chunks_in_block >= 200 {
            // Maximum to efficiently utilize ~5KB packet size limit
            extra_data_blocks.push(std::mem::take(&mut current_delta));
            chunks_in_block = 0;
        }
    }
    if !current_delta.params.is_empty() {
        extra_data_blocks.push(current_delta);
    }

    extra_data_blocks
}

pub fn encode_payload<T: Serialize>(
    data: &T,
    magic: [u8; 2],
    version: u8,
    domain_id: u8,
) -> Result<Vec<VSlotDelta>, StoreError> {
    let payload = compress_payload(data, magic, version, domain_id)?;
    Ok(chunk_payload(&payload))
}

#[derive(Default, Debug, Clone)]
pub struct StoreStats {
    pub chunks: usize,
    pub structural_bytes: usize,
    pub payload_compressed: usize,
    pub payload_uncompressed: usize,
}

#[derive(Debug, Clone)]
pub struct FaceInstance {
    pub pos: [i32; 3],
    pub size: i32,
    pub orient: u8,
}

#[derive(Debug, Clone)]
pub struct DbEntry<T> {
    pub vslot_idx: u16,
    pub data: T,
    pub instances: Vec<FaceInstance>,
}

pub fn read_all<T: DeserializeOwned + Clone>(
    world: &World,
    vslots: &[Option<VSlot>],
    magic: [u8; 2],
    version: u8,
    domain_id: u8,
) -> Result<(Vec<DbEntry<T>>, StoreStats), StoreError> {
    let mut stats = StoreStats::default();
    let mut decoded_vslots = HashMap::new();

    for (vslot_idx, vs_opt) in vslots.iter().enumerate() {
        if let Some(vs) = vs_opt {
            let mut chunks = HashMap::new();
            let mut max_idx = 0;

            for p in &vs.params {
                let name_bytes = p.name.as_bytes();
                if let Some(idx) = decode_chunk_name(name_bytes) {
                    stats.chunks += 1;
                    stats.structural_bytes += 2 + name_bytes.len() + 16;
                    let mut chunk = [0u8; 16];
                    for i in 0..4 {
                        chunk[i * 4..(i + 1) * 4].copy_from_slice(&p.values[i].to_le_bytes());
                    }
                    chunks.insert(idx, chunk);
                    if idx > max_idx {
                        max_idx = idx;
                    }
                }
            }

            if chunks.is_empty() {
                continue;
            }

            let mut payload = Vec::with_capacity((max_idx + 1) * 16);
            let mut valid = true;
            for i in 0..=max_idx {
                if let Some(chunk) = chunks.get(&i) {
                    payload.extend_from_slice(chunk);
                } else {
                    valid = false;
                    break;
                }
            }

            if !valid || payload.len() < 6 {
                continue;
            }
            if &payload[0..2] != &magic {
                continue;
            }
            if payload[2] != version || payload[3] != domain_id {
                continue;
            }

            let zlib_len = u16::from_le_bytes([payload[4], payload[5]]) as usize;
            if payload.len() < 6 + zlib_len {
                continue;
            }

            let zlib_data = &payload[6..6 + zlib_len];
            stats.payload_compressed += zlib_len;
            let mut dec = ZlibDecoder::new(zlib_data);
            let mut raw = Vec::new();
            if dec.read_to_end(&mut raw).is_ok() {
                stats.payload_uncompressed += raw.len();
                if let Ok(data) = bitcode::deserialize::<T>(&raw) {
                    decoded_vslots.insert(vslot_idx as u16, data);
                }
            }
        }
    }

    let mut instance_map: HashMap<u16, Vec<FaceInstance>> = HashMap::new();

    fn traverse<T: Clone>(
        node: &Node,
        pos: [i32; 3],
        size: i32,
        decoded: &HashMap<u16, T>,
        instances: &mut HashMap<u16, Vec<FaceInstance>>,
    ) {
        match node {
            Node::Leaf(c) => {
                for (orient, &t) in c.textures.iter().enumerate() {
                    if decoded.contains_key(&t) {
                        instances.entry(t).or_default().push(FaceInstance {
                            pos,
                            size,
                            orient: orient as u8,
                        });
                    }
                }
            }
            Node::Branch(ch) => {
                let half = size / 2;
                for i in 0..8 {
                    let cx = (i & 1) as i32;
                    let cy = ((i >> 1) & 1) as i32;
                    let cz = ((i >> 2) & 1) as i32;
                    let cpos = [pos[0] + cx * half, pos[1] + cy * half, pos[2] + cz * half];
                    traverse(&ch[i], cpos, half, decoded, instances);
                }
            }
        }
    }

    traverse(
        &world.root,
        [0, 0, 0],
        world.size,
        &decoded_vslots,
        &mut instance_map,
    );

    let mut results = Vec::new();
    for (vslot_idx, data) in decoded_vslots {
        if let Some(instances) = instance_map.remove(&vslot_idx) {
            results.push(DbEntry {
                vslot_idx,
                data,
                instances,
            });
        }
    }

    Ok((results, stats))
}