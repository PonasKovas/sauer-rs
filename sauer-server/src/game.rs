use crate::Items;
use bytes::Bytes;
use cube_string::{CubeString, IntoCubeStringLossy};
use sauer_proto::data::{GameMode, MasterMode};
use std::collections::HashMap;

/// The game being played. It sits behind one lock, so a reader never sees
/// half of a map change.
pub struct Game {
    pub map_name: CubeString,
    pub mode: GameMode,
    pub master_mode: MasterMode,
    /// Each player's current map vote (id → map name). Cleared on map change.
    pub map_votes: HashMap<u64, CubeString>,
    pub items: Items,
    /// Raw bytes of the last map uploaded via channel 2.
    pub map_data: Option<Bytes>,
}

impl Default for Game {
    fn default() -> Self {
        Self {
            map_name: "complex".lossy_cubestring(),
            mode: GameMode::Ffa,
            master_mode: MasterMode::Open,
            map_votes: HashMap::new(),
            items: Items::new(),
            map_data: None,
        }
    }
}

impl Game {
    /// Switches to another map: votes are cleared and items wait for the new
    /// map's list.
    pub fn change_map(&mut self, map_name: CubeString, mode: GameMode) {
        self.map_name = map_name;
        self.mode = mode;
        self.map_votes.clear();
        self.items.await_list();
    }
}

/// Server settings, mostly set once at startup.
pub struct Config {
    pub desc: CubeString,
    pub auth: CubeString,
    pub has_password: bool,
    /// Whether to register with the master server.
    pub update_master: bool,
    pub master_name: String,
    pub master_port: u16,
    /// Password for claiming admin with `/setmaster PASSWORD`. Empty disables it.
    pub admin_pass: String,
    /// Whether the master mode goes back to open once nobody holds master
    /// (engine behavior). Off for servers whose plugin sets the mode itself.
    pub reopen_when_masterless: bool,
    /// How many clients can connect, as shown in the server browser.
    pub max_clients: usize,
}

impl Default for Config {
    fn default() -> Self {
        Self {
            desc: "sauer-rs".lossy_cubestring(),
            auth: CubeString::new(),
            has_password: false,
            update_master: true,
            master_name: "master.sauerbraten.org".to_string(),
            master_port: 28787,
            admin_pass: String::new(),
            reopen_when_masterless: true,
            max_clients: 128,
        }
    }
}
