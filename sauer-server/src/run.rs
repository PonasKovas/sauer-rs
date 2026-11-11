use crate::{
    Server,
    base::Base,
    handle_client::handle_client,
    peer::Peer,
    plugin::{Context, Flow, Plugin},
};
use lenet::HostBuilder;
use lenet_tokio::{Endpoint, EventQueuePolicy, LenetConfig};
use std::{
    net::SocketAddr,
    sync::{Arc, Weak},
};
use tracing::error;

/// Sauerbraten's `DISC_OVERFLOW` disconnect reason (`shared/iengine.h`).
const DISC_OVERFLOW: u32 = 9;
/// Packets a client may have waiting for its handler before it is kicked.
const MAX_QUEUED_EVENTS: usize = 4096;
/// Outgoing payload (queued plus unacknowledged) a client may make the
/// server hold before it is kicked. Twice the 16 MiB file transfer limit
/// (`engine/server.cpp`), so a map or demo download fits.
const MAX_OUTGOING_BYTES: usize = 32 << 20;

pub(crate) async fn run<P: Plugin>(
    bind_addr: SocketAddr,
    max_peers: usize,
    plugin: P,
) -> std::io::Result<()> {
    let endpoint = Endpoint::start(
        LenetConfig::new()
            .bind_addr(bind_addr)
            .host(
                HostBuilder::new()
                    .peer_count(max_peers)
                    .channel_limit(sauer_proto::CHANNEL_COUNT)
                    .checksum(false),
            )
            .event_queue(EventQueuePolicy::BoundedThenDisconnect {
                cap: MAX_QUEUED_EVENTS,
                disconnect_data: DISC_OVERFLOW,
            })
            .peer_outgoing_limit(MAX_OUTGOING_BYTES, DISC_OVERFLOW),
    )
    .await?;
    let mut accepts = endpoint.listen().await.map_err(std::io::Error::other)?;

    // Base is always the innermost plugin — user plugins run first and can cancel it.
    // Arc::new_cyclic lets Base hold a Weak back-reference to the full arc so it
    // can fire semantic hooks through the complete plugin chain.
    let plugin = Arc::new_cyclic(|weak: &Weak<(P, Base<P>)>| (plugin, Base::new(weak.clone())));
    let mut server = Server::new();
    plugin.init(&mut server).await;

    let mut extinfo_addr = bind_addr;
    extinfo_addr.set_port(bind_addr.port() + 1);
    let extinfo_socket = tokio::net::UdpSocket::bind(extinfo_addr).await?;
    let extinfo_socket = Arc::new(extinfo_socket);
    server
        .extinfo_socket
        .set(Arc::clone(&extinfo_socket))
        .unwrap();

    let server = Arc::new(server);
    plugin.start(&server).await;

    // spawn master server task
    let master_server = Arc::clone(&server);
    let server_port = bind_addr.port();
    tokio::spawn(async move {
        use sauer_proto::masterserver::{MasterClientRequest, MasterServerResponse};
        use tokio::io::{AsyncBufReadExt, AsyncWriteExt, BufReader};

        loop {
            if !master_server.config.read().update_master {
                tokio::time::sleep(std::time::Duration::from_secs(60)).await;
                continue;
            }

            let host = master_server.config.read().master_name.clone();
            let port = master_server.config.read().master_port;

            match tokio::net::TcpStream::connect((host.as_str(), port)).await {
                Ok(mut stream) => {
                    let req = MasterClientRequest::RegServ(server_port);
                    if let Err(e) = stream.write_all(req.to_string().as_bytes()).await {
                        tracing::error!(target: "sauer_server", "Failed to write to master server: {}", e);
                    } else {
                        let mut reader = BufReader::new(stream);
                        let mut line = String::new();

                        // We connected and sent regserv. Now we read responses or just keep connection alive.
                        // Sauerbraten sends `regserv` once per hour on the same connection, or reconnects if dropped.
                        let mut last_reg = tokio::time::Instant::now();

                        loop {
                            if !master_server.config.read().update_master {
                                break; // disconnect
                            }

                            // Send regserv every hour
                            if last_reg.elapsed() >= std::time::Duration::from_secs(60 * 60) {
                                let req = MasterClientRequest::RegServ(server_port);
                                if reader
                                    .get_mut()
                                    .write_all(req.to_string().as_bytes())
                                    .await
                                    .is_err()
                                {
                                    break;
                                }
                                last_reg = tokio::time::Instant::now();
                            }

                            // Wait for data or timeout
                            let read_fut = reader.read_line(&mut line);
                            let timeout_fut =
                                tokio::time::sleep(std::time::Duration::from_secs(10));

                            tokio::select! {
                                res = read_fut => {
                                    match res {
                                        Ok(0) => break, // EOF
                                        Ok(_) => {
                                            match MasterServerResponse::parse(&line) {
                                                Some(MasterServerResponse::SuccReg) => {
                                                    tracing::info!(target: "sauer_server", "Master server registration succeeded");
                                                }
                                                Some(MasterServerResponse::FailReg(reason)) => {
                                                    tracing::error!(target: "sauer_server", "Master server registration failed: {}", reason);
                                                }
                                                Some(other) => {
                                                    tracing::debug!(target: "sauer_server", "Master server: {:?}", other);
                                                }
                                                None => {
                                                    tracing::debug!(target: "sauer_server", "Unknown master server message: {}", line.trim());
                                                }
                                            }
                                            line.clear();
                                        }
                                        Err(e) => {
                                            tracing::error!(target: "sauer_server", "Error reading from master server: {}", e);
                                            break;
                                        }
                                    }
                                }
                                _ = timeout_fut => {
                                    // Just a wake-up to check update_master and hour timeout
                                }
                            }
                        }
                    }
                }
                Err(e) => {
                    tracing::error!(target: "sauer_server", "Failed to connect to master server {}:{}: {}", host, port, e);
                }
            }

            // If disconnected or failed, wait before retrying
            tokio::time::sleep(std::time::Duration::from_secs(60)).await;
        }
    });

    // spawn UDP reader task for server browser pings & ExtInfo
    let ext_plugin = Arc::clone(&plugin);
    let ext_server = Arc::clone(&server);
    tokio::spawn(async move {
        let mut buf = [0u8; 1024];
        loop {
            match extinfo_socket.recv_from(&mut buf).await {
                Ok((len, addr)) => {
                    let data = &buf[..len];
                    match sauer_proto::server_query::ServerQueryRequest::parse(data) {
                        Ok(req) => {
                            let mut ctx = crate::plugin::ServerQueryContext {
                                server: Arc::clone(&ext_server),
                                addr,
                            };
                            let _ = ext_plugin.server_query(&mut ctx, &req).await;
                        }
                        Err(e) => {
                            tracing::debug!(target: "sauer_server", "Failed to parse ServerQuery UDP packet from {}: {:?}", addr, e);
                        }
                    }
                }
                Err(e) => {
                    tracing::error!(target: "sauer_server", "ServerQuery UDP recv error: {}", e);
                }
            }
        }
    });

    let mut next_id = 0u64;

    while let Some(incoming) = accepts.accept().await {
        let (tx, rx) = incoming.accept().split();

        let id = next_id;
        next_id += 1;

        server.clients.insert(id, Peer::new(tx));

        let plugin = Arc::clone(&plugin);
        let server = Arc::clone(&server);
        tokio::spawn(async move {
            // The session and the `disconnected` hook each run in their own
            // task, so a panic in a hook can't skip the cleanup that frees the
            // client's cn and player data.
            let session = tokio::spawn(client_session(
                Arc::clone(&server),
                Arc::clone(&plugin),
                rx,
                id,
            ));
            if let Err(e) = session.await {
                error!("client {id} session panicked: {e}");
                server.disconnect(id);
            }

            let cleanup = tokio::spawn({
                let server = Arc::clone(&server);
                let plugin = Arc::clone(&plugin);
                async move {
                    let mut ctx = Context { server, id };
                    plugin.disconnected(&mut ctx).await;
                }
            });
            if let Err(e) = cleanup.await {
                error!("client {id} disconnect hook panicked: {e}");
            }

            server.clients.remove(&id);
        });
    }

    Ok(())
}

async fn client_session<P: Plugin>(
    server: Arc<Server>,
    plugin: Arc<P>,
    rx: lenet_tokio::ConnectionReceiver,
    id: u64,
) {
    let mut ctx = Context {
        server: Arc::clone(&server),
        id,
    };

    if plugin.connected(&mut ctx).await == Flow::Continue {
        if let Err(e) = handle_client(&server, &plugin, rx, id).await {
            error!("handling peer: {e}");
        }
    }
}
