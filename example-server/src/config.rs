use crate::Args;
use cube_string::IntoCubeStringLossy;
use sauer_proto::data::{GameMode, MasterMode};
use sauer_server::{Server, plugin::Plugin};
use serde::{Deserialize, Serialize};
use std::{net::SocketAddr, path::PathBuf};
use tokio::fs;

/// Server configuration loaded from TOML.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Config {
    pub bind_addr: SocketAddr,
    pub max_peers: usize,
    pub plugin_dir: PathBuf,
    #[serde(default)]
    pub base: BaseConfig,
    #[serde(default)]
    pub telegram: TelegramConfig,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TelegramConfig {
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

/// Initial server settings (map, mode, description, etc.).
/// These become the startup state of `Server` and can be changed at runtime by plugins.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BaseConfig {
    #[serde(default = "default_server_desc")]
    pub server_desc: String,
    #[serde(default)]
    pub server_auth: String,
    #[serde(default)]
    pub has_password: bool,
    #[serde(default = "default_map")]
    pub map: String,
    #[serde(default = "default_game_mode")]
    pub mode: GameMode,
    #[serde(default = "default_master_mode")]
    pub master_mode: MasterMode,
    #[serde(default = "default_updatemaster")]
    pub updatemaster: bool,
}

fn default_server_desc() -> String {
    "sauer-rs".into()
}

fn default_map() -> String {
    "complex".into()
}

fn default_game_mode() -> GameMode {
    GameMode::Ffa
}

fn default_master_mode() -> MasterMode {
    MasterMode::Open
}

fn default_updatemaster() -> bool {
    true
}

impl Default for BaseConfig {
    fn default() -> Self {
        Self {
            server_desc: default_server_desc(),
            server_auth: String::new(),
            has_password: false,
            map: default_map(),
            mode: GameMode::Ffa,
            master_mode: MasterMode::Open,
            updatemaster: true,
        }
    }
}

impl Plugin for BaseConfig {
    async fn init(&self, server: &mut Server) {
        *server.server_desc.write().unwrap() = self.server_desc.as_str().lossy_cubestring();
        *server.server_auth.write().unwrap() = self.server_auth.as_str().lossy_cubestring();
        *server.has_password.write().unwrap() = self.has_password;
        *server.map_name.write().unwrap() = self.map.as_str().lossy_cubestring();
        *server.game_mode.write().unwrap() = self.mode;
        *server.master_mode.write().unwrap() = self.master_mode;
        *server.update_master.write().unwrap() = self.updatemaster;
    }
}

impl Config {
    pub async fn load(args: &Args) -> anyhow::Result<Self> {
        let contents = fs::read_to_string(&args.config).await?;
        Ok(toml::from_str(&contents)?)
    }
}
