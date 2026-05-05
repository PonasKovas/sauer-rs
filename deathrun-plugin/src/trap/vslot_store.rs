use crate::map_state::MapState;
use flate2::{Compression, read::ZlibDecoder, write::ZlibEncoder};
use serde::{Serialize, de::DeserializeOwned};
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

fn decode_chunk_name(name: &[u8]) -> Option<usize> {
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

fn putint(out: &mut Vec<u8>, n: i32) {
    if (-126..=127).contains(&n) {
        out.push(n as u8);
    } else if (-32768..=32767).contains(&n) {
        out.push(0x80);
        out.extend_from_slice(&(n as i16).to_le_bytes());
    } else {
        out.push(0x81);
        out.extend_from_slice(&n.to_le_bytes());
    }
}

fn write_sendstring(out: &mut Vec<u8>, data: &[u8]) {
    for &c in data {
        putint(out, c as i32);
    }
    putint(out, 0);
}

pub fn encode_payload<T: Serialize>(
    data: &T,
    version: u8,
    domain_id: u8,
) -> Result<Vec<Vec<u8>>, StoreError> {
    let raw = bitcode::serialize(data).map_err(|e| StoreError::Serialise(e.to_string()))?;

    let mut zlib = Vec::new();
    let mut enc = ZlibEncoder::new(&mut zlib, Compression::default());
    enc.write_all(&raw)
        .map_err(|e| StoreError::Serialise(e.to_string()))?;
    enc.finish()
        .map_err(|e| StoreError::Serialise(e.to_string()))?;

    let mut payload = Vec::with_capacity(6 + zlib.len());
    payload.extend_from_slice(b"DR");
    payload.push(version);
    payload.push(domain_id);
    payload.extend_from_slice(&(zlib.len() as u16).to_le_bytes());
    payload.extend_from_slice(&zlib);

    let total_chunks = (payload.len() + 15) / 16;
    let mut extra_data_blocks = Vec::new();
    let mut current_extra = Vec::new();
    let mut chunks_in_block = 0;

    for idx in 0..total_chunks {
        let start = idx * 16;
        let end = (start + 16).min(payload.len());
        let mut chunk = [0u8; 16];
        chunk[..end - start].copy_from_slice(&payload[start..end]);

        current_extra.push(0); // VSLOT_SHPARAM
        write_sendstring(&mut current_extra, &encode_chunk_name(idx));
        current_extra.extend_from_slice(&chunk);

        chunks_in_block += 1;
        if chunks_in_block >= 60 {
            // Safe maximum per network packet
            current_extra.push(0xFF);
            extra_data_blocks.push(std::mem::take(&mut current_extra));
            chunks_in_block = 0;
        }
    }
    if !current_extra.is_empty() {
        current_extra.push(0xFF);
        extra_data_blocks.push(current_extra);
    }

    Ok(extra_data_blocks)
}

#[derive(Default, Debug, Clone)]
pub struct StoreStats {
    pub chunks: usize,
    pub structural_bytes: usize,
    pub payload_compressed: usize,
    pub payload_uncompressed: usize,
}

pub fn read_all<T: DeserializeOwned + Clone>(
    map: &MapState,
    version: u8,
    domain_id: u8,
    extract_pos: impl Fn(&T) -> [i32; 3],
) -> Result<(Vec<T>, StoreStats), StoreError> {
    let mut stats = StoreStats::default();
    // 1. Decode all active VSlots into a map (vslot_idx -> decoded payload)
    let mut decoded_vslots = std::collections::HashMap::new();

    for (vslot_idx, vs_opt) in map.meta.vslots.iter().enumerate() {
        if let Some(vs) = vs_opt {
            let mut chunks = std::collections::HashMap::new();
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
            if &payload[0..2] != b"DR" {
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

    // 2. Traverse octree, find physical VSlots, and validate absolute coordinates
    let mut valid_results = std::collections::HashMap::new();

    fn traverse<T: Clone>(
        node: &sauer_map::Node,
        pos: [i32; 3],
        size: i32,
        decoded: &std::collections::HashMap<u16, T>,
        results: &mut std::collections::HashMap<[i32; 3], T>,
        extract_pos: &impl Fn(&T) -> [i32; 3],
    ) {
        match node {
            sauer_map::Node::Leaf(c) => {
                for &t in &c.textures {
                    if let Some(trap) = decoded.get(&t) {
                        let bpos = extract_pos(trap);
                        // A physically valid trap data cube must reside inside the button volume
                        let valid_x = pos[0] >= bpos[0] && pos[0] <= bpos[0] + 8;
                        let valid_y = pos[1] >= bpos[1] && pos[1] <= bpos[1] + 8;
                        let valid_z = pos[2] >= bpos[2] && pos[2] <= bpos[2] + 8;

                        if valid_x && valid_y && valid_z {
                            // HashMap keyed by button_pos implicitly deduplicates
                            // multiple hits if the cavity floor was subdivided.
                            results.insert(bpos, trap.clone());
                        } else {
                            tracing::warn!(
                                "Rejected copied trap payload outside expected bounds at {:?}",
                                pos
                            );
                        }
                    }
                }
            }
            sauer_map::Node::Branch(ch) => {
                let half = size / 2;
                for i in 0..8 {
                    let cx = (i & 1) as i32;
                    let cy = ((i >> 1) & 1) as i32;
                    let cz = ((i >> 2) & 1) as i32;
                    let cpos = [pos[0] + cx * half, pos[1] + cy * half, pos[2] + cz * half];
                    traverse(&ch[i], cpos, half, decoded, results, extract_pos);
                }
            }
        }
    }

    traverse(
        &map.world.root,
        [0, 0, 0],
        map.world.size,
        &decoded_vslots,
        &mut valid_results,
        &extract_pos,
    );

    Ok((valid_results.into_values().collect(), stats))
}
