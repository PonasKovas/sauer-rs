use crate::plugin::Plugin;
use std::net::{Ipv4Addr, SocketAddr, SocketAddrV4};

pub struct ServerBuilder<P = ()> {
    bind_addr: SocketAddr,
    max_peers: usize,
    plugin: P,
}

impl ServerBuilder {
    pub fn new() -> Self {
        ServerBuilder {
            bind_addr: SocketAddrV4::new(Ipv4Addr::UNSPECIFIED, 28785).into(),
            max_peers: 128,
            plugin: (),
        }
    }
}

impl<P: Plugin> ServerBuilder<P> {
    pub fn bind_addr(mut self, addr: impl Into<SocketAddr>) -> Self {
        self.bind_addr = addr.into();
        self
    }

    pub fn max_peers(mut self, peers: usize) -> Self {
        self.max_peers = peers;
        self
    }

    pub fn plugin<T: Plugin>(self, plugin: T) -> ServerBuilder<(P, T)> {
        ServerBuilder {
            bind_addr: self.bind_addr,
            max_peers: self.max_peers,
            plugin: (self.plugin, plugin),
        }
    }

    pub async fn run(self) -> std::io::Result<()> {
        crate::run::run(self.bind_addr, self.max_peers, self.plugin).await
    }
}

impl Default for ServerBuilder<()> {
    fn default() -> Self {
        Self::new()
    }
}
