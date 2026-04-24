use bytes::Bytes;
use cube_string::CubeString;
use dashmap::DashMap;
use peer::Peer;
use sauer_proto::{
    PacketSequence,
    data::{GameMode, MasterMode},
    route::{Chan0, Chan1, Chan2, S2C},
};
use std::{
    net::SocketAddr,
    sync::{Mutex, RwLock},
};

mod builder;
mod handle_client;
mod peer;
pub mod plugin;
pub mod utils;
pub mod players;
pub mod items;
mod welcome;

pub use builder::ServerBuilder;
pub use plugin::{Context, Plugin};
pub use players::{ClipboardData, PlayerData, Players};
pub use items::{
    Items, ServerItem, apply_pickup, canpickup, delay_spawn, entity_to_item_type, spawntime,
};

pub struct Server {
    pub(crate) clients: DashMap<u32, Peer>,
    /// Extension point for external plugins to store their own state.
    pub extra: anymap::Map<dyn anymap::any::Any + Send + Sync>,

    // ── Per-client game state ─────────────────────────────────────────────────
    pub players: Players,
    pub items: RwLock<Items>,
    /// Raw bytes of the last map uploaded via channel 2.
    pub map_data: RwLock<Option<Bytes>>,

    // ── Server configuration (freely readable; use setter methods when a change
    // must be synced to all clients automatically) ────────────────────────────
    pub server_desc: RwLock<CubeString>,
    pub server_auth: RwLock<CubeString>,
    pub has_password: RwLock<bool>,
    pub map_name: RwLock<CubeString>,
    pub game_mode: RwLock<GameMode>,
    /// Access via [`Server::master_mode`] / [`Server::set_master_mode`] so that
    /// changes are automatically broadcast to clients.
    master_mode: Mutex<MasterMode>,
}

impl Server {
    pub(crate) fn new(
        server_desc: CubeString,
        server_auth: CubeString,
        has_password: bool,
        map_name: CubeString,
        game_mode: GameMode,
        master_mode: MasterMode,
    ) -> Self {
        Self {
            clients: DashMap::new(),
            extra: anymap::Map::new(),
            players: Players::new(),
            items: RwLock::new(Items::new()),
            map_data: RwLock::new(None),
            server_desc: RwLock::new(server_desc),
            server_auth: RwLock::new(server_auth),
            has_password: RwLock::new(has_password),
            map_name: RwLock::new(map_name),
            game_mode: RwLock::new(game_mode),
            master_mode: Mutex::new(master_mode),
        }
    }

    // ── Targeted sends ────────────────────────────────────────────────────────

    pub fn send_chan0(&self, cn: u32, pkt: impl PacketSequence<S2C<Chan0>>) {
        if let Some(peer) = self.clients.get(&cn) {
            peer.try_send_chan0(pkt);
        }
    }

    pub fn send_chan1(&self, cn: u32, pkt: impl PacketSequence<S2C<Chan1>>) {
        if let Some(peer) = self.clients.get(&cn) {
            peer.try_send_chan1(pkt);
        }
    }

    pub fn send_chan2(&self, cn: u32, pkt: impl PacketSequence<S2C<Chan2>>) {
        if let Some(peer) = self.clients.get(&cn) {
            peer.try_send_chan2(pkt);
        }
    }

    // ── Broadcast ─────────────────────────────────────────────────────────────

    pub fn broadcast_chan0(&self, pkt: impl PacketSequence<S2C<Chan0>>) {
        for entry in self.clients.iter() {
            entry.value().try_send_chan0(&pkt);
        }
    }

    pub fn broadcast_chan1(&self, pkt: impl PacketSequence<S2C<Chan1>>) {
        for entry in self.clients.iter() {
            entry.value().try_send_chan1(&pkt);
        }
    }

    pub fn broadcast_except_chan0(&self, except: u32, pkt: impl PacketSequence<S2C<Chan0>>) {
        for entry in self.clients.iter() {
            if *entry.key() != except {
                entry.value().try_send_chan0(&pkt);
            }
        }
    }

    pub fn broadcast_except_chan1(&self, except: u32, pkt: impl PacketSequence<S2C<Chan1>>) {
        for entry in self.clients.iter() {
            if *entry.key() != except {
                entry.value().try_send_chan1(&pkt);
            }
        }
    }

    // ── Connection management ─────────────────────────────────────────────────

    /// Initiate a graceful disconnect. The client is removed from the map only
    /// once the ENet connection is fully closed.
    pub fn disconnect(&self, cn: u32) {
        if let Some(peer) = self.clients.get(&cn) {
            peer.try_disconnect();
        }
    }

    // ── Queries ───────────────────────────────────────────────────────────────

    pub fn peer_addr(&self, cn: u32) -> Option<SocketAddr> {
        self.clients.get(&cn).map(|p| p.addr())
    }

    pub fn client_cns(&self) -> Vec<u32> {
        self.clients.iter().map(|e| *e.key()).collect()
    }

    // ── Master mode ───────────────────────────────────────────────────────────

    pub fn master_mode(&self) -> MasterMode {
        *self.master_mode.lock().unwrap()
    }

    /// Sets the master mode and broadcasts the updated master state to everyone.
    pub fn set_master_mode(&self, mode: MasterMode) {
        *self.master_mode.lock().unwrap() = mode;
        self.broadcast_current_master();
    }
}
