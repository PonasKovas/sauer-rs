//! Repo chores, run with `cargo xtask <task>`.

use std::path::Path;

mod lua_docs;
mod proto_docs;

fn main() {
    let root = Path::new(env!("CARGO_MANIFEST_DIR")).parent().unwrap();
    let mut args = std::env::args().skip(1);
    match args.next().as_deref() {
        // CI writes them next to the rustdoc output instead.
        Some("lua-docs") => {
            let out = args.next().map_or(root.join("lua-api"), Into::into);
            lua_docs::run(root, &out)
        }
        Some("proto-docs") => proto_docs::run(
            &root.join("sauer-proto/src"),
            &root.join("sauer-proto-ref.md"),
        ),
        _ => {
            eprintln!("usage: cargo xtask (lua-docs [out_dir] | proto-docs)");
            std::process::exit(1);
        }
    }
}
