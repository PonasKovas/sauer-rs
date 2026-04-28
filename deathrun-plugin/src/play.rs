use crate::map_state::{MapState, Trap};
use crate::trap::{StoredTrap, TRAP_STORE_VERSION};
use cube_string::IntoCubeStringLossy;
use sauer_proto::{channel1, channel2, data::GameMode};
use sauer_server::{
    Server,
    plugin::{Context, Plugin},
};
use std::collections::HashMap;
use std::sync::{Arc, RwLock};
use tracing::{error, info, warn};

pub struct LoadedMap {
    pub state: MapState,
    pub traps: Vec<Trap>,
}

pub struct PlayMaps {
    pub maps: HashMap<String, LoadedMap>,
    pub current_map: Option<String>,
}

impl Default for PlayMaps {
    fn default() -> Self {
        Self {
            maps: HashMap::new(),
            current_map: None,
        }
    }
}

impl PlayMaps {
    pub fn add_map(&mut self, name: String, data: &[u8]) -> Result<(), String> {
        let mut state = MapState::from_bytes(data).map_err(|e| e.to_string())?;
        let mut traps = Vec::new();
        match crate::entity_store::read::<Vec<StoredTrap>>(&state.world, TRAP_STORE_VERSION) {
            Ok(Some(stored)) => {
                for st in &stored {
                    st.apply_reset_to_world(&mut state.world);
                }
                traps = stored.into_iter().filter_map(|st| st.into_trap()).collect();
                info!("Loaded {} traps from map '{name}'", traps.len());
            }
            Ok(None) => {}
            Err(e) => warn!("Could not load traps from map '{name}': {e}"),
        }
        if self.current_map.is_none() {
            self.current_map = Some(name.clone());
        }
        self.maps.insert(name, LoadedMap { state, traps });
        Ok(())
    }

    pub fn remove_map(&mut self, name: &str) {
        self.maps.remove(name);
        if self.current_map.as_deref() == Some(name) {
            self.current_map = self.maps.keys().next().cloned();
        }
    }
}

pub struct DeathrunPlay {
    maps: Arc<RwLock<PlayMaps>>,
}

impl DeathrunPlay {
    pub fn new() -> Self {
        Self {
            maps: Arc::new(RwLock::new(PlayMaps::default())),
        }
    }

    pub fn maps_handle(&self) -> Arc<RwLock<PlayMaps>> {
        Arc::clone(&self.maps)
    }

    pub fn add_map(&self, name: String, data: &[u8]) -> Result<(), String> {
        self.maps.write().unwrap().add_map(name, data)
    }

    pub fn remove_map(&self, name: &str) {
        self.maps.write().unwrap().remove_map(name);
    }
}

fn send_current_map_to(server: &Server, cn: u32) -> bool {
    let maps = match server.extra.get::<Arc<RwLock<PlayMaps>>>() {
        Some(m) => m,
        None => return false,
    };
    let bytes = {
        let lock = maps.read().unwrap();
        let name = match &lock.current_map {
            Some(n) => n,
            None => return false,
        };
        match lock.maps.get(name) {
            Some(loaded) => match loaded.state.to_ogz_bytes() {
                Ok(b) => b,
                Err(e) => {
                    error!("Failed to serialize map '{name}': {e}");
                    return false;
                }
            },
            None => return false,
        }
    };
    server.send_chan2(cn, &channel2::SendMap { data: bytes });
    true
}

impl Plugin for DeathrunPlay {
    async fn init(&self, server: &mut Server) {
        *server.map_name.write().unwrap() = "".lossy_cubestring();
        *server.game_mode.write().unwrap() = GameMode::TeamPlay;
        server.extra.insert(Arc::clone(&self.maps));
    }

    async fn on_client_welcomed(&self, ctx: &mut Context) {
        let cn = ctx.cn();
        if send_current_map_to(ctx.server(), cn) {
            info!("Sent current map to cn {cn}");
        }
    }

    async fn get_map(&self, ctx: &mut Context, _packet: &channel1::GetMap) {
        let cn = ctx.cn();
        if send_current_map_to(ctx.server(), cn) {
            info!("Sent current map to cn {cn} on N_GETMAP");
            ctx.cancel();
        }
    }
}
