//! The deathrun server, as deployed: `--mode edit` for building maps and
//! traps, `--mode play` for playing them. Settings come from a TOML file.

use clap::Parser;
use deathrun::play::{PlayConfig, map_dir::MapDir};
use deathrun::{DeathrunEdit, DeathrunPlay};
use sauer_plugin_lua::{LuaPlugin, LuaPluginOptions};
use sauer_server::{ServerBuilder, Settings, plugin::Plugin};
use serde::Deserialize;
use std::{net::SocketAddr, path::PathBuf};
use telegram_plugin::TelegramConfig;
use tracing::{error, warn};

#[derive(clap::ValueEnum, Clone, Debug)]
enum ServerMode {
    Edit,
    Play,
}

#[derive(Parser, Debug)]
#[command(version, about, long_about = None)]
struct Args {
    #[arg(short, long, default_value = "config.toml.local")]
    config: PathBuf,
    #[arg(long, value_enum, default_value = "edit")]
    mode: ServerMode,
}

#[derive(Debug, Deserialize)]
struct Config {
    bind_addr: SocketAddr,
    max_peers: usize,
    plugin_dir: PathBuf,
    /// Play mode's maps.
    map_dir: Option<PathBuf>,
    #[serde(default)]
    base: Settings,
    #[serde(default)]
    deathrun_play: PlayConfig,
    #[serde(default)]
    deathrun: DeathrunConfig,
    #[serde(default)]
    telegram: TelegramConfig,
}

/// Settings for both modes.
#[derive(Debug, Default, Deserialize)]
struct DeathrunConfig {
    /// Shown in every welcome wall and at the end of #help, e.g. where to send
    /// new maps. Sauer color codes work ("\f2green").
    #[serde(default)]
    notes: Vec<String>,
}

#[tokio::main]
async fn main() {
    sauer_server::utils::init_default_tracing();
    let args = Args::parse();
    let config: Config = match std::fs::read_to_string(&args.config)
        .map_err(anyhow::Error::from)
        .and_then(|s| Ok(toml::from_str(&s)?))
    {
        Ok(c) => c,
        Err(e) => {
            error!("Error loading config: {e}");
            return;
        }
    };

    let mut base = config.base.clone();
    base.server_desc.get_or_insert_with(|| "deathrun".into());
    base.updatemaster.get_or_insert(false);

    match args.mode {
        ServerMode::Edit => {
            let edit = DeathrunEdit::new().with_notes(config.deathrun.notes.clone());
            run(&config, base, edit).await;
        }
        ServerMode::Play => {
            let play = DeathrunPlay::new(config.deathrun_play.clone())
                .with_notes(config.deathrun.notes.clone());
            match config.map_dir.clone() {
                Some(dir) => {
                    let maps = MapDir::open(dir, play.maps_handle()).await;
                    tokio::spawn(maps.watch());
                }
                None => warn!("No map_dir in config; starting play mode with no maps"),
            }
            run(&config, base, play).await;
        }
    }
}

async fn run(config: &Config, base: Settings, deathrun: impl Plugin) {
    let mut lua_options = LuaPluginOptions::default();
    lua_options.plugin_directory = config.plugin_dir.clone();
    let lua = match LuaPlugin::load(&lua_options).await {
        Ok(x) => x,
        Err(e) => {
            error!("Error loading lua plugins: {e}");
            return;
        }
    };
    // Telegram after Lua, so it only relays chat Lua didn't cancel.
    let server = ServerBuilder::new()
        .bind_addr(config.bind_addr)
        .max_peers(config.max_peers)
        .plugin(base)
        .plugin(deathrun)
        .plugin(lua)
        .plugin(config.telegram.bridge());
    if let Err(e) = server.run().await {
        error!("Error running the server: {e}");
    }
}
