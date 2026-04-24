//! Temporary proof-of-concept: serialize a data struct into IBullets (type 9) entities
//! at position 0,0,0, packing data into their 5 u16 attribute slots.
//!
//! On each /sendmap:
//!   1. Scan the uploaded map for type-9 entities at 0,0,0 and try to decode POC data.
//!   2. If decoded, log to console and broadcast a ServMsg.
//!   3. Encode a fresh PocData, append the required entities to the in-memory map, and
//!      broadcast N_EDITENT packets so all clients mirror the entities.

use std::io::{Read, Write};
use std::time::{SystemTime, UNIX_EPOCH};

use cube_string::IntoCubeStringLossy;
use flate2::{Compression, read::ZlibDecoder, write::ZlibEncoder};
use ogz_rs::data::{Entity, EntityType};
use sauer_proto::{Client, channel1, data::Vec3};
use sauer_server::Server;
use serde::{Deserialize, Serialize};
use tracing::{info, warn};

use crate::map::get_server_state;

// ─── data struct ─────────────────────────────────────────────────────────────

#[derive(Debug, Serialize, Deserialize)]
pub struct PocData {
    pub message: String,
    pub timestamp: u64,
    pub counter: u32,
}

impl PocData {
    fn fresh() -> Self {
        let timestamp = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap_or_default()
            .as_secs();
        Self {
            message: format!("sauer-rs entity POC at t={timestamp}"),
            timestamp,
            counter: 42,
        }
    }
}

// ─── encoding ────────────────────────────────────────────────────────────────

fn encode(data: &PocData) -> Vec<u8> {
    let raw = bitcode::serialize(data).expect("bitcode serialise failed");
    let mut enc = ZlibEncoder::new(Vec::new(), Compression::default());
    enc.write_all(&raw).expect("zlib write failed");
    enc.finish().expect("zlib finish failed")
}

fn decode(compressed: &[u8]) -> Option<PocData> {
    let mut dec = ZlibDecoder::new(compressed);
    let mut raw = Vec::new();
    dec.read_to_end(&mut raw).ok()?;
    bitcode::deserialize(&raw).ok()
}

// ─── packing / unpacking between bytes and entity attrs ─────────────────────
//
// Wire format (flat byte stream split across entities):
//   bytes 0-1 : MAGIC  (0xD34D)
//   bytes 2-3 : total compressed length as u16 LE
//   bytes 4.. : compressed payload
//
// Each entity holds 5 x u16 = 10 bytes. The stream is padded to a multiple of
// 10 bytes before chunking. attr values are stored as u16 little-endian pairs;
// i16 bit-pattern is preserved on round-trip via `as u16` / `as i16` casts.

const MAGIC: u16 = 0xD34D;
const POC_ENT_TYPE: u8 = 9; // IBullets

fn pack(compressed: &[u8]) -> Vec<[u16; 5]> {
    assert!(
        compressed.len() <= 65531,
        "compressed data too large (max 65531 bytes)"
    );

    let mut stream = Vec::with_capacity(4 + compressed.len());
    stream.extend_from_slice(&MAGIC.to_le_bytes());
    stream.extend_from_slice(&(compressed.len() as u16).to_le_bytes());
    stream.extend_from_slice(compressed);
    // Pad to multiple of 10
    let rem = stream.len() % 10;
    if rem != 0 {
        stream.resize(stream.len() + (10 - rem), 0);
    }

    stream
        .chunks_exact(10)
        .map(|chunk| {
            [
                u16::from_le_bytes([chunk[0], chunk[1]]),
                u16::from_le_bytes([chunk[2], chunk[3]]),
                u16::from_le_bytes([chunk[4], chunk[5]]),
                u16::from_le_bytes([chunk[6], chunk[7]]),
                u16::from_le_bytes([chunk[8], chunk[9]]),
            ]
        })
        .collect()
}

fn unpack(entities: &[&Entity]) -> Option<Vec<u8>> {
    if entities.is_empty() {
        return None;
    }

    let mut stream: Vec<u8> = Vec::new();
    for ent in entities {
        for &a in &ent.attrs {
            let v = a as u16; // bit-exact i16→u16
            stream.extend_from_slice(&v.to_le_bytes());
        }
    }

    if stream.len() < 4 {
        return None;
    }

    let magic = u16::from_le_bytes([stream[0], stream[1]]);
    if magic != MAGIC {
        return None;
    }

    let len = u16::from_le_bytes([stream[2], stream[3]]) as usize;
    if stream.len() < 4 + len {
        return None;
    }

    Some(stream[4..4 + len].to_vec())
}

// ─── helpers ─────────────────────────────────────────────────────────────────

fn is_poc_entity(ent: &Entity) -> bool {
    matches!(ent.ent_type, EntityType::GameSpecific(POC_ENT_TYPE))
        && ent.position == [0.0, 0.0, 0.0]
}

// ─── public entry point ──────────────────────────────────────────────────────

/// Call this after the map sent by `cn` has been parsed and stored in server state.
pub fn on_map_received(server: &Server, cn: u32) {
    // 1. Look for existing POC entities in the just-uploaded map.
    {
        let state = get_server_state(server).read().unwrap();
        if let Some(map) = state.map.as_ref() {
            let poc_ents: Vec<&Entity> =
                map.world.entities.iter().filter(|e| is_poc_entity(e)).collect();

            if poc_ents.is_empty() {
                info!("[entity_poc] No POC entities found in uploaded map.");
            } else {
                match unpack(&poc_ents).and_then(|c| decode(&c)) {
                    Some(data) => {
                        info!("[entity_poc] Decoded POC data: {:?}", data);
                        server.broadcast_chan1(&channel1::ServMsg {
                            text: format!(
                                "[POC] Loaded from map entities — msg: '{}' | ts: {} | counter: {}",
                                data.message, data.timestamp, data.counter
                            )
                            .lossy_cubestring(),
                        });
                    }
                    None => warn!(
                        "[entity_poc] Found {} candidate entities but decode failed",
                        poc_ents.len()
                    ),
                }
            }
        }
    }

    // 2. Encode fresh PocData and pack into entity attr chunks.
    let compressed = encode(&PocData::fresh());
    let chunks = pack(&compressed);

    info!(
        "[entity_poc] Writing {} entities ({} compressed bytes) into map",
        chunks.len(),
        compressed.len()
    );

    // 3. Find the starting entity index (append after existing entities).
    let start_id = {
        let state = get_server_state(server).read().unwrap();
        state.map.as_ref().map(|m| m.world.entities.len()).unwrap_or(0)
    };

    // 4. Push new entities into the in-memory map.
    {
        let mut state = get_server_state(server).write().unwrap();
        if let Some(map) = state.map.as_mut() {
            for attrs in &chunks {
                map.world.entities.push(Entity {
                    position: [0.0, 0.0, 0.0],
                    attrs: attrs.map(|v| v as i16),
                    ent_type: EntityType::GameSpecific(POC_ENT_TYPE),
                    extra_data: vec![],
                });
            }
        }
    }

    // 5. Broadcast N_EDITENT for every new entity so clients mirror the state.
    for (i, attrs) in chunks.iter().enumerate() {
        server.broadcast_chan1(&Client {
            client_num: cn as i32,
            packet: channel1::EditEnt {
                ent_id: (start_id + i) as i32,
                pos: Vec3 { x: 0.0, y: 0.0, z: 0.0 },
                type_: sauer_proto::data::EntityType::IBullets,
                attrs: attrs.map(|v| v as i32),
            },
        });
    }

    info!("[entity_poc] Done — entity IDs {start_id}..{}", start_id + chunks.len() - 1);
}
