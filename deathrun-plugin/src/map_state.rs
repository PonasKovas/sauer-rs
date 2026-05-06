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

#[derive(Clone)]
pub struct Trap {
    pub button_pos: [i32; 3],
    pub button_size: i32,
    pub button_orient: u8,

    pub trigger_payload: Option<sauer_proto::data::ClipboardPayload>,

    /// Per-cell changes — used for accurate server-map updates (no region collateral).
    pub trigger_cells: Vec<([i32; 3], i32, sauer_map::Node)>,

    /// Modified existing entities when the trap is active.
    pub trigger_entities_mod: Vec<(usize, ogz_rs::data::Entity)>,

    /// Newly added entities when the trap is active.
    pub trigger_entities_add: Vec<ogz_rs::data::Entity>,

    /// DelCube/paste region — tight diff bounds, grid-aligned.
    pub clip_pos: [i32; 3],
    pub clip_s: [i32; 3],
    pub clip_grid: i32,

    pub time: u32,
    pub timeout: u32,

    /// Base-state geometry — what to restore when resetting from an active trap.
    /// Populated by `cmd_endtrap`; mirrors the original map's cells/entities for
    /// the trap region so a save-while-active can be correctly recovered on load.
    pub reset_payload: Option<sauer_proto::data::ClipboardPayload>,
    pub reset_cells: Vec<([i32; 3], i32, sauer_map::Node)>,
    pub reset_entities_mod: Vec<(usize, ogz_rs::data::Entity)>,

    pub is_active: bool,
}

pub struct DeathrunState {
    pub map: Option<sauer_map::Map>,
    /// True while waiting for a player to upload a map after a map vote.
    pub waiting_for_map: bool,
    pub traps: Vec<Trap>,
    pub trap_edit: Option<crate::trap::TrapEditState>,
    pub trap_edit_particle_ent: Option<usize>,
    pub active_traps: Vec<crate::trap::ActiveTrap>,
    pub client_vslot_bloat: dashmap::DashMap<u64, usize>,
}

impl Default for DeathrunState {
    fn default() -> Self {
        Self {
            map: None,
            waiting_for_map: false,
            traps: Vec::new(),
            trap_edit: None,
            trap_edit_particle_ent: None,
            active_traps: Vec::new(),
            client_vslot_bloat: dashmap::DashMap::new(),
        }
    }
}

pub fn track_vslot_bloat(server: &std::sync::Arc<Server>, bytes: usize) {
    let state = get_server_state(server).read().unwrap();
    let mut to_resend = Vec::new();
    for mut entry in state.client_vslot_bloat.iter_mut() {
        let id = *entry.key();
        let new_size = *entry + bytes;
        *entry = new_size;
        if new_size > 3_000_000 {
            *entry = 0;
            to_resend.push(id);
        }
    }
    drop(state);

    for id in to_resend {
        crate::ui::send_warn(
            server,
            id,
            "\x0C3[INFO] Automatically reloading map to clear video memory bloat...",
        );
        crate::map_state::send_map_to_async(server, id);
    }
}

pub(crate) fn get_server_state(server: &Server) -> &RwLock<DeathrunState> {
    server.extra.get().expect("DeathrunState not initialised")
}

pub(crate) fn is_waiting_for_map(server: &Server) -> bool {
    get_server_state(server).read().unwrap().waiting_for_map
}

pub(crate) fn with_map_state_mut<F>(ctx: &Context, f: F)
where
    F: FnOnce(&mut sauer_map::Map),
{
    if let Some(map) = get_server_state(ctx.server()).write().unwrap().map.as_mut() {
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
                state.to_ogz_bytes()
            }).await.expect("to_ogz_bytes panicked");

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
