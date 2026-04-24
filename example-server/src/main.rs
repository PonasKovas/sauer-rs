use crate::config::Config;
use clap::Parser;
use deathrun_plugin::DeathRun;
use deathrun_plugin::options::DeathrunOptions;
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
        .server_desc(config.base.server_desc_cube())
        .server_auth(config.base.server_auth_cube())
        .has_password(config.base.has_password)
        .map_name(config.base.map_cube())
        .game_mode(config.base.mode)
        .master_mode(config.base.master_mode)
        .plugin(DeathRun::new(DeathrunOptions { is_edit_server: true }))
        .plugin(lua_plugin);

    if let Err(e) = server.run().await {
        error!("Error running the server: {e}");
    }
}
