use cube_string::CubeString;
use dashmap::DashMap;
use parking_lot::RwLock;
use peer::Peer;
use sauer_proto::{
    PacketDescriptor, PacketSequence,
    data::Cn,
    route::{Channel, S2C},
};
use std::net::SocketAddr;
use std::{fmt::Debug, sync::Arc};

mod bans;
mod base;
mod builder;
pub mod command;
mod game;
mod handle_client;
pub mod items;
mod packet_log;
mod peer;
pub mod players;
pub mod plugin;
mod run;
#[cfg(feature = "serde")]
mod settings;
pub mod ui;
pub mod utils;
mod welcome;

pub use bans::{DISC_IPBAN, DISC_KICK};
pub use builder::ServerBuilder;
pub use game::{Config, Game};
pub use items::{
    ItemState, Items, MAX_ENTS, ServerItem, apply_pickup, canpickup, delay_spawn, spawntime,
};
pub use players::{PlayerData, Players, Status};
#[cfg(feature = "serde")]
pub use settings::Settings;

pub use plugin::{Context, Flow, Plugin, ServerQueryContext};
pub use sauer_proto;

pub struct Server {
    pub start_time: std::time::Instant,
    pub extinfo_socket: std::sync::OnceLock<Arc<tokio::net::UdpSocket>>,
    pub(crate) clients: DashMap<u64, Peer>,
    /// Who holds each client number.
    cns: RwLock<[CnSlot; 256]>,
    /// Extension point for external plugins to store their own state.
    pub extra: anymap::Map<dyn anymap::any::Any + Send + Sync>,

    pub players: Players,
    /// Don't hold this while calling into `players`: a player closure may take it.
    pub game: RwLock<Game>,
    /// Use setter methods when a change must reach the clients.
    pub config: RwLock<Config>,
    pub(crate) commands: RwLock<Vec<command::CommandInfo>>,
    /// Extra lines shown at the end of `#help`, below the command list.
    pub(crate) help_lines: RwLock<Vec<CubeString>>,
    /// IPs kicked with `/kick`, and when each ban runs out.
    bans: RwLock<Vec<(std::net::IpAddr, std::time::Instant)>>,
    /// Added in `init` with `add_fake_player`, so no lock.
    fake_players: Vec<(Cn, FakePlayer)>,
}

impl Server {
    pub fn new() -> Self {
        Self {
            start_time: std::time::Instant::now(),
            extinfo_socket: std::sync::OnceLock::new(),
            clients: DashMap::new(),
            cns: RwLock::new([CnSlot::Free; 256]),
            extra: anymap::Map::new(),
            players: Players::new(),
            game: RwLock::new(Game::default()),
            config: RwLock::new(Config::default()),
            commands: RwLock::new(vec![command::CommandInfo {
                name: "#help".to_string(),
                desc: "list all server commands".to_string(),
                usage: "".to_string(),
            }]),
            help_lines: RwLock::new(Vec::new()),
            bans: RwLock::new(Vec::new()),
            fake_players: Vec::new(),
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

    /// Gives a newly connected client the lowest free cn.
    pub(crate) fn assign_cn(&self, id: u64) -> Option<Cn> {
        let mut cns = self.cns.write();
        let cn = cns.iter().position(|slot| *slot == CnSlot::Free)?;
        cns[cn] = CnSlot::Client(id);
        tracing::info!("assigned id {id} to cn {cn}");
        Some(Cn(cn as u8))
    }

    /// Frees the cn a client was holding, and returns it.
    pub(crate) fn free_cn(&self, id: u64) -> Option<Cn> {
        let mut cns = self.cns.write();
        let cn = cns.iter().position(|slot| *slot == CnSlot::Client(id))?;
        cns[cn] = CnSlot::Free;
        Some(Cn(cn as u8))
    }

    pub fn get_cn(&self, id: u64) -> Option<Cn> {
        let cns = self.cns.read();
        cns.iter()
            .position(|slot| *slot == CnSlot::Client(id))
            .map(|i| Cn(i as u8))
    }

    /// The connected client using `cn`. `None` for a free or reserved cn: a
    /// reserved one belongs to a fake player (like a plugin's bot) with no
    /// player data, so packets aimed at it are ignored.
    pub fn get_id(&self, cn: Cn) -> Option<u64> {
        match self.cns.read()[cn.index()] {
            CnSlot::Client(id) => Some(id),
            CnSlot::Free | CnSlot::Reserved => None,
        }
    }

    /// Adds a player with no connection at `cn`, like a plugin's bot that
    /// speaks for the server. It's a spectator every client is told about when
    /// it joins. Returns false if `cn` is taken.
    #[must_use]
    pub fn add_fake_player(&mut self, cn: Cn, player: FakePlayer) -> bool {
        let slot = &mut self.cns.get_mut()[cn.index()];
        if *slot != CnSlot::Free {
            return false;
        }
        *slot = CnSlot::Reserved;
        self.fake_players.push((cn, player));
        true
    }

    // ── Sending ───────────────────────────────────────────────────────────────
    // The channel comes from the packet's type.

    pub fn send<C: Channel>(
        &self,
        id: u64,
        pkt: impl PacketSequence<S2C<C>> + PacketDescriptor + Debug,
    ) {
        packet_log::trace_out(id, &pkt);
        if let Some(peer) = self.clients.get(&id) {
            peer.try_send(pkt);
        }
    }

    pub fn broadcast<C: Channel>(
        &self,
        pkt: impl PacketSequence<S2C<C>> + PacketDescriptor + Debug,
    ) {
        packet_log::trace_out_broadcast(&pkt);
        for entry in self.clients.iter().filter(|e| e.value().joined()) {
            entry.value().try_send(&pkt);
        }
    }

    pub fn broadcast_except<C: Channel>(
        &self,
        except: u64,
        pkt: impl PacketSequence<S2C<C>> + PacketDescriptor + Debug,
    ) {
        packet_log::trace_out_broadcast_except(except, &pkt);
        for entry in self.clients.iter().filter(|e| e.value().joined()) {
            if *entry.key() != except {
                entry.value().try_send(&pkt);
            }
        }
    }

    /// Passes a client's packet on to everyone else, wrapped in `N_CLIENT` with
    /// their cn so the others know who it came from.
    pub fn relay_from<T>(&self, id: u64, packet: T)
    where
        sauer_proto::Client<T>:
            PacketSequence<S2C<sauer_proto::route::Chan1>> + PacketDescriptor + Debug,
    {
        if let Some(cn) = self.get_cn(id) {
            self.broadcast_except(
                id,
                &sauer_proto::Client {
                    client_num: cn,
                    packet,
                },
            );
        }
    }

    // ── Connection management ─────────────────────────────────────────────────

    /// Initiate a graceful disconnect. The client is removed from the map only
    /// once the ENet connection is fully closed.
    pub fn disconnect(&self, id: u64) {
        self.disconnect_with(id, 0);
    }

    // ── Queries ───────────────────────────────────────────────────────────────

    pub fn peer_addr(&self, id: u64) -> Option<SocketAddr> {
        self.clients.get(&id).map(|p| p.addr())
    }

    /// Every connected peer, joined or still in the handshake.
    pub fn client_ids(&self) -> Vec<u64> {
        self.clients.iter().map(|e| *e.key()).collect()
    }

    /// The clients that finished joining (see [`Server::welcome`]).
    pub fn joined_ids(&self) -> Vec<u64> {
        self.clients
            .iter()
            .filter(|e| e.value().joined())
            .map(|e| *e.key())
            .collect()
    }

    pub fn client_count(&self) -> usize {
        self.clients.len()
    }
}

/// A player with no connection, see [`Server::add_fake_player`].
#[derive(Clone, Debug)]
pub struct FakePlayer {
    pub name: CubeString,
    pub model: sauer_proto::data::PlayerModel,
    pub team: CubeString,
}

/// Who holds a client number.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum CnSlot {
    Free,
    Client(u64),
    /// A fake player's, see [`Server::add_fake_player`].
    Reserved,
}

impl Default for Server {
    fn default() -> Self {
        Self::new()
    }
}
