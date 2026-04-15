use crate::{
    Server,
    handle_client::handle_client,
    peer::Peer,
    plugin::{Context, Plugin},
};
use enet_rs::{Host, HostConfig};
use std::{
    net::{Ipv4Addr, SocketAddr, SocketAddrV4},
    sync::Arc,
};
use tracing::error;

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct ServerBuilder<P = ()> {
    bind_addr: SocketAddr,
    max_peers: usize,

    plugin: P,
}

impl ServerBuilder {
    pub fn new() -> Self {
        ServerBuilder {
            bind_addr: SocketAddrV4::new(Ipv4Addr::LOCALHOST, 28785).into(),
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
    pub async fn run(self) -> enet_rs::Result<()> {
        let mut host = Host::bind(
            self.bind_addr,
            HostConfig {
                max_peers: self.max_peers,
                channel_count: sauer_proto::CHANNEL_COUNT as u8,
                use_checksum: false,
                incoming_bandwidth: 0,
                outgoing_bandwidth: 0,
            },
        )
        .await?;

        let plugin = Arc::new(self.plugin);
        let server = Arc::new(Server::new());

        let mut next_cn = 0;

        while let Some(peer) = host.accept().await {
            let (tx, rx) = peer.split();

            let cn = next_cn;
            next_cn = (next_cn + 1) % i32::MAX as u32;

            server.clients.insert(cn, Peer::new(tx));

            let plugin = Arc::clone(&plugin);
            let server = Arc::clone(&server);
            tokio::spawn(async move {
                'ff: {
                    if plugin.connected(Context::new(&server, cn)).await.is_break() {
                        break 'ff; // walmart version of GOTO
                    }

                    if let Err(e) = handle_client(&server, &plugin, rx, cn).await {
                        error!("handling peer: {e}");
                    }
                }

                let _ = plugin.disconnected(Context::new(&server, cn)).await;

                server.clients.remove(&cn);
            });
        }

        Ok(())
    }
}

impl Default for ServerBuilder<()> {
    fn default() -> Self {
        Self::new()
    }
}
