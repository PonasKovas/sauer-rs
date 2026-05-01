use crate::{
    Server,
    base::Base,
    handle_client::handle_client,
    peer::Peer,
    plugin::{Context, Plugin},
};
use enet_rs::{Host, HostConfig};
use std::{
    net::SocketAddr,
    sync::{Arc, Weak},
};
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
    // Arc::new_cyclic lets Base hold a Weak back-reference to the full arc so it
    // can fire semantic hooks through the complete plugin chain.
    let plugin = Arc::new_cyclic(|weak: &Weak<(P, Base<P>)>| (plugin, Base::new(weak.clone())));
    let mut server = Server::new();
    plugin.init(&mut server).await;

    let mut extinfo_addr = bind_addr;
    extinfo_addr.set_port(bind_addr.port() + 1);
    let extinfo_socket = tokio::net::UdpSocket::bind(extinfo_addr).await?;
    let extinfo_socket = Arc::new(extinfo_socket);
    server.extinfo_socket.set(Arc::clone(&extinfo_socket)).unwrap();

    let server = Arc::new(server);
    plugin.start(&server).await;

    // spawn master server task
    let master_server = Arc::clone(&server);
    let server_port = bind_addr.port();
    tokio::spawn(async move {
        use tokio::io::{AsyncBufReadExt, AsyncWriteExt, BufReader};
        use sauer_proto::masterserver::{MasterClientRequest, MasterServerResponse};
        
        loop {
            if !*master_server.update_master.read().unwrap() {
                tokio::time::sleep(std::time::Duration::from_secs(60)).await;
                continue;
            }

            let host = master_server.master_name.read().unwrap().clone();
            let port = master_server.master_port;
            
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
                            if !*master_server.update_master.read().unwrap() {
                                break; // disconnect
                            }

                            // Send regserv every hour
                            if last_reg.elapsed() >= std::time::Duration::from_secs(60 * 60) {
                                let req = MasterClientRequest::RegServ(server_port);
                                if reader.get_mut().write_all(req.to_string().as_bytes()).await.is_err() {
                                    break;
                                }
                                last_reg = tokio::time::Instant::now();
                            }

                            // Wait for data or timeout
                            let read_fut = reader.read_line(&mut line);
                            let timeout_fut = tokio::time::sleep(std::time::Duration::from_secs(10));
                            
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
                                cancelled: false,
                            };
                            ext_plugin.server_query(&mut ctx, &req).await;
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

    let mut next_cn = 0u32;

    while let Some(peer) = host.accept().await {
        let (tx, rx) = peer.split();

        let cn = next_cn;
        next_cn = (next_cn + 1) % i32::MAX as u32;

        server.clients.insert(cn, Peer::new(tx));

        let plugin = Arc::clone(&plugin);
        let server = Arc::clone(&server);
        tokio::spawn(async move {
            let mut ctx = Context {
                server: Arc::clone(&server),
                cn,
                cancelled: false,
            };

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
