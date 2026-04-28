use clap::Parser;
use cube_string::IntoCubeStringLossy;
use deathrun_plugin::{DeathrunEdit, DeathrunPlay, play::PlayMaps};
use sauer_plugin_lua::{LuaPlugin, LuaPluginOptions};
use sauer_proto::data::MasterMode;
use sauer_server::{Server, ServerBuilder, plugin::Plugin};
use serde::{Deserialize, Serialize};
use std::{
    collections::HashSet,
    net::SocketAddr,
    path::PathBuf,
    sync::{Arc, RwLock},
};
use tokio::fs;
use tracing::{error, info, warn};

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

#[derive(Debug, Clone, Serialize, Deserialize)]
struct Config {
    pub bind_addr: SocketAddr,
    pub max_peers: usize,
    pub plugin_dir: PathBuf,
    pub map_dir: Option<PathBuf>,
    #[serde(default)]
    pub base: BaseConfig,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct BaseConfig {
    #[serde(default = "default_server_desc")]
    pub server_desc: String,
    #[serde(default)]
    pub server_auth: String,
    #[serde(default)]
    pub has_password: bool,
    #[serde(default = "default_master_mode")]
    pub master_mode: MasterMode,
}

fn default_server_desc() -> String {
    "deathrun".into()
}

fn default_master_mode() -> MasterMode {
    MasterMode::Locked
}

impl Default for BaseConfig {
    fn default() -> Self {
        Self {
            server_desc: default_server_desc(),
            server_auth: String::new(),
            has_password: false,
            master_mode: MasterMode::Open,
        }
    }
}

impl Plugin for BaseConfig {
    async fn init(&self, server: &mut Server) {
        *server.server_desc.write().unwrap() = self.server_desc.as_str().lossy_cubestring();
        *server.server_auth.write().unwrap() = self.server_auth.as_str().lossy_cubestring();
        *server.has_password.write().unwrap() = self.has_password;
        *server.master_mode.write().unwrap() = self.master_mode;
    }
}

impl Config {
    async fn load(args: &Args) -> anyhow::Result<Self> {
        let contents = fs::read_to_string(&args.config).await?;
        Ok(toml::from_str(&contents)?)
    }
}

async fn load_maps_from_dir(map_dir: &PathBuf, play: &DeathrunPlay) {
    let mut entries = match fs::read_dir(map_dir).await {
        Ok(e) => e,
        Err(e) => {
            warn!("Failed to read map directory {map_dir:?}: {e}");
            return;
        }
    };
    while let Ok(Some(entry)) = entries.next_entry().await {
        let path = entry.path();
        if path.extension().map_or(false, |e| e == "ogz") {
            let name = path.file_stem().unwrap().to_string_lossy().to_string();
            match fs::read(&path).await {
                Ok(data) => match play.add_map(name.clone(), &data) {
                    Ok(()) => info!("Loaded map '{name}'"),
                    Err(e) => error!("Failed to load map '{name}': {e}"),
                },
                Err(e) => error!("Failed to read {path:?}: {e}"),
            }
        }
    }
}

async fn poll_map_dir(map_dir: PathBuf, maps: Arc<RwLock<PlayMaps>>) {
    let mut known: HashSet<String> = maps.read().unwrap().maps.keys().cloned().collect();

    loop {
        tokio::time::sleep(std::time::Duration::from_secs(2)).await;

        let mut current = HashSet::new();
        if let Ok(mut entries) = fs::read_dir(&map_dir).await {
            while let Ok(Some(entry)) = entries.next_entry().await {
                let path = entry.path();
                if path.extension().map_or(false, |e| e == "ogz") {
                    if let Some(name) = path.file_stem().and_then(|s| s.to_str()) {
                        current.insert(name.to_string());
                    }
                }
            }
        }

        for name in &current {
            if !known.contains(name) {
                let path = map_dir.join(format!("{name}.ogz"));
                match fs::read(&path).await {
                    Ok(data) => match maps.write().unwrap().add_map(name.clone(), &data) {
                        Ok(()) => info!("Map dir: loaded '{name}'"),
                        Err(e) => error!("Map dir: failed to load '{name}': {e}"),
                    },
                    Err(e) => error!("Map dir: failed to read '{name}': {e}"),
                }
            }
        }

        for name in known.difference(&current).cloned().collect::<Vec<_>>() {
            maps.write().unwrap().remove_map(&name);
            info!("Map dir: removed '{name}'");
        }

        known = current;
    }
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

    match args.mode {
        ServerMode::Edit => {
            let server = ServerBuilder::new()
                .bind_addr(config.bind_addr)
                .max_peers(config.max_peers)
                .plugin(config.base)
                .plugin(DeathrunEdit::new())
                .plugin(lua_plugin);

            if let Err(e) = server.run().await {
                error!("Error running the server: {e}");
            }
        }
        ServerMode::Play => {
            let play = DeathrunPlay::new();

            if let Some(ref map_dir) = config.map_dir {
                load_maps_from_dir(map_dir, &play).await;
                tokio::spawn(poll_map_dir(map_dir.clone(), play.maps_handle()));
            } else {
                warn!("No map_dir in config; starting play mode with no maps");
            }

            let server = ServerBuilder::new()
                .bind_addr(config.bind_addr)
                .max_peers(config.max_peers)
                .plugin(config.base)
                .plugin(play)
                .plugin(lua_plugin);

            if let Err(e) = server.run().await {
                error!("Error running the server: {e}");
            }
        }
    }
}
