use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DeathrunOptions {
    /// If true, the server runs in editing mode: it waits for the first map to be uploaded,
    /// making everyone a spectator until the master sends the map.
    pub is_edit_server: bool,
}

impl Default for DeathrunOptions {
    fn default() -> Self {
        Self {
            is_edit_server: true,
        }
    }
}
