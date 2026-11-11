use parking_lot::RwLock;
use sauer_proto::channel2;
use sauer_server::Server;
use sauer_server::plugin::Context;
use tracing::error;

pub fn update_server_items(map: &sauer_map::Map, server: &Server) {
    let items = map.entities.iter().enumerate().filter_map(|(i, ent)| {
        let ogz_rs::data::EntityType::GameSpecific(n) = ent.ent_type else {
            return None;
        };
        let proto_type = sauer_proto::data::EntityType::try_from(n as i32).ok()?;
        let item_type = sauer_proto::data::ItemType::try_from(proto_type).ok()?;
        Some((i, item_type))
    });
    let num_players = server.client_count();
    server.game.write().items.load(items, num_players);
}

pub use crate::trap::Trap;

pub struct DeathrunState {
    pub map: Option<sauer_map::Map>,
    /// True while waiting for a player to upload a map after a map vote.
    pub waiting_for_map: bool,
    pub uploading_map_name: Option<cube_string::CubeString>,
    pub traps: Vec<Trap>,
    pub mode: crate::trap::TrapMode,
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
            mode: Default::default(),
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
    get_server_state(server).read().waiting_for_map
}

pub(crate) fn with_map_state_mut<F>(ctx: &Context, mut f: F)
where
    F: FnMut(&mut sauer_map::Map),
{
    let mut state = get_server_state(ctx.server()).write();
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
            let lock = get_server_state(&server_clone).read();
            lock.map.clone()
        };

        if let Some(state) = state_opt {
            let out_bytes = tokio::task::spawn_blocking(move || {
                state.to_ogz_bytes_merged(flate2::Compression::fast())
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
