use clap::Parser;
use deathrun_plugin::map_state::MapState;
use deathrun_plugin::trap::{StoredTrap, TRAP_DOMAIN};
use flate2::read::ZlibDecoder;
use std::io::Read;
use std::path::PathBuf;

#[derive(Parser, Debug)]
#[command(
    version,
    about = "Convert an OGZ map with legacy entity_store traps to vslot_store"
)]
struct Args {
    #[arg(help = "The input .ogz file")]
    input: PathBuf,

    #[arg(help = "The output .ogz file")]
    output: PathBuf,

    #[arg(long, help = "Purge legacy traps without attempting to convert them (useful for corrupt files)")]
    purge_only: bool,
}

fn decompress<T: serde::de::DeserializeOwned>(compressed: &[u8]) -> Result<T, String> {
    let mut dec = ZlibDecoder::new(compressed);
    let mut raw = Vec::new();
    dec.read_to_end(&mut raw).map_err(|e| e.to_string())?;
    bitcode::deserialize(&raw).map_err(|e| e.to_string())
}

fn unpack(entities: &[&ogz_rs::data::Entity]) -> Result<(u8, Vec<u8>), String> {
    let mut stream: Vec<u8> = Vec::with_capacity(entities.len() * 10);
    for ent in entities {
        for &a in &ent.attrs {
            stream.extend_from_slice(&(a as u16).to_le_bytes());
        }
    }
    if stream.len() < 3 {
        return Err("stream too short".into());
    }
    let version = stream[0];
    let len = u16::from_le_bytes([stream[1], stream[2]]) as usize;
    if stream.len() < 3 + len {
        return Err("truncated data".into());
    }
    Ok((version, stream[3..3 + len].to_vec()))
}

fn is_store_entity(ent: &ogz_rs::data::Entity, domain_id: u8) -> bool {
    if !matches!(ent.ent_type, ogz_rs::data::EntityType::Particles) {
        return false;
    }
    if (ent.position[0] - (-10000.0)).abs() > 0.1 || (ent.position[1] - (-10000.0)).abs() > 0.1 {
        return false;
    }
    let expected_z = -10000.0 - (domain_id as f32);
    (ent.position[2] - expected_z).abs() < 0.1
        || (domain_id == 1 && (ent.position[2] - (-10000.0)).abs() < 0.1)
}

fn main() {
    let args = Args::parse();

    let input_data = std::fs::read(&args.input).expect("Failed to read input file");
    let mut map_state = MapState::from_bytes(&input_data).expect("Failed to parse map");

    let mut store_ents: Vec<(usize, &ogz_rs::data::Entity)> = map_state
        .world
        .entities
        .iter()
        .enumerate()
        .filter(|(_, e)| is_store_entity(e, TRAP_DOMAIN))
        .collect();

    if store_ents.is_empty() {
        println!("No legacy traps found in the provided map.");
        return;
    }

    // Sort to extract in correct order
    store_ents.sort_by_key(|&(i, _)| i);

    if args.purge_only {
        println!("Purging {} legacy trap entities without converting.", store_ents.len());
        
        for ent in &mut map_state.world.entities {
            if is_store_entity(ent, TRAP_DOMAIN) || is_store_entity(ent, 2) || ent.position[2] < -9000.0 {
                ent.ent_type = ogz_rs::data::EntityType::Empty;
                ent.attrs = [0; 5];
                ent.position = [0.0; 3];
            }
        }
    } else {
        let refs: Vec<_> = store_ents.iter().map(|(_, e)| *e).collect();
        let (_, compressed) = unpack(&refs).expect("Failed to unpack legacy entities");
        let stored_traps: Vec<StoredTrap> =
            decompress(&compressed).expect("Failed to deserialize legacy traps");

        println!("Found {} legacy traps.", stored_traps.len());

        // 1. Clear legacy trap data (domain 1 and 2, plus generic negative coords)
        for ent in &mut map_state.world.entities {
            if is_store_entity(ent, TRAP_DOMAIN) || is_store_entity(ent, 2) || ent.position[2] < -9000.0
            {
                ent.ent_type = ogz_rs::data::EntityType::Empty;
                ent.attrs = [0; 5];
                ent.position = [0.0; 3];
            }
        }

        // 2. Restore map to clean state (in case it was saved active)
        for st in &stored_traps {
            st.apply_reset_to_world(&mut map_state.world);
        }

        let loaded_traps: Vec<_> = stored_traps
            .into_iter()
            .filter_map(|st| st.into_trap())
            .collect();

        // 3. Rebuild trap geometries and bake them into VSlots
        deathrun_plugin::trap::placement::rebuild_all_trap_buttons(&mut map_state, &loaded_traps);

        for trap in &loaded_traps {
            let st = deathrun_plugin::trap::StoredTrap::from_trap(trap);
            let packets = deathrun_plugin::trap::stored::generate_trap_vslot_packets(&st);
            for p in &packets {
                map_state.handle_edit_v_slot(p);
            }
        }
    }

    // 4. Save
    let out_data = map_state.to_ogz_bytes().expect("Failed to serialize map");
    std::fs::write(&args.output, out_data).expect("Failed to write output file");

    println!("Operation successful! Wrote to {:?}", args.output);
}
