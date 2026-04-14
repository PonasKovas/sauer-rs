use std::path::PathBuf;

fn main() {
    let manifest_dir = PathBuf::from(std::env::var("CARGO_MANIFEST_DIR").unwrap());
    let enet = manifest_dir.join("enet");

    let c_sources = [
        "callbacks.c",
        "compress.c",
        "host.c",
        "list.c",
        "packet.c",
        "peer.c",
        "protocol.c",
        "unix.c",
    ];

    cc::Build::new()
        .include(enet.join("include"))
        // Prevent unix.c from redefining socklen_t, which conflicts with
        // the system header definition on musl (and newer glibc).
        .define("HAS_SOCKLEN_T", "1")
        .files(c_sources.iter().map(|s| enet.join(s)))
        .file(manifest_dir.join("c_helper.c"))
        .flag("-w") // 👈 disable all warnings
        .compile("enet_c");

    println!("cargo:rerun-if-changed=c_helper.c");
    println!("cargo:rerun-if-changed=enet/");
}
