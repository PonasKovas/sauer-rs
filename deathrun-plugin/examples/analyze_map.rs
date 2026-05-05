use clap::Parser;
use deathrun_plugin::map_state::MapState;
use deathrun_plugin::trap::{StoredTrap, TRAP_DOMAIN, TRAP_STORE_VERSION};
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

    let mut trap_vslots = 0;
    let mut trap_params = 0;
    let mut trap_bytes = 0;
    let mut empty_vslots = 0;

    for vs_opt in &raw_ogz.vslots {
        match vs_opt {
            Some(vs) => {
                let mut has_trap = false;
                for p in &vs.params {
                    let name_bytes = p.name.as_bytes();
                    // Magic prefix 1 indicates a trap chunk
                    if !name_bytes.is_empty() && name_bytes[0] == 1 {
                        has_trap = true;
                        trap_params += 1;
                        trap_bytes += 2 + name_bytes.len() + 16;
                    }
                }
                if has_trap {
                    trap_vslots += 1;
                }
            }
            None => {
                empty_vslots += 1;
            }
        }
    }

    println!("\n=== VSlot Store (Trap) Stats ===");
    println!("Empty VSlot indices: {}", empty_vslots);
    println!("VSlots containing trap data: {}", trap_vslots);
    println!("Total trap chunks (params): {}", trap_params);
    
    let trap_mb = trap_bytes as f64 / 1024.0 / 1024.0;
    let percent = if data.len() > 0 {
        (trap_bytes as f64 / data.len() as f64) * 100.0
    } else {
        0.0
    };
    
    println!("Trap Data Overhead: {:.2} MB ({:.1}% of file)", trap_mb, percent);

    // Load state to evaluate *valid* traps vs ghost bloat
    if let Ok(mut state) = MapState::from_bytes(&data) {
        if let Ok(stored) = deathrun_plugin::trap::vslot_store::read_all::<StoredTrap>(
            &state,
            TRAP_STORE_VERSION,
            TRAP_DOMAIN,
            |t| t.button_pos,
        ) {
            println!("Valid (Active/Loadable) Traps: {}", stored.len());
        }

        let compacted_data = state.to_ogz_bytes().expect("Failed to serialize map");
        println!("\n=== Compaction Simulation ===");
        println!("If unreferenced ghosts are cleared by the server, the map shrinks to: {:.2} MB", compacted_data.len() as f64 / 1024.0 / 1024.0);

        if args.strip_traps || args.output.is_some() {
            println!("\n=== Processing Output ===");
            
            if args.strip_traps {
                for vs_opt in &mut state.meta.vslots {
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