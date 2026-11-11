use clap::Parser;
use deathrun_plugin::trap::{EntChange, commands::payload_size, persist};
use sauer_map::Map;
use std::path::PathBuf;

#[derive(Parser, Debug)]
#[command(version, about = "Analyze an OGZ map for deathrun trap data.")]
struct Args {
    #[arg(help = "The input .ogz file")]
    input: PathBuf,

    #[arg(short, long, help = "Output cleaned map to this path")]
    output: Option<PathBuf>,

    #[arg(
        long,
        help = "Strip all trap data (old vslot store and the vault record) from the map's VSlots"
    )]
    strip_traps: bool,
}

fn is_trap_param(name: &[u8]) -> bool {
    // Old vslot store chunks start with 0x01; the vault record with "~drv".
    name.first() == Some(&1) || name.starts_with(b"~drv")
}

fn main() {
    let args = Args::parse();

    let data = std::fs::read(&args.input).expect("Failed to read input file");
    let raw_ogz = ogz_rs::OgzMap::parse(&data).expect("Failed to parse OGZ structure");

    println!("=== Map Analysis: {:?} ===", args.input);
    println!(
        "Total File Size: {:.2} MB",
        data.len() as f64 / 1024.0 / 1024.0
    );
    println!("World Size: {}", raw_ogz.world_size);
    println!("Entities: {}", raw_ogz.entities.len());
    println!("Total VSlot Array Size: {}", raw_ogz.vslots.len());

    let Ok(mut map) = Map::from_bytes(&data) else {
        println!("\n[WARNING] Could not parse the full octree. The map may be corrupted.");
        return;
    };

    let old_params: usize = map
        .vslots
        .iter()
        .flatten()
        .flat_map(|vs| &vs.props.params)
        .filter(|p| p.name.as_bytes().first() == Some(&1))
        .count();
    if old_params > 0 {
        println!("\nOld vslot-store trap chunks: {old_params} (not loaded anymore)");
    }

    let loaded = persist::load(&map);
    println!("\n=== Trap Vault ===");
    match loaded.vault {
        None => println!("No trap record found."),
        Some(v) => {
            println!(
                "Vault: {} cube at {:?}{}",
                v.size,
                v.origin,
                if v.is_at_far_corner(map.size) {
                    ""
                } else {
                    " (not at the far corner)"
                }
            );
            println!("Record generation: {}", loaded.generation);
            println!("Traps: {}", loaded.traps.len());
            for (i, t) in loaded.traps.iter().enumerate() {
                let geom = t.geom.as_ref().map_or("no geometry".to_string(), |g| {
                    let e = g.extent();
                    format!(
                        "{}x{}x{} at grid {}, {:.1} KB",
                        e[0],
                        e[1],
                        e[2],
                        g.clip_grid,
                        payload_size(&g.payload) as f64 / 1024.0
                    )
                });
                let added = t
                    .ents
                    .iter()
                    .filter(|c| matches!(c, EntChange::Added(_)))
                    .count();
                println!(
                    "  #{i} button {:?}: {geom}, {} entity changes ({added} created)",
                    t.button_pos,
                    t.ents.len()
                );
            }
        }
    }
    for w in &loaded.warnings {
        println!("[WARNING] {w}");
    }

    if args.strip_traps || args.output.is_some() {
        println!("\n=== Processing Output ===");
        if args.strip_traps {
            for vs in map.vslots.iter_mut().flatten() {
                vs.props.params.retain(|p| !is_trap_param(p.name.as_bytes()));
            }
            println!("Stripped all trap data from all VSlots.");
        }
        if let Some(out_path) = args.output {
            let out_data = map.to_ogz_bytes().expect("Failed to serialize");
            std::fs::write(&out_path, &out_data).expect("Failed to write output file");
            println!(
                "Saved output map to {:?} (New Size: {:.2} MB)",
                out_path,
                out_data.len() as f64 / 1024.0 / 1024.0
            );
        }
    }
}
