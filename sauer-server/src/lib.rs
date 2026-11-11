use bytes::Bytes;
use cube_string::{CubeString, IntoCubeStringLossy};
use dashmap::DashMap;
use parking_lot::RwLock;
use peer::Peer;
use sauer_proto::{
    PacketDescriptor, PacketSequence,
    data::{GameMode, MasterMode},
    route::{Chan0, Chan1, Chan2, S2C},
};
use std::{collections::HashMap, net::SocketAddr};
use std::{fmt::Debug, sync::Arc};

mod bans;
mod base;
mod builder;
pub mod command;
mod handle_client;
pub mod items;
mod packet_log;
mod peer;
pub mod players;
pub mod plugin;
mod run;
pub mod ui;
pub mod utils;
mod welcome;

pub use builder::ServerBuilder;
pub use items::{
    ItemState, Items, MAX_ENTS, ServerItem, apply_pickup, canpickup, delay_spawn, spawntime,
};
pub use players::{ClipboardData, PlayerData, Players};

/// Stands in for a client id in cn slots taken with `Server::reserve_cn`.
const RESERVED_ID: u64 = u64::MAX;
pub use plugin::{Context, Plugin, ServerQueryContext};

pub struct Server {
    pub start_time: std::time::Instant,
    pub extinfo_socket: std::sync::OnceLock<Arc<tokio::net::UdpSocket>>,
    pub(crate) clients: DashMap<u64, Peer>,
    /// Maps client numbers (0-255) to internal ids.
    pub cns: RwLock<[Option<u64>; 256]>,
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
    pub update_master: RwLock<bool>,
    pub master_name: RwLock<String>,
    pub master_port: u16,
    pub map_name: RwLock<CubeString>,
    pub game_mode: RwLock<GameMode>,
    pub master_mode: RwLock<MasterMode>,
    /// Tracks each player's current map vote (id → map name). Cleared on map change.
    pub map_votes: RwLock<HashMap<u64, CubeString>>,
    pub commands: RwLock<Vec<command::CommandInfo>>,
    /// Extra lines shown at the end of `#help`, below the command list.
    pub help_lines: RwLock<Vec<CubeString>>,
    /// Password for claiming admin with `/setmaster PASSWORD`. Empty disables it.
    pub admin_pass: RwLock<String>,
    /// IPs kicked with `/kick`, and when each ban runs out.
    pub bans: RwLock<Vec<(std::net::IpAddr, std::time::Instant)>>,
    /// Whether the master mode goes back to open once nobody holds master
    /// (engine behavior). Off for servers whose plugin sets the mode itself.
    pub reopen_when_masterless: RwLock<bool>,
}

impl Server {
    pub fn new() -> Self {
        Self {
            start_time: std::time::Instant::now(),
            extinfo_socket: std::sync::OnceLock::new(),
            clients: DashMap::new(),
            cns: RwLock::new([None; 256]),
            extra: anymap::Map::new(),
            players: Players::new(),
            items: RwLock::new(Items::new()),
            map_data: RwLock::new(None),
            server_desc: RwLock::new("sauer-rs".lossy_cubestring()),
            server_auth: RwLock::new(CubeString::new()),
            has_password: RwLock::new(false),
            update_master: RwLock::new(true),
            master_name: RwLock::new("master.sauerbraten.org".to_string()),
            master_port: 28787,
            map_name: RwLock::new("complex".lossy_cubestring()),
            game_mode: RwLock::new(GameMode::Ffa),
            master_mode: RwLock::new(MasterMode::Open),
            map_votes: RwLock::new(HashMap::new()),
            commands: RwLock::new(vec![command::CommandInfo {
                name: "#help".to_string(),
                desc: "list all server commands".to_string(),
                usage: "".to_string(),
            }]),
            help_lines: RwLock::new(Vec::new()),
            admin_pass: RwLock::new(String::new()),
            bans: RwLock::new(Vec::new()),
            reopen_when_masterless: RwLock::new(true),
        }
    }

    pub fn register_command(&self, name: &str, desc: &str, usage: &str) {
        self.commands.write().push(command::CommandInfo {
            name: name.to_string(),
            desc: desc.to_string(),
            usage: usage.to_string(),
        });
    }

    /// Appends lines to the end of the `#help` output.
    pub fn add_help_lines(&self, lines: impl IntoIterator<Item = CubeString>) {
        self.help_lines.write().extend(lines);
    }

    // ── Server Query Responses ────────────────────────────────────────────────

    pub fn send_server_query(
        &self,
        to: std::net::SocketAddr,
        response: &sauer_proto::server_query::ServerQueryResponse,
    ) {
        if let Some(sock) = self.extinfo_socket.get() {
            let data = response.to_bytes();
            let _ = sock.try_send_to(&data, to);
        }
    }

    // ── Client ID / CN Mapping ────────────────────────────────────────────────

    pub fn assign_cn(&self, id: u64) -> Option<u8> {
        let mut cns = self.cns.write();

        // 1. Check if already assigned
        for (cn, slot) in cns.iter().enumerate() {
            if slot == &Some(id) {
                tracing::debug!("id {} already has cn {}", id, cn);
                return Some(cn as u8);
            }
        }

        // 2. Assign new
        for (cn, slot) in cns.iter_mut().enumerate() {
            if slot.is_none() {
                *slot = Some(id);
                tracing::info!("assigned id {} to cn {}", id, cn);
                return Some(cn as u8);
            }
        }
        tracing::error!("failed to assign cn for id {}", id);
        None
    }

    pub fn free_cn(&self, cn: u8) {
        let mut cns = self.cns.write();
        cns[cn as usize] = None;
    }

    pub fn get_cn(&self, id: u64) -> Option<u8> {
        let cns = self.cns.read();
        cns.iter()
            .position(|&slot| slot == Some(id))
            .map(|i| i as u8)
    }

    /// The connected client using `cn`. `None` for a free or reserved cn: a
    /// reserved one belongs to a fake player (like a plugin's bot) with no
    /// player data, so packets aimed at it are ignored.
    pub fn get_id(&self, cn: u8) -> Option<u64> {
        let cns = self.cns.read();
        cns[cn as usize].filter(|&id| id != RESERVED_ID)
    }

    pub fn reserve_cn(&self, cn: u8) -> bool {
        let mut cns = self.cns.write();
        if cns[cn as usize].is_some() {
            return false;
        }
        cns[cn as usize] = Some(RESERVED_ID);
        true
    }

    // ── Targeted sends ────────────────────────────────────────────────────────

    pub fn send_chan0(
        &self,
        id: u64,
        pkt: impl PacketSequence<S2C<Chan0>> + PacketDescriptor + Debug,
    ) {
        packet_log::trace_out(id, &pkt);
        if let Some(peer) = self.clients.get(&id) {
            peer.try_send_chan0(pkt);
        }
    }

    pub fn send_chan1(
        &self,
        id: u64,
        pkt: impl PacketSequence<S2C<Chan1>> + PacketDescriptor + Debug,
    ) {
        packet_log::trace_out(id, &pkt);
        if let Some(peer) = self.clients.get(&id) {
            peer.try_send_chan1(pkt);
        }
    }

    pub fn send_chan2(&self, id: u64, pkt: impl PacketSequence<S2C<Chan2>>) {
        if let Some(peer) = self.clients.get(&id) {
            peer.try_send_chan2(pkt);
        }
    }

    // ── Broadcast ─────────────────────────────────────────────────────────────

    pub fn broadcast_chan0(&self, pkt: impl PacketSequence<S2C<Chan0>> + PacketDescriptor + Debug) {
        packet_log::trace_out_broadcast(&pkt);
        for entry in self.clients.iter() {
            entry.value().try_send_chan0(&pkt);
        }
    }

    pub fn broadcast_chan1(&self, pkt: impl PacketSequence<S2C<Chan1>> + PacketDescriptor + Debug) {
        packet_log::trace_out_broadcast(&pkt);
        for entry in self.clients.iter() {
            entry.value().try_send_chan1(&pkt);
        }
    }

    pub fn broadcast_except_chan0(
        &self,
        except: u64,
        pkt: impl PacketSequence<S2C<Chan0>> + PacketDescriptor + Debug,
    ) {
        packet_log::trace_out_broadcast_except(except, &pkt);
        for entry in self.clients.iter() {
            if *entry.key() != except {
                entry.value().try_send_chan0(&pkt);
            }
        }
    }

    pub fn broadcast_except_chan1(
        &self,
        except: u64,
        pkt: impl PacketSequence<S2C<Chan1>> + PacketDescriptor + Debug,
    ) {
        packet_log::trace_out_broadcast_except(except, &pkt);
        for entry in self.clients.iter() {
            if *entry.key() != except {
                entry.value().try_send_chan1(&pkt);
            }
        }
    }

    // ── Connection management ─────────────────────────────────────────────────

    /// Initiate a graceful disconnect. The client is removed from the map only
    /// once the ENet connection is fully closed.
    pub fn disconnect(&self, id: u64) {
        if let Some(peer) = self.clients.get(&id) {
            peer.try_disconnect();
        }
    }

    // ── Queries ───────────────────────────────────────────────────────────────

    pub fn peer_addr(&self, id: u64) -> Option<SocketAddr> {
        self.clients.get(&id).map(|p| p.addr())
    }

    pub fn client_ids(&self) -> Vec<u64> {
        self.clients.iter().map(|e| *e.key()).collect()
    }

    pub fn client_count(&self) -> usize {
        self.clients.len()
    }
}

impl Default for Server {
    fn default() -> Self {
        Self::new()
    }
}
