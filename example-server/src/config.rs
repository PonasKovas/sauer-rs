use crate::Args;
use sauer_base_plugin::options::BasePluginOptions;
use serde::{Deserialize, Serialize};
use std::{net::SocketAddr, path::PathBuf};
use tokio::fs;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Config {
    pub bind_addr: SocketAddr,
    pub max_peers: usize,
    pub plugin_dir: PathBuf,
    pub base: BasePluginOptions,
}

impl Config {
    pub async fn load(args: &Args) -> anyhow::Result<Self> {
        let contents = fs::read_to_string(&args.config).await?;

        let toml = toml::from_str(&contents)?;

        Ok(toml)
    }
}
