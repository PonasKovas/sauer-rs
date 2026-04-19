mod combat;
mod items;
mod plugin;
mod spawn;
mod state;

use cube_string::{Color, CubeString, IntoCubeStringLossy};
use sauer_proto::data::GameMode;
use std::sync::{Arc, Mutex};

use state::GameState;

pub struct BasePlugin {
    pub(crate) state: Arc<Mutex<GameState>>,
    pub(crate) server_desc: CubeString,
    pub(crate) map_name: CubeString,
    pub(crate) mode: GameMode,
}

impl Default for BasePlugin {
    fn default() -> Self {
        Self::new()
    }
}

impl BasePlugin {
    pub fn new() -> Self {
        Self {
            state: Arc::new(Mutex::new(GameState::new())),
            server_desc: CubeString::builder()
                .color(Color::Magenta)
                .push_str("sauer")
                .color(Color::White)
                .push_str("-")
                .color(Color::Yellow)
                .push_str("rs")
                .build(),
            map_name: "complex".lossy_cubestring(),
            mode: GameMode::Ffa,
        }
    }

    /// Set the map loaded on connect (default: `"complex"`).
    pub fn map(mut self, name: impl IntoCubeStringLossy) -> Self {
        self.map_name = name.lossy_cubestring();
        self
    }

    /// Set the game mode (default: `GameMode::Ffa`).
    pub fn mode(mut self, mode: GameMode) -> Self {
        self.mode = mode;
        self
    }

    /// Set the server description shown in the server browser.
    pub fn desc(mut self, desc: CubeString) -> Self {
        self.server_desc = desc;
        self
    }
}
