use std::env;
use std::fs;
use std::process;

// Assuming the crate is named `ogz-rs` in Cargo.toml
use ogz_rs::OgzMap;

fn main() {
    // 1. Parse command line arguments
    let args: Vec<String> = env::args().collect();
    if args.len() < 2 {
        eprintln!("Usage: {} <path_to_map.ogz>", args[0]);
        process::exit(1);
    }

    let file_path = &args[1];
    println!("Reading file: {}", file_path);

    // 2. Read file to memory
    let original_data = fs::read(file_path).unwrap_or_else(|e| {
        eprintln!("Failed to read file on disk: {}", e);
        process::exit(1);
    });

    println!("File loaded ({} bytes). Parsing...", original_data.len());

    // 3. Parse the map
    let map1 = match OgzMap::from_bytes(&original_data) {
        Ok(m) => m,
        Err(e) => {
            eprintln!("Failed to parse map: {}", e);
            process::exit(1);
        }
    };

    println!("Parsed successfully! Map version: {}", map1.version);

    // If the map is an older legacy version, we can't do the roundtrip
    // because our crate serializes strictly to Version 33 modern formats.
    if map1.version < 32 {
        println!(
            "Map is version {}, which uses legacy structures. Skipping roundtrip serialization test.",
            map1.version
        );
        return;
    }

    println!("Serializing map back to memory...");

    // 4. Serialize back to memory buffer
    let serialized_data = match map1.to_bytes() {
        Ok(b) => b,
        Err(e) => {
            eprintln!("Failed to serialize map: {}", e);
            process::exit(1);
        }
    };

    println!(
        "Serialized successfully ({} bytes). Deserializing roundtrip...",
        serialized_data.len()
    );

    // 5. Parse the newly serialized buffer
    let map2 = match OgzMap::from_bytes(&serialized_data) {
        Ok(m) => m,
        Err(e) => {
            eprintln!("Failed to deserialize roundtrip data: {}", e);
            process::exit(1);
        }
    };

    println!("Deserialized successfully! Comparing equality...");

    // 6. Compare for equality
    if map1 == map2 {
        println!("Success! The maps are 100% identical after the roundtrip.");
    } else {
        eprintln!("Mismatch! Map data changed during the serialization roundtrip.");

        // Optional: If you want to see what broke, you could write both to disk to diff them
        // fs::write("map1_debug.txt", &repr1).unwrap();
        // fs::write("map2_debug.txt", &repr2).unwrap();
        // eprintln!("Wrote debug dumps to map1_debug.txt and map2_debug.txt for comparison.");

        process::exit(1);
    }
}
