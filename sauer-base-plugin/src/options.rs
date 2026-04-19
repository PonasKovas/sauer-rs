use cube_string::{CubeString, IntoCubeStringLossy};
use sauer_proto::data::GameMode;
use sauer_server::Server;
use std::sync::RwLock;

#[derive(Clone, PartialEq, Debug, Hash)]
pub struct BasePluginOptions {
    /// Displayed in the server browser.
    pub server_desc: CubeString,
    /// Auth domain for server authentication (leave empty to disable).
    pub server_auth: CubeString,
    /// Server join password. Clients must supply the correct Tiger hash in N_CONNECT.
    pub has_password: bool,
    pub map: CubeString,
    pub mode: GameMode,
    /// If true, the base plugin will instantly welcome and add to the world newly connected players
    ///
    /// Disable if you want to extra checks or procedures like auth, you can welcome them manually later by using [`crate::welcome`]
    pub instant_welcome: bool,
}

impl BasePluginOptions {
    pub fn new() -> Self {
        Self {
            server_desc: "sauer-rs".lossy_cubestring(),
            server_auth: CubeString::new(),
            has_password: false,
            map: "complex".lossy_cubestring(),
            mode: GameMode::Ffa,
            instant_welcome: true,
        }
    }
}

impl Default for BasePluginOptions {
    fn default() -> Self {
        Self::new()
    }
}

pub fn get_options(server: &Server) -> &RwLock<BasePluginOptions> {
    match server.extra.get() {
        Some(x) => x,
        None => panic!("BasePluginOptions not initialized"),
    }
}
