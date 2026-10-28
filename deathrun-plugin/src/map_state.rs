use sauer_proto::channel2;
use sauer_server::plugin::Context;
use sauer_server::{Server, ServerItem, delay_spawn, spawntime};
use std::sync::RwLock;
use tracing::error;

pub fn update_server_items(map: &sauer_map::Map, server: &Server) {
    let num_players = server.client_count();
    let mut items = server.items.write().unwrap();
    items.reset();

    for (i, ent) in map.entities.iter().enumerate() {
        if let ogz_rs::data::EntityType::GameSpecific(n) = ent.ent_type {
            let Ok(proto_type) = sauer_proto::data::EntityType::try_from(n as i32) else {
                continue;
            };
            if let Ok(item_type) = sauer_proto::data::ItemType::try_from(proto_type) {
                if items.list.len() <= i {
                    items.list.resize_with(i + 1, || ServerItem {
                        item_type,
                        spawned: false,
                        spawn_time_ms: 0,
                    });
                }
                items.list[i].item_type = item_type;
                if delay_spawn(item_type) {
                    items.list[i].spawned = false;
                    items.list[i].spawn_time_ms = spawntime(item_type, num_players);
                } else {
                    items.list[i].spawned = true;
                    items.list[i].spawn_time_ms = 0;
                }
            }
        }
    }
    items.notgotitems = false;
}

pub use crate::trap::Trap;

pub struct DeathrunState {
    pub map: Option<sauer_map::Map>,
    /// True while waiting for a player to upload a map after a map vote.
    pub waiting_for_map: bool,
    pub uploading_map_name: Option<cube_string::CubeString>,
    pub traps: Vec<Trap>,
    pub trap_edit: Option<crate::trap::TrapEditState>,
    pub base_map: Option<Box<sauer_map::Map>>,
    pub active_trap_stack: Vec<usize>,
    /// Where trap geometry and the trap record live in the map, once created.
    pub vault: Option<crate::trap::vault::Vault>,
    /// Generation of the last trap record written to the anchor.
    pub record_generation: u32,
}

impl Default for DeathrunState {
    fn default() -> Self {
        Self {
            map: None,
            waiting_for_map: false,
            uploading_map_name: None,
            traps: Vec::new(),
            trap_edit: None,
            base_map: None,
            active_trap_stack: Vec::new(),
            vault: None,
            record_generation: 0,
        }
    }
}

pub(crate) fn get_server_state(server: &Server) -> &RwLock<DeathrunState> {
    server.extra.get().expect("DeathrunState not initialised")
}

pub(crate) fn is_waiting_for_map(server: &Server) -> bool {
    get_server_state(server).read().unwrap().waiting_for_map
}

pub(crate) fn with_map_state_mut<F>(ctx: &Context, mut f: F)
where
    F: FnMut(&mut sauer_map::Map),
{
    let mut state = get_server_state(ctx.server()).write().unwrap();
    if let Some(base) = state.base_map.as_mut() {
        f(base);
    }
    if let Some(map) = state.map.as_mut() {
        f(map);
    }
}

pub(crate) fn send_map_to_async(server: &std::sync::Arc<Server>, id: u64) {
    let server_clone = std::sync::Arc::clone(server);
    tokio::spawn(async move {
        let state_opt = {
            let lock = get_server_state(&server_clone).read().unwrap();
            lock.map.clone()
        };

        if let Some(state) = state_opt {
            let out_bytes = tokio::task::spawn_blocking(move || {
                state.to_ogz_bytes_with(flate2::Compression::fast())
            })
            .await
            .expect("to_ogz_bytes panicked");

            match out_bytes {
                Ok(bytes) => {
                    server_clone.send_chan2(id, &channel2::SendMap { data: bytes });
                }
                Err(e) => {
                    error!("failed to serialize map state: {e}");
                }
            }
        }
    });
}
