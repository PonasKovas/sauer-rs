use dashmap::DashMap;
use peer::Peer;
use sauer_proto::{
    PacketSequence,
    route::{Chan0, Chan1, Chan2, S2C},
};
use std::net::SocketAddr;

mod builder;
mod handle_client;
mod peer;
pub mod plugin;
pub mod utils;

pub use builder::ServerBuilder;
pub use plugin::{Context, Plugin};

pub struct Server {
    pub(crate) clients: DashMap<u32, Peer>,
    pub extra: anymap::Map<dyn anymap::any::Any + Send + Sync>,
}

impl Server {
    fn new() -> Self {
        Self {
            clients: DashMap::new(),
            extra: anymap::Map::new(),
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
}
