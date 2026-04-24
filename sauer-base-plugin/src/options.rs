use cube_string::{CubeString, IntoCubeStringLossy};
use sauer_proto::data::{GameMode, MasterMode};
use sauer_server::Server;
use std::sync::RwLock;

#[derive(Clone, PartialEq, Debug, Hash)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
pub struct BasePluginOptions {
    /// Displayed in the server browser.
    pub server_desc: CubeString,
    /// Auth domain for server authentication (leave empty to disable).
    pub server_auth: CubeString,
    /// Server join password. Clients must supply the correct Tiger hash in N_CONNECT.
    #[cfg_attr(feature = "serde", serde(default))]
    pub has_password: bool,
    pub map: CubeString,
    pub mode: GameMode,
    /// Controls access to the server.
    ///
    /// - `Open` / `Veto` / `Auth`: players are welcomed immediately on connect.
    /// - `Locked`: players are welcomed but forced into spectator; only masters can unspectate them.
    /// - `Private`: players are **not** welcomed automatically — a separate plugin must
    ///   perform auth and then call [`crate::welcome`] manually.
    pub master_mode: MasterMode,
}

impl BasePluginOptions {
    pub fn new() -> Self {
        Self {
            server_desc: "sauer-rs".lossy_cubestring(),
            server_auth: CubeString::new(),
            has_password: false,
            map: "complex".lossy_cubestring(),
            mode: GameMode::Ffa,
            master_mode: MasterMode::Open,
        }
    }
}

impl Default for BasePluginOptions {
    fn default() -> Self {
        Self::new()
    }
}

pub fn get_options(server: &Server) -> &RwLock<BasePluginOptions> {
    server
        .extra
        .get()
        .expect("BasePluginOptions not initialized")
}
