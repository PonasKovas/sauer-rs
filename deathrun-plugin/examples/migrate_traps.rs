use clap::Parser;
use deathrun_plugin::trap::stored::StoredEntity;
use deathrun_plugin::trap::StoredTrap;
use flate2::read::ZlibDecoder;
use sauer_map::Map;
use std::collections::HashMap;
use std::io::Read;
use std::path::PathBuf;

#[derive(Parser, Debug)]
#[command(about = "Migrate an OGZ map's traps from the pre-material layout to the new layout.")]
struct Args {
    #[arg(help = "The input .ogz file with broken traps")]
    input: PathBuf,

    #[arg(help = "The output path for the fixed .ogz file")]
    output: PathBuf,
}

// === OLD STRUCT DEFINITIONS FOR BITCODE DESERIALIZATION ===

#[derive(serde::Serialize, serde::Deserialize, Clone)]
pub struct OldCube {
    pub edges: [u8; 12],
    pub textures: [u16; 6],
}

#[derive(serde::Serialize, serde::Deserialize, Clone)]
pub enum OldNode {
    Branch(Box<[OldNode; 8]>),
    Leaf(OldCube),
}

#[derive(serde::Serialize, serde::Deserialize, Clone)]
pub struct OldClipboardPayload {
    pub unpack_len: i32,
    pub sel: sauer_core::Selection,
    pub nodes: Vec<OldNode>,
    pub vslots: Vec<sauer_proto::data::clipboard::VSlotData>,
}

#[derive(serde::Serialize, serde::Deserialize, Clone)]
pub struct OldStoredTrap {
    pub button_pos: [i32; 3],
    pub button_size: i32,
    pub button_orient: u8,
    pub trigger_payload: Option<OldClipboardPayload>,
    pub trigger_entities: Vec<StoredEntity>,
    pub clip_pos: [i32; 3],
    pub clip_s: [i32; 3],
    pub clip_grid: i32,
    pub time: u32,
    pub timeout: u32,
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

fn convert_node(old: OldNode) -> sauer_core::Node {
    match old {
        OldNode::Branch(b) => sauer_core::Node::Branch(Box::new(b.map(convert_node))),
        OldNode::Leaf(c) => sauer_core::Node::Leaf(sauer_core::Cube {
            edges: c.edges,
            textures: c.textures,
            material: Default::default(),
        }),
    }
}

fn main() {
    let args = Args::parse();
    let data = std::fs::read(&args.input).expect("Failed to read input file");
    let mut map = Map::from_bytes(&data).expect("Failed to parse OGZ structure");

    let mut decoded_vslots = HashMap::new();

    // 1. Manually extract and deserialize old trap bitcode payloads
    for (vslot_idx, vs_opt) in map.vslots.iter().enumerate() {
        if let Some(vs) = vs_opt {
            let mut chunks = HashMap::new();
            let mut max_idx = 0;

            for p in &vs.params {
                let name_bytes = p.name.as_bytes();
                if let Some(idx) = decode_chunk_name(name_bytes) {
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

            if !valid
                || payload.len() < 6
                || &payload[0..2] != b"DR"
                || payload[2] != 3
                || payload[3] != 1
            {
                continue;
            }

            let zlib_len = u16::from_le_bytes([payload[4], payload[5]]) as usize;
            if payload.len() < 6 + zlib_len {
                continue;
            }

            let mut dec = ZlibDecoder::new(&payload[6..6 + zlib_len]);
            let mut raw = Vec::new();
            if dec.read_to_end(&mut raw).is_ok() {
                if let Ok(data) = bitcode::deserialize::<OldStoredTrap>(&raw) {
                    decoded_vslots.insert(vslot_idx as u16, data);
                }
            }
        }
    }

    println!("Found {} valid old traps. Migrating...", decoded_vslots.len());

    // 2. Convert old shapes to new StoredTrap structs
    let mut new_traps = Vec::new();
    for (_, old) in decoded_vslots {
        let trigger_payload = old.trigger_payload.map(|p| sauer_proto::data::ClipboardPayload {
            unpack_len: p.unpack_len,
            sel: p.sel,
            nodes: p.nodes.into_iter().map(convert_node).collect(),
            vslots: p.vslots,
        });

        new_traps.push(StoredTrap {
            button_pos: old.button_pos,
            button_size: old.button_size,
            button_orient: old.button_orient,
            trigger_payload,
            trigger_entities: old.trigger_entities,
            clip_pos: old.clip_pos,
            clip_s: old.clip_s,
            clip_grid: old.clip_grid,
            time: old.time,
            timeout: old.timeout,
        });
    }

    // 3. Strip the old, corrupt VSlot structural payloads
    for vs_opt in &mut map.vslots {
        if let Some(vs) = vs_opt {
            vs.params.retain(|p| {
                let nb = p.name.as_bytes();
                nb.is_empty() || nb[0] != 1
            });
        }
    }
    map.compact_vslots();

    // 4. Rebuild all geometry buttons & apply new healthy VSlot payloads
    let mut loaded_traps = Vec::new();
    for st in new_traps {
        if let Some(trap) = st.clone().into_trap() {
            loaded_traps.push(trap);
        }
    }

    deathrun_plugin::trap::placement::rebuild_all_trap_buttons(&mut map, &loaded_traps);
    for trap in &loaded_traps {
        let packets = deathrun_plugin::trap::stored::generate_trap_vslot_packets(
            &StoredTrap::from_trap(trap),
        );
        for p in &packets {
            map.apply_vslots(&p.sel, p.all_faces, p.delta != 0, &p.extra_data);
        }
    }

    let out_data = map.to_ogz_bytes().expect("Failed to serialize map");
    std::fs::write(&args.output, out_data).expect("Failed to save map");
    println!("Successfully migrated {} traps to {:?}", loaded_traps.len(), args.output);
}