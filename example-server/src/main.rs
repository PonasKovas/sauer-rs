use crate::config::Config;
use clap::Parser;
use deathrun_plugin::{DeathrunEdit, DeathrunPlay};
use sauer_plugin_lua::LuaPlugin;
use sauer_plugin_lua::LuaPluginOptions;
use sauer_server::ServerBuilder;
use std::path::PathBuf;
use tracing::error;

mod config;

#[derive(Parser, Debug)]
#[command(version, about, long_about = None)]
struct Args {
    /// Config path
    #[arg(short, long, default_value = "config.toml.local")]
    config: PathBuf,
}

#[tokio::main]
async fn main() {
    sauer_server::utils::init_default_tracing();

    let args = Args::parse();

    let config = match Config::load(&args).await {
        Ok(x) => x,
        Err(e) => {
            error!("Error loading config: {e}");
            return;
        }
    };

    let mut lua_options = LuaPluginOptions::default();
    lua_options.plugin_directory = config.plugin_dir.clone();

    let lua_plugin = match LuaPlugin::load(&lua_options).await {
        Ok(x) => x,
        Err(e) => {
            error!("Error loading lua plugins: {e}");
            return;
        }
    };

    let server = ServerBuilder::new()
        .bind_addr(config.bind_addr)
        .max_peers(config.max_peers)
        .plugin(config.base)
        .plugin(DeathrunEdit::new())
        .plugin(DeathrunPlay::new(Default::default()))
        .plugin(lua_plugin);

    if let Err(e) = server.run().await {
        error!("Error running the server: {e}");
    }
}
