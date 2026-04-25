use crate::{
    Server,
    base::Base,
    handle_client::handle_client,
    peer::Peer,
    plugin::{Context, Plugin},
};
use enet_rs::{Host, HostConfig};
use std::{net::SocketAddr, sync::Arc};
use tracing::error;

pub(crate) async fn run<P: Plugin>(
    bind_addr: SocketAddr,
    max_peers: usize,
    plugin: P,
) -> enet_rs::Result<()> {
    let mut host = Host::bind(
        bind_addr,
        HostConfig {
            max_peers,
            channel_count: sauer_proto::CHANNEL_COUNT as u8,
            use_checksum: false,
            incoming_bandwidth: 0,
            outgoing_bandwidth: 0,
        },
    )
    .await?;

    // Base is always the innermost plugin — user plugins run first and can cancel it.
    let plugin = Arc::new((Base, plugin));
    let mut server = Server::new();
    plugin.init(&mut server).await;

    let server = Arc::new(server);
    plugin.start(&server).await;

    let mut next_cn = 0u32;

    while let Some(peer) = host.accept().await {
        let (tx, rx) = peer.split();

        let cn = next_cn;
        next_cn = (next_cn + 1) % i32::MAX as u32;

        server.clients.insert(cn, Peer::new(tx));

        let plugin = Arc::clone(&plugin);
        let server = Arc::clone(&server);
        tokio::spawn(async move {
            let mut ctx = Context { server: Arc::clone(&server), cn, cancelled: false, map_changed: None };

            plugin.connected(&mut ctx).await;

            if !ctx.cancelled {
                if let Err(e) = handle_client(&server, &plugin, rx, cn).await {
                    error!("handling peer: {e}");
                }
            }

            ctx.cancelled = false;
            plugin.disconnected(&mut ctx).await;

            server.clients.remove(&cn);
        });
    }

    Ok(())
}
