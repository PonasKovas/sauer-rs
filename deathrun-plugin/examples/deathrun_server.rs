use clap::Parser;
use cube_string::IntoCubeStringLossy;
use deathrun_plugin::{DeathrunEdit, DeathrunPlay};
use sauer_plugin_lua::{LuaPlugin, LuaPluginOptions};
use sauer_proto::data::MasterMode;
use sauer_server::{Server, ServerBuilder, plugin::Plugin};
use serde::{Deserialize, Serialize};
use std::{net::SocketAddr, path::PathBuf};
use tokio::fs;
use tracing::error;

#[derive(Parser, Debug)]
#[command(version, about, long_about = None)]
struct Args {
    #[arg(short, long, default_value = "config.toml.local")]
    config: PathBuf,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct Config {
    pub bind_addr: SocketAddr,
    pub max_peers: usize,
    pub plugin_dir: PathBuf,
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
        .plugin(lua_plugin);

    if let Err(e) = server.run().await {
        error!("Error running the server: {e}");
    }
}
