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
    /// Traps whose active state is in the map, oldest first.
    pub active_trap_stack: Vec<crate::trap::TrapId>,
    /// Per-player fake entities in `map`.
    pub virt: sauer_kit::virt_ents::VirtEnts,
    /// Where client edits may not go: trap buttons and the vault.
    pub protected: sauer_kit::coop_edit::Protected,
    /// Where trap geometry and the trap record live in the map, once created.
    pub vault: Option<crate::trap::vault::Vault>,
    /// Generation of the last trap record written to the anchor.
    pub record_generation: u32,
}

impl DeathrunState {
    pub fn trap_index(&self, id: crate::trap::TrapId) -> Option<usize> {
        self.traps.iter().position(|t| t.id == id)
    }

    pub fn trap(&self, id: crate::trap::TrapId) -> Option<&Trap> {
        self.traps.iter().find(|t| t.id == id)
    }

    pub fn trap_mut(&mut self, id: crate::trap::TrapId) -> Option<&mut Trap> {
        self.traps.iter_mut().find(|t| t.id == id)
    }

    /// Shows `player` the fake entities `ents` under `namespace`.
    pub fn show_virt(
        &mut self,
        player: u64,
        namespace: &str,
        ents: Vec<sauer_kit::virt_ents::FakeEnt>,
    ) -> sauer_kit::virt_ents::VirtEntUpdates {
        match self.map.as_mut() {
            Some(map) => self.virt.update(&mut map.entities, player, namespace, ents),
            None => Default::default(),
        }
    }

    /// The trap whose button is at `hit_pt`, give or take half a unit.
    pub fn trap_at(&self, hit_pt: [f32; 3]) -> Option<crate::trap::TrapId> {
        let trap = self
            .traps
            .iter()
            .find(|t| t.contains_button_point(hit_pt, 0.5))?;
        Some(trap.id)
    }

    /// The number players see for a trap: its place in the list.
    pub fn trap_number(&self, id: crate::trap::TrapId) -> String {
        self.trap_index(id)
            .map_or_else(|| "?".to_string(), |i| i.to_string())
    }
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
            virt: Default::default(),
            protected: Default::default(),
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
                    server_clone.send(id, &channel2::SendMap { data: bytes });
                }
                Err(e) => {
                    error!("failed to serialize map state: {e}");
                }
            }
        }
    });
}
