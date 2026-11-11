use crate::Args;
use serde::Deserialize;
use std::{net::SocketAddr, path::PathBuf};
use telegram_plugin::TelegramConfig;

/// Server configuration loaded from TOML.
#[derive(Debug, Deserialize)]
pub struct Config {
    pub bind_addr: SocketAddr,
    pub max_peers: usize,
    pub plugin_dir: PathBuf,
    #[serde(default)]
    pub base: sauer_server::Settings,
    #[serde(default)]
    pub telegram: TelegramConfig,
}

impl Config {
    pub async fn load(args: &Args) -> anyhow::Result<Self> {
        let contents = tokio::fs::read_to_string(&args.config).await?;
        Ok(toml::from_str(&contents)?)
    }
}
