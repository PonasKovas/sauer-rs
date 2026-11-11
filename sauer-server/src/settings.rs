//! Startup settings, as read from a config file.

use crate::{Plugin, Server};
use cube_string::IntoCubeStringLossy;
use sauer_proto::data::{GameMode, MasterMode};

/// Settings to apply at startup. What's left out keeps the server's (or an
/// earlier plugin's) value. As a plugin it applies them in `init`.
#[derive(Clone, Debug, Default, serde::Deserialize)]
#[serde(default)]
pub struct Settings {
    pub server_desc: Option<String>,
    pub server_auth: Option<String>,
    pub has_password: Option<bool>,
    /// Password for claiming admin with `/setmaster PASSWORD`. Empty disables admin.
    pub admin_pass: Option<String>,
    pub map: Option<String>,
    pub mode: Option<GameMode>,
    pub master_mode: Option<MasterMode>,
    /// Whether to register with the master server.
    pub updatemaster: Option<bool>,
}

impl Settings {
    pub fn apply(&self, server: &mut Server) {
        let config = server.config.get_mut();
        if let Some(desc) = &self.server_desc {
            config.desc = desc.as_str().lossy_cubestring();
        }
        if let Some(auth) = &self.server_auth {
            config.auth = auth.as_str().lossy_cubestring();
        }
        if let Some(has_password) = self.has_password {
            config.has_password = has_password;
        }
        if let Some(pass) = &self.admin_pass {
            config.admin_pass = pass.clone();
        }
        if let Some(update) = self.updatemaster {
            config.update_master = update;
        }
        if config.admin_pass.is_empty() {
            tracing::info!("admin password not set, /setmaster PASSWORD is off");
        }
        let game = server.game.get_mut();
        if let Some(map) = &self.map {
            game.map_name = map.as_str().lossy_cubestring();
        }
        if let Some(mode) = self.mode {
            game.mode = mode;
        }
        if let Some(master_mode) = self.master_mode {
            game.master_mode = master_mode;
        }
    }
}

impl Plugin for Settings {
    async fn init(&self, server: &mut Server) {
        self.apply(server);
    }
}
