mod error;
mod options;

pub use error::BasePluginError;
pub use options::BasePluginOptions;
use sauer_server::{Plugin, Server};

pub struct BasePlugin {
    //
}

impl BasePlugin {
    pub async fn init(options: &BasePluginOptions) -> Result<Self, BasePluginError> {
        Ok(Self {})
    }
}

impl Plugin for BasePlugin {
    async fn init(&self, server: &mut Server) -> () {
        // server.extra.insert(BaseShared);
    }
}
