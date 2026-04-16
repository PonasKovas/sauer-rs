use dashmap::DashMap;
use peer::Peer;

mod base_plugin;
mod builder;
mod handle_client;
mod peer;
pub mod plugin;
pub mod utils;

pub use base_plugin::BasePlugin;
pub use builder::ServerBuilder;

pub struct Server {
    clients: DashMap<u32, Peer>,
}

impl Server {
    fn new() -> Self {
        Self {
            clients: DashMap::new(),
        }
    }
    pub fn get_client(&self, client_number: u32) -> dashmap::mapref::one::Ref<'_, u32, Peer> {
        match self.clients.get(&client_number) {
            Some(x) => x,
            None => panic!("tried to get client number which doesnt exist"),
        }
    }
}
