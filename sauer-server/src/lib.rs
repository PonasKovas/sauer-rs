use dashmap::DashMap;
use peer::Peer;
use sauer_proto::{Encode, route::{Chan1, S2C}};

mod base_plugin;
mod builder;
mod handle_client;
mod peer;
pub mod plugin;
pub mod utils;

pub use base_plugin::BasePlugin;
pub use builder::ServerBuilder;
pub use plugin::{Context, Plugin};

pub struct Server {
    clients: DashMap<u32, Peer>,
    pub extra: anymap::Map<dyn anymap::any::Any + Send + Sync>,
}

impl Server {
    fn new() -> Self {
        Self {
            clients: DashMap::new(),
            extra: anymap::Map::new(),
        }
    }
    pub fn get_client(&self, client_number: u32) -> dashmap::mapref::one::Ref<'_, u32, Peer> {
        match self.clients.get(&client_number) {
            Some(x) => x,
            None => panic!("tried to get client number which doesnt exist"),
        }
    }

    /// Send a channel-1 packet to every connected client. Drops silently on a
    /// full send buffer (same semantics as `Peer::try_send_chan1`).
    pub fn broadcast_chan1(&self, pkt: &impl Encode<S2C<Chan1>>) {
        for entry in self.clients.iter() {
            entry.value().try_send_chan1(pkt);
        }
    }
}
