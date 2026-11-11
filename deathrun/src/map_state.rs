use parking_lot::RwLock;
use sauer_server::Server;
use sauer_server::plugin::Context;

pub fn update_server_items(map: &sauer_map::Map, server: &Server) {
    let items = map.entities.iter().enumerate().filter_map(|(i, ent)| {
        let item_type = sauer_proto::data::ItemType::try_from(ent.ent_type).ok()?;
        Some((i, item_type))
    });
    let num_players = server.joined_count();
    server.game.write().items.load(items, num_players);
}

pub use crate::trap::Trap;

/// The traps whose active state is in the map, and the map without them.
pub struct ActiveLayer {
    pub base: Box<sauer_map::Map>,
    /// Oldest first.
    pub stack: Vec<crate::trap::TrapId>,
}

pub struct DeathrunState {
    pub map: sauer_map::Map,
    /// After a map vote: the voted map's name, until someone uploads it.
    pub awaiting_upload: Option<cube_string::CubeString>,
    pub traps: Vec<Trap>,
    pub mode: crate::trap::TrapMode,
    /// Set while traps are firing.
    pub active: Option<ActiveLayer>,
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
    /// The map with no trap firing.
    pub fn base_map(&self) -> &sauer_map::Map {
        self.active.as_ref().map_or(&self.map, |a| &a.base)
    }

    /// The copy of the map without the firing traps, if any are firing.
    pub fn base_map_mut(&mut self) -> Option<&mut sauer_map::Map> {
        self.active.as_mut().map(|a| &mut *a.base)
    }

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
        self.virt
            .update(&mut self.map.entities, player, namespace, ents)
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
            map: sauer_map::Map::default_empty(),
            awaiting_upload: None,
            traps: Vec::new(),
            mode: Default::default(),
            active: None,
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
    get_server_state(server).read().awaiting_upload.is_some()
}

pub(crate) fn with_map_state_mut<F>(ctx: &Context, mut f: F)
where
    F: FnMut(&mut sauer_map::Map),
{
    let mut state = get_server_state(ctx.server()).write();
    if let Some(base) = state.base_map_mut() {
        f(base);
    }
    {
        let map = &mut state.map;
        f(map);
    }
}
