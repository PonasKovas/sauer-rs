use clap::Parser;
use sauer_map::Map;
use deathrun_plugin::trap::StoredTrap;
use std::path::PathBuf;

#[derive(Parser, Debug)]
#[command(
    version,
    about = "Analyze an OGZ map for VSlot Store (Trap) data and bloat."
)]
struct Args {
    #[arg(help = "The input .ogz file")]
    input: PathBuf,

    #[arg(short, long, help = "Output cleaned map to this path")]
    output: Option<PathBuf>,

    #[arg(
        long,
        help = "Strip all trap data from the map's VSlots (destructive rescue)"
    )]
    strip_traps: bool,
}

fn main() {
    let args = Args::parse();

    let data = std::fs::read(&args.input).expect("Failed to read input file");
    let raw_ogz = ogz_rs::OgzMap::parse(&data).expect("Failed to parse OGZ structure");

    println!("=== Map Analysis: {:?} ===", args.input);
    println!("Total File Size: {:.2} MB", data.len() as f64 / 1024.0 / 1024.0);
    println!("World Size: {}", raw_ogz.world_size);
    println!("Entities: {}", raw_ogz.entities.len());
    println!("Total VSlot Array Size: {}", raw_ogz.vslots.len());

    let mut empty_vslots = 0;
    for vs_opt in &raw_ogz.vslots {
        if vs_opt.is_none() {
            empty_vslots += 1;
        }
    }

    // Load state to evaluate *valid* traps vs ghost bloat
    if let Ok(mut state) = Map::from_bytes(&data) {
        if let Ok((entries, stats)) = state.db_scan::<StoredTrap>() {
            let mut valid_count = 0;
            for entry in &entries {
                let st = &entry.data;
                let bpos = st.button_pos;
                let valid = entry.instances.iter().any(|inst| {
                    inst.pos[0] >= bpos[0] && inst.pos[0] <= bpos[0] + 8 &&
                    inst.pos[1] >= bpos[1] && inst.pos[1] <= bpos[1] + 8 &&
                    inst.pos[2] >= bpos[2] && inst.pos[2] <= bpos[2] + 8
                });
                if valid {
                    valid_count += 1;
                }
            }
            println!("\n=== VSlot Store (Trap) Stats ===");
            println!("Empty VSlot indices: {}", empty_vslots);
            println!("Valid (Active/Loadable) Traps: {}", valid_count);
            println!("Total trap chunks (params): {}", stats.chunks);
            
            let comp_kb = stats.payload_compressed as f64 / 1024.0;
            let uncomp_kb = stats.payload_uncompressed as f64 / 1024.0;
            let overhead_mb = stats.structural_bytes as f64 / 1024.0 / 1024.0;
            let percent = if data.len() > 0 {
                (stats.payload_compressed as f64 / data.len() as f64) * 100.0
            } else {
                0.0
            };
            
            println!("Uncompressed Bitcode Size: {:.2} KB", uncomp_kb);
            println!("Compressed Zlib Size: {:.2} KB ({:.1}% of OGZ file)", comp_kb, percent);
            println!("VSlot Structural Memory Overhead: {:.2} MB", overhead_mb);
        }

        let compacted_data = state.to_ogz_bytes().expect("Failed to serialize map");
        println!("\n=== Compaction Simulation ===");
        println!("If unreferenced ghosts are cleared by the server, the map shrinks to: {:.2} MB", compacted_data.len() as f64 / 1024.0 / 1024.0);

        if args.strip_traps || args.output.is_some() {
            println!("\n=== Processing Output ===");
            
            if args.strip_traps {
                for vs_opt in &mut state.vslots {
                    if let Some(vs) = vs_opt {
                        vs.params.retain(|p| {
                            let nb = p.name.as_bytes();
                            nb.is_empty() || nb[0] != 1
                        });
                    }
                }
                println!("Stripped all trap data from all VSlots.");
            }

            if let Some(out_path) = args.output {
                let out_data = state.to_ogz_bytes().expect("Failed to serialize");
                std::fs::write(&out_path, &out_data).expect("Failed to write output file");
                println!("Saved output map to {:?} (New Size: {:.2} MB)", out_path, out_data.len() as f64 / 1024.0 / 1024.0);
            }
        }
    } else {
        println!("\n[WARNING] Could not parse the full octree. The map may be corrupted.");
    }
}
