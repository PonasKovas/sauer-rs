//! Stores a byte blob inside VSlot shader-param *names*.
//!
//! Sauerbraten keeps vslot params of any face in the `.ogz`, and param names
//! survive the network and the file byte-for-byte (unlike the float values,
//! which `-ffast-math`/x87 builds may alter). Names are capped at 259 bytes by
//! the engine, so the blob is base64 encoded and split across several params:
//!
//! `~{tag}{generation:08x}{index:04x}{count:04x}{base64 chunk}`
//!
//! The leading `~` can never be a real shader uniform. When a map holds
//! several copies (an old anchor pasted somewhere), the newest complete
//! generation wins.

use crate::Map;
use cube_string::CubeString;
use sauer_proto::data::{VSlotParam, VSlotProps};
use std::collections::BTreeMap;

const CHUNK_CHARS: usize = 224;
const HEADER_CHARS: usize = 8 + 4 + 4;

/// Builds the vslot delta (params only) that carries `data`.
pub fn encode(tag: &str, generation: u32, data: &[u8]) -> VSlotProps {
    let text = base64_encode(data);
    let chunks: Vec<&[u8]> = text.as_bytes().chunks(CHUNK_CHARS).collect();
    let count = chunks.len().max(1);
    assert!(count <= 0xFFFF, "param blob too large");

    let mut params = Vec::with_capacity(count);
    for i in 0..count {
        let chunk = chunks.get(i).copied().unwrap_or(&[]);
        let mut name = format!("~{tag}{generation:08x}{i:04x}{count:04x}").into_bytes();
        name.extend_from_slice(chunk);
        params.push(VSlotParam {
            name: CubeString::from_bytes(name),
            values: [0.0; 4],
        });
    }
    VSlotProps {
        params,
        ..Default::default()
    }
}

/// Finds the newest complete blob with `tag` among all of the map's vslots.
pub fn find_latest(map: &Map, tag: &str) -> Option<(u32, Vec<u8>)> {
    let prefix = format!("~{tag}");
    // generation -> (count, index -> chunk)
    let mut found: BTreeMap<u32, (usize, BTreeMap<usize, Vec<u8>>)> = BTreeMap::new();

    for vs in map.vslots.iter().flatten() {
        for p in &vs.props.params {
            let name = p.name.as_bytes();
            let Some(rest) = name.strip_prefix(prefix.as_bytes()) else {
                continue;
            };
            if rest.len() < HEADER_CHARS {
                continue;
            }
            let (Some(generation), Some(index), Some(count)) = (
                parse_hex(&rest[0..8]),
                parse_hex(&rest[8..12]),
                parse_hex(&rest[12..16]),
            ) else {
                continue;
            };
            let entry = found
                .entry(generation)
                .or_insert_with(|| (count as usize, BTreeMap::new()));
            if entry.0 == count as usize {
                entry
                    .1
                    .insert(index as usize, rest[HEADER_CHARS..].to_vec());
            }
        }
    }

    for (generation, (count, chunks)) in found.into_iter().rev() {
        if chunks.len() != count || chunks.keys().next_back() != Some(&(count - 1)) {
            continue;
        }
        let text: Vec<u8> = chunks.into_values().flatten().collect();
        if let Some(data) = base64_decode(&text) {
            return Some((generation, data));
        }
    }
    None
}

fn parse_hex(s: &[u8]) -> Option<u32> {
    u32::from_str_radix(std::str::from_utf8(s).ok()?, 16).ok()
}

const B64: &[u8; 64] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/";

fn base64_encode(data: &[u8]) -> String {
    let mut out = String::with_capacity(data.len().div_ceil(3) * 4);
    for chunk in data.chunks(3) {
        let b = [
            chunk[0],
            *chunk.get(1).unwrap_or(&0),
            *chunk.get(2).unwrap_or(&0),
        ];
        let n = (b[0] as u32) << 16 | (b[1] as u32) << 8 | b[2] as u32;
        for i in 0..4 {
            if i <= chunk.len() {
                out.push(B64[(n >> (18 - 6 * i)) as usize & 63] as char);
            } else {
                out.push('=');
            }
        }
    }
    out
}

fn base64_decode(text: &[u8]) -> Option<Vec<u8>> {
    if !text.len().is_multiple_of(4) {
        return None;
    }
    let mut out = Vec::with_capacity(text.len() / 4 * 3);
    for quad in text.chunks(4) {
        let mut n = 0u32;
        let mut pad = 0;
        for &c in quad {
            let v = match c {
                b'=' => {
                    pad += 1;
                    0
                }
                _ if pad > 0 => return None,
                _ => B64.iter().position(|&x| x == c)? as u32,
            };
            n = n << 6 | v;
        }
        if pad > 2 {
            return None;
        }
        let bytes = [(n >> 16) as u8, (n >> 8) as u8, n as u8];
        out.extend_from_slice(&bytes[..3 - pad]);
    }
    Some(out)
}

#[cfg(test)]
mod tests {
    use super::*;
    use ogz_rs::data::VSlot;

    fn map_with(deltas: &[VSlotProps]) -> Map {
        let mut map = Map::new_empty(1024);
        for d in deltas {
            map.vslots.push(Some(VSlot {
                prev: 1,
                props: VSlotProps {
                    params: d.params.clone(),
                    ..Default::default()
                },
            }));
        }
        map
    }

    #[test]
    fn base64_roundtrip() {
        for len in 0..10 {
            let data: Vec<u8> = (0..len).map(|i| (i * 37 + 200) as u8).collect();
            assert_eq!(
                base64_decode(base64_encode(&data).as_bytes()).unwrap(),
                data
            );
        }
    }

    #[test]
    fn names_fit_engine_limit_and_roundtrip() {
        let data: Vec<u8> = (0..5000u32).map(|i| (i * 7919) as u8).collect();
        let delta = encode("t", 3, &data);
        assert!(delta.params.len() > 1);
        assert!(delta.params.iter().all(|p| p.name.len() < 259));
        assert!(delta.params.iter().all(|p| !p.name.as_bytes().contains(&0)));
        assert_eq!(find_latest(&map_with(&[delta]), "t"), Some((3, data)));
    }

    #[test]
    fn newest_complete_generation_wins() {
        let old = encode("t", 1, b"old");
        let new = encode("t", 2, b"new");
        let mut broken = encode("t", 9, &[1; 600]);
        broken.params.pop();
        let map = map_with(&[old, broken, new]);
        assert_eq!(find_latest(&map, "t"), Some((2, b"new".to_vec())));
        assert_eq!(find_latest(&map, "other"), None);
    }

    #[test]
    fn empty_blob() {
        let map = map_with(&[encode("t", 0, b"")]);
        assert_eq!(find_latest(&map, "t"), Some((0, vec![])));
    }
}
