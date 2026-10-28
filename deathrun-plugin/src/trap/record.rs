//! What the vault anchor stores: every trap's settings, where its geometry
//! sits on the shelf, and its entity changes. Geometry itself is real map
//! geometry on the shelf, not part of this record.

use super::ents::EntChange;
use flate2::{Compression, read::ZlibDecoder, write::ZlibEncoder};
use serde::{Deserialize, Serialize};
use std::io::{Read, Write};

/// Param-name tag of the trap database (see `sauer_map::param_blob`).
pub const TAG: &str = "drv";
const FORMAT: u8 = 1;
const MAX_RAW: u64 = 16 * 1024 * 1024;

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct VaultRecord {
    pub origin: [i32; 3],
    pub size: i32,
    pub traps: Vec<TrapRecord>,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct TrapRecord {
    pub button_pos: [i32; 3],
    pub button_size: i32,
    pub button_orient: u8,
    pub time: u32,
    pub timeout: u32,
    pub geom: Option<GeomRecord>,
    pub ents: Vec<EntChange>,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct GeomRecord {
    pub clip_pos: [i32; 3],
    pub clip_s: [i32; 3],
    pub clip_grid: i32,
    /// Relative to the vault origin.
    pub shelf: [i32; 3],
}

impl VaultRecord {
    pub fn to_bytes(&self) -> Vec<u8> {
        let raw = bitcode::serialize(self).expect("vault record serializes");
        let mut out = vec![FORMAT];
        let mut enc = ZlibEncoder::new(&mut out, Compression::best());
        enc.write_all(&raw).expect("in-memory write");
        enc.finish().expect("in-memory write");
        out
    }

    pub fn from_bytes(data: &[u8]) -> Result<Self, String> {
        let (&format, body) = data.split_first().ok_or("empty trap record")?;
        if format != FORMAT {
            return Err(format!("unknown trap record format {format}"));
        }
        let mut raw = Vec::new();
        ZlibDecoder::new(body)
            .take(MAX_RAW)
            .read_to_end(&mut raw)
            .map_err(|e| format!("corrupt trap record: {e}"))?;
        bitcode::deserialize(&raw).map_err(|e| format!("corrupt trap record: {e}"))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::trap::ents::EntRec;

    #[test]
    fn roundtrip() {
        let rec = VaultRecord {
            origin: [1536, 1536, 0],
            size: 512,
            traps: vec![TrapRecord {
                button_pos: [8, 16, 24],
                button_size: 8,
                button_orient: 3,
                time: 2000,
                timeout: 20000,
                geom: Some(GeomRecord {
                    clip_pos: [0, 0, 0],
                    clip_s: [2, 3, 4],
                    clip_grid: 16,
                    shelf: [129, 129, 129],
                }),
                ents: vec![EntChange::Added(EntRec {
                    ty: 1,
                    pos: [1.5, 2.0, 3.0],
                    attrs: [1, 2, 3, 4, 5],
                })],
            }],
        };
        assert_eq!(VaultRecord::from_bytes(&rec.to_bytes()).unwrap(), rec);
        assert!(VaultRecord::from_bytes(&[9, 1, 2]).is_err());
    }
}
