//! Repo chores, run with `cargo xtask <task>`.

use std::path::Path;

mod lua_docs;

fn main() {
    let root = Path::new(env!("CARGO_MANIFEST_DIR")).parent().unwrap();
    match std::env::args().nth(1).as_deref() {
        Some("lua-docs") => lua_docs::run(root, &root.join("lua-api")),
        _ => {
            eprintln!("usage: cargo xtask lua-docs");
            std::process::exit(1);
        }
    }
}
