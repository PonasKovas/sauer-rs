use clap::Parser;
use cube_string::IntoCubeStringLossy;
use deathrun_plugin::{
    DeathrunEdit, DeathrunPlay,
    play::{PlayConfig, PlayMaps},
};
use sauer_plugin_lua::{LuaPlugin, LuaPluginOptions};
use sauer_proto::data::MasterMode;
use sauer_server::{Server, ServerBuilder, plugin::Plugin};
use serde::{Deserialize, Serialize};
use std::{
    collections::{HashMap, HashSet},
    net::SocketAddr,
    path::PathBuf,
    sync::{Arc, RwLock},
};
use telegram_plugin::TelegramBridge;
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
    #[serde(default)]
    pub deathrun_play: PlayConfig,
    #[serde(default)]
    pub telegram: TelegramConfig,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct TelegramConfig {
    #[serde(default)]
    pub enabled: bool,
    #[serde(default)]
    pub token: String,
    #[serde(default)]
    pub chat_id: i64,
}

impl Default for TelegramConfig {
    fn default() -> Self {
        Self {
            enabled: false,
            token: String::new(),
            chat_id: 0,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct BaseConfig {
    #[serde(default = "default_server_desc")]
    pub server_desc: String,
    #[serde(default)]
    pub server_auth: String,
    #[serde(default)]
    pub has_password: bool,
    /// Password for claiming admin with `/setmaster PASSWORD`. Empty disables admin.
    #[serde(default)]
    pub admin_pass: String,
    #[serde(default = "default_master_mode")]
    pub master_mode: MasterMode,
    #[serde(default = "default_updatemaster")]
    pub updatemaster: bool,
}

fn default_server_desc() -> String {
    "deathrun".into()
}

fn default_master_mode() -> MasterMode {
    MasterMode::Locked
}

fn default_updatemaster() -> bool {
    false
}

impl Default for BaseConfig {
    fn default() -> Self {
        Self {
            server_desc: default_server_desc(),
            server_auth: String::new(),
            has_password: false,
            admin_pass: String::new(),
            master_mode: MasterMode::Open,
            updatemaster: default_updatemaster(),
        }
    }
}

impl Plugin for BaseConfig {
    async fn init(&self, server: &mut Server) {
        *server.server_desc.write().unwrap() = self.server_desc.as_str().lossy_cubestring();
        *server.server_auth.write().unwrap() = self.server_auth.as_str().lossy_cubestring();
        *server.has_password.write().unwrap() = self.has_password;
        *server.admin_pass.write().unwrap() = self.admin_pass.clone();
        if self.admin_pass.is_empty() {
            tracing::info!("admin password not set, /setmaster PASSWORD is off");
        } else {
            tracing::info!("admin password set");
        }
        *server.master_mode.write().unwrap() = self.master_mode;
        *server.update_master.write().unwrap() = self.updatemaster;
    }
}

impl Config {
    async fn load(args: &Args) -> anyhow::Result<Self> {
        let contents = fs::read_to_string(&args.config).await?;
        Ok(toml::from_str(&contents)?)
    }
}

use std::time::SystemTime;

async fn load_initial_maps(map_dir: &PathBuf, play: &DeathrunPlay) -> HashMap<String, SystemTime> {
    let mut known_files = HashMap::new();
    let Ok(mut entries) = fs::read_dir(map_dir).await else {
        warn!("Failed to read map directory {map_dir:?}");
        return known_files;
    };

    while let Ok(Some(entry)) = entries.next_entry().await {
        let path = entry.path();
        if path.extension().map_or(false, |e| e == "ogz") {
            if let Some(name) = path.file_stem().and_then(|s| s.to_str()) {
                let name_str = name.to_string();
                if let Ok(data) = fs::read(&path).await {
                    match play.add_map(name_str.clone(), &data) {
                        Ok(()) => {
                            info!("Loaded map '{name_str}'");
                            if let Ok(metadata) = entry.metadata().await {
                                if let Ok(modified) = metadata.modified() {
                                    known_files.insert(name_str, modified);
                                }
                            }
                        }
                        Err(e) => error!("Failed to load map '{name_str}': {e}"),
                    }
                }
            }
        }
    }
    known_files
}

async fn sync_map_dir(
    map_dir: PathBuf,
    maps: Arc<RwLock<PlayMaps>>,
    mut known_files: HashMap<String, SystemTime>,
) {
    use notify::{Event, RecursiveMode, Watcher};

    let (tx, mut rx) = tokio::sync::mpsc::unbounded_channel::<()>();

    let tx_clone = tx.clone();
    let mut watcher = notify::recommended_watcher(move |res: notify::Result<Event>| {
        if res.is_ok() {
            let _ = tx_clone.send(());
        }
    })
    .expect("Failed to initialize map watcher");

    watcher
        .watch(&map_dir, RecursiveMode::NonRecursive)
        .expect("Failed to watch map directory");

    let mut interval = tokio::time::interval(std::time::Duration::from_secs(5));

    loop {
        tokio::select! {
            _ = interval.tick() => {}
            _ = rx.recv() => {
                // Debounce rapid events from cloud syncs or compilers
                tokio::time::sleep(std::time::Duration::from_millis(250)).await;
                // Drain any queued events that arrived during the sleep
                while let Ok(_) = rx.try_recv() {}
            }
        }

        let mut current_files = HashSet::new();

        if let Ok(mut entries) = fs::read_dir(&map_dir).await {
            while let Ok(Some(entry)) = entries.next_entry().await {
                let path = entry.path();
                if path.extension().map_or(false, |e| e == "ogz") {
                    if let Some(name) = path.file_stem().and_then(|s| s.to_str()) {
                        let name_str = name.to_string();
                        current_files.insert(name_str.clone());

                        let Ok(metadata) = entry.metadata().await else {
                            continue;
                        };
                        let Ok(modified) = metadata.modified() else {
                            continue;
                        };

                        let needs_update = match known_files.get(&name_str) {
                            Some(&last_mod) => modified > last_mod,
                            None => true,
                        };

                        if needs_update {
                            match fs::read(&path).await {
                                Ok(data) => {
                                    match maps.write().unwrap().add_map(name_str.clone(), &data) {
                                        Ok(()) => {
                                            info!("Map dir: loaded/updated '{name_str}'");
                                            known_files.insert(name_str, modified);
                                        }
                                        Err(e) => {
                                            error!("Map dir: failed to load '{name_str}': {e}")
                                        }
                                    }
                                }
                                Err(e) => error!("Map dir: failed to read '{name_str}': {e}"),
                            }
                        }
                    }
                }
            }
        }

        // Detect deletions
        let removed: Vec<String> = known_files
            .keys()
            .filter(|name| !current_files.contains(*name))
            .cloned()
            .collect();

        for name in removed {
            maps.write().unwrap().remove_map(&name);
            known_files.remove(&name);
            info!("Map dir: removed '{name}'");
        }
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

    let telegram_plugin = TelegramBridge::new(
        config.telegram.enabled,
        config.telegram.token.clone(),
        config.telegram.chat_id,
    );

    match args.mode {
        ServerMode::Edit => {
            let server = ServerBuilder::new()
                .bind_addr(config.bind_addr)
                .max_peers(config.max_peers)
                .plugin(config.base)
                .plugin(DeathrunEdit::new())
                .plugin(telegram_plugin)
                .plugin(lua_plugin);

            if let Err(e) = server.run().await {
                error!("Error running the server: {e}");
            }
        }
        ServerMode::Play => {
            let play = DeathrunPlay::new(config.deathrun_play.clone());

            if let Some(ref map_dir) = config.map_dir {
                let known = load_initial_maps(map_dir, &play).await;
                tokio::spawn(sync_map_dir(map_dir.clone(), play.maps_handle(), known));
            } else {
                warn!("No map_dir in config; starting play mode with no maps");
            }

            let server = ServerBuilder::new()
                .bind_addr(config.bind_addr)
                .max_peers(config.max_peers)
                .plugin(config.base)
                .plugin(play)
                .plugin(telegram_plugin)
                .plugin(lua_plugin);

            if let Err(e) = server.run().await {
                error!("Error running the server: {e}");
            }
        }
    }
}
