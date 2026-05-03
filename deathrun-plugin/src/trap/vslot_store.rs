use flate2::{Compression, read::ZlibDecoder, write::ZlibEncoder};
use sauer_proto::channel1::EditVSlot;
use sauer_proto::data::SelInfo;
use serde::{de::DeserializeOwned, Serialize};
use std::io::{Read, Write};
use crate::map_state::MapState;

const MAX_NAME_PAYLOAD: usize = 200;
const MAX_FLOAT_PAYLOAD: usize = 16;
const MAX_CHUNK_PAYLOAD: usize = MAX_NAME_PAYLOAD + MAX_FLOAT_PAYLOAD;
const MAX_CHUNKS_PER_PACKET: usize = 90;

#[derive(Debug, thiserror::Error)]
pub enum StoreError {
    #[error("serialise error: {0}")]
    Serialise(String),
    #[error("corrupt data: {0}")]
    Corrupt(String),
    #[error("version mismatch: expected {expected}, found {found}")]
    VersionMismatch { expected: u8, found: u8 },
}

pub fn cobs_encode(data: &[u8]) -> Vec<u8> {
    let mut encoded = Vec::with_capacity(data.len() + data.len() / 254 + 2);
    encoded.push(0);
    let mut code_idx = 0;
    let mut code = 1u8;

    for &byte in data {
        if byte == 0 {
            encoded[code_idx] = code;
            code_idx = encoded.len();
            encoded.push(0);
            code = 1;
        } else {
            encoded.push(byte);
            code += 1;
            if code == 0xFF {
                encoded[code_idx] = code;
                code_idx = encoded.len();
                encoded.push(0);
                code = 1;
            }
        }
    }
    encoded[code_idx] = code;
    encoded
}

pub fn cobs_decode(data: &[u8]) -> Result<Vec<u8>, StoreError> {
    let mut decoded = Vec::with_capacity(data.len());
    let mut i = 0;
    while i < data.len() {
        let code = data[i] as usize;
        if code == 0 {
            return Err(StoreError::Corrupt("COBS decode: zero byte".into()));
        }
        if i + code > data.len() {
            for j in 1..(data.len() - i) {
                decoded.push(data[i + j]);
            }
            break;
        }
        for j in 1..code {
            decoded.push(data[i + j]);
        }
        i += code;
        if i < data.len() && code < 0xFF {
            decoded.push(0);
        }
    }
    Ok(decoded)
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

pub fn write<T: Serialize>(
    sel: &SelInfo,
    data: &T,
    version: u8,
    domain_id: u8,
) -> Result<Vec<EditVSlot>, StoreError> {
    let raw = bitcode::serialize(data).map_err(|e| StoreError::Serialise(e.to_string()))?;
    
    let mut stream = vec![version];
    stream.extend_from_slice(&raw);

    let mut enc = ZlibEncoder::new(Vec::new(), Compression::default());
    enc.write_all(&stream).map_err(|e| StoreError::Serialise(e.to_string()))?;
    let compressed = enc.finish().map_err(|e| StoreError::Serialise(e.to_string()))?;

    let total_chunks = (compressed.len() + MAX_CHUNK_PAYLOAD - 1) / MAX_CHUNK_PAYLOAD;
    if total_chunks == 0 {
        return Ok(vec![EditVSlot {
            sel: sel.clone(),
            delta: 0,
            all_faces: true,
            extra_data: vec![0xFF],
        }]);
    }

    let mut packets = Vec::new();
    let mut current_extra_data = Vec::new();
    let mut chunks_in_packet = 0;
    let mut is_first_packet = true;

    for idx in 0..total_chunks {
        let start = idx * MAX_CHUNK_PAYLOAD;
        let end = (start + MAX_CHUNK_PAYLOAD).min(compressed.len());
        let chunk = &compressed[start..end];

        let float_bytes = &chunk[..chunk.len().min(MAX_FLOAT_PAYLOAD)];
        let name_bytes = if chunk.len() > MAX_FLOAT_PAYLOAD {
            &chunk[MAX_FLOAT_PAYLOAD..]
        } else {
            &[]
        };

        let mut float_padded = [0u8; 16];
        float_padded[..float_bytes.len()].copy_from_slice(float_bytes);

        let vals = [
            f32::from_le_bytes(float_padded[0..4].try_into().unwrap()),
            f32::from_le_bytes(float_padded[4..8].try_into().unwrap()),
            f32::from_le_bytes(float_padded[8..12].try_into().unwrap()),
            f32::from_le_bytes(float_padded[12..16].try_into().unwrap()),
        ];

        let cobs_encoded = cobs_encode(name_bytes);
        let prefix = format!("DR{}_{}_{}_", domain_id, idx, total_chunks);
        let mut name_full = prefix.into_bytes();
        name_full.extend_from_slice(&cobs_encoded);

        current_extra_data.push(0); // VSLOT_SHPARAM
        write_sendstring(&mut current_extra_data, &name_full);
        for v in &vals {
            current_extra_data.extend_from_slice(&v.to_le_bytes());
        }

        chunks_in_packet += 1;
        if chunks_in_packet >= MAX_CHUNKS_PER_PACKET || idx == total_chunks - 1 {
            current_extra_data.push(0xFF);
            packets.push(EditVSlot {
                sel: sel.clone(),
                delta: if is_first_packet { 0 } else { 1 },
                all_faces: true,
                extra_data: std::mem::take(&mut current_extra_data),
            });
            chunks_in_packet = 0;
            is_first_packet = false;
        }
    }

    Ok(packets)
}

pub fn read<T: DeserializeOwned>(
    map: &MapState,
    version: u8,
    domain_id: u8,
) -> Result<Option<T>, StoreError> {
    let mut chunks = std::collections::HashMap::new();
    let mut total_chunks = None;

    let prefix_match = format!("DR{}_", domain_id);

    for vs_opt in &map.meta.vslots {
        if let Some(vs) = vs_opt {
            for p in &vs.params {
                let name_str = p.name.to_utf8();
                if name_str.starts_with(&prefix_match) {
                    let parts: Vec<&str> = name_str.splitn(4, '_').collect();
                    if parts.len() == 4 {
                        if let (Ok(idx), Ok(tot)) = (parts[1].parse::<usize>(), parts[2].parse::<usize>()) {
                            total_chunks = Some(tot);
                            
                            let mut chunk_data = Vec::new();
                            for &val in &p.values {
                                chunk_data.extend_from_slice(&f32::to_le_bytes(val));
                            }
                            
                            let cobs_data = parts[3].as_bytes();
                            let decoded_name = cobs_decode(cobs_data)?;
                            chunk_data.extend_from_slice(&decoded_name);
                            
                            chunks.insert(idx, chunk_data);
                        }
                    }
                }
            }
        }
    }

    if chunks.is_empty() {
        return Ok(None);
    }

    let tot = total_chunks.ok_or_else(|| StoreError::Corrupt("Missing total chunks".into()))?;
    if chunks.len() != tot {
        tracing::warn!("Missing chunks: got {}, expected {}", chunks.len(), tot);
        return Err(StoreError::Corrupt(format!("Missing chunks: got {}, expected {}", chunks.len(), tot)));
    }

    let mut compressed = Vec::new();
    for i in 0..tot {
        let mut chunk = chunks.remove(&i).unwrap();
        compressed.append(&mut chunk);
    }

    let mut dec = ZlibDecoder::new(compressed.as_slice());
    let mut decompressed = Vec::new();
    dec.read_to_end(&mut decompressed).map_err(|e| StoreError::Corrupt(e.to_string()))?;

    if decompressed.is_empty() {
        return Err(StoreError::Corrupt("Empty uncompressed data".into()));
    }

    let found_version = decompressed[0];
    if found_version != version {
        return Err(StoreError::VersionMismatch { expected: version, found: found_version });
    }

    let data = bitcode::deserialize(&decompressed[1..]).map_err(|e| StoreError::Corrupt(e.to_string()))?;
    Ok(Some(data))
}