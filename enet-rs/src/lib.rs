//! # enet-rs
//!
//! Async Rust implementation of the [ENet](http://enet.bespin.org/) reliable UDP protocol.
//!
//! ## Quick-start — Server
//!
//! ```rust,no_run
//! use enet_rs::{Host, HostConfig, Packet};
//!
//! #[tokio::main]
//! async fn main() -> Result<(), Box<dyn std::error::Error>> {
//!     let mut host = Host::bind("0.0.0.0:7777", HostConfig::default()).await?;
//!
//!     while let Some(mut peer) = host.accept().await {
//!         tokio::spawn(async move {
//!             loop {
//!                 match peer.recv().await {
//!                     Ok(pkt) => {
//!                         println!("ch{}: {} bytes", pkt.channel, pkt.data.len());
//!                         peer.send_packet(Packet::reliable(b"pong".as_ref(), 0)).await.ok();
//!                     }
//!                     Err(e) => { println!("peer gone: {e}"); break; }
//!                 }
//!             }
//!         });
//!     }
//!     Ok(())
//! }
//! ```
//!
//! ## Quick-start — Client
//!
//! ```rust,no_run
//! use enet_rs::{Host, HostConfig, Packet};
//!
//! #[tokio::main]
//! async fn main() -> Result<(), Box<dyn std::error::Error>> {
//!     let mut peer = Host::connect("127.0.0.1:7777", 2).await?;
//!     peer.send_packet(Packet::reliable(b"ping".as_ref(), 0)).await?;
//!     if let Ok(pkt) = peer.recv().await {
//!         println!("reply: {:?}", pkt.data);
//!     }
//!     Ok(())
//! }
//! ```

mod channel;
mod error;
mod peer;
mod peer_task;
mod protocol;
mod recv_task;

pub use error::{Error, Result};
pub use peer::{Peer, PeerReceiver, PeerSender};

use bytes::Bytes;
use std::{net::ToSocketAddrs, sync::Arc, time::Duration};
use tokio::{net::UdpSocket, sync::mpsc, time::timeout};

use recv_task::{HostCmd, RecvTask};

// ── Packet ────────────────────────────────────────────────────────────────────

/// Delivery semantics for a [`Packet`].
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SendMode {
    /// Reliable, ordered delivery (retransmitted until ACKed).
    Reliable,
    /// Unreliable, ordered delivery (late arrivals are discarded).
    Unreliable,
    /// Fire-and-forget, no ordering guarantees.
    Unsequenced,
}

/// A message sent to or received from a peer.
#[derive(Debug, Clone)]
pub struct Packet {
    /// The raw payload.
    pub data: Bytes,
    /// Channel index (0-based).
    pub channel: u8,
    /// Delivery semantics.
    pub mode: SendMode,
}

impl Packet {
    /// Create a reliable packet.
    pub fn reliable(data: impl Into<Bytes>, channel: u8) -> Self {
        Self {
            data: data.into(),
            channel,
            mode: SendMode::Reliable,
        }
    }

    /// Create an unreliable, sequenced packet.
    pub fn unreliable(data: impl Into<Bytes>, channel: u8) -> Self {
        Self {
            data: data.into(),
            channel,
            mode: SendMode::Unreliable,
        }
    }

    /// Create an unsequenced (fire-and-forget) packet.
    pub fn unsequenced(data: impl Into<Bytes>) -> Self {
        Self {
            data: data.into(),
            channel: 0,
            mode: SendMode::Unsequenced,
        }
    }
}

// ── HostConfig ────────────────────────────────────────────────────────────────

/// Configuration for a [`Host`].
#[derive(Debug, Clone)]
pub struct HostConfig {
    /// Maximum number of simultaneous peers (default: 64).
    pub max_peers: usize,
    /// Number of channels per peer (default: 2).
    pub channel_count: u8,
    /// Whether to compute and verify CRC-32 checksums (default: false).
    pub use_checksum: bool,
    /// Incoming bandwidth limit in bytes/sec (0 = unlimited).
    pub incoming_bandwidth: u32,
    /// Outgoing bandwidth limit in bytes/sec (0 = unlimited).
    pub outgoing_bandwidth: u32,
}

impl Default for HostConfig {
    fn default() -> Self {
        Self {
            max_peers: 64,
            channel_count: 2,
            use_checksum: false,
            incoming_bandwidth: 0,
            outgoing_bandwidth: 0,
        }
    }
}

// ── Host ─────────────────────────────────────────────────────────────────────

/// An ENet host (server or client-side connection manager).
///
/// Spawns a lightweight routing task and one async task per peer. User code
/// interacts via async channels — no manual event loops required.
pub struct Host {
    accept_rx: mpsc::Receiver<Peer>,
    /// Held to keep RecvTask alive; dropped when Host is dropped, causing RecvTask to exit.
    _host_cmd_tx: mpsc::Sender<HostCmd>,
}

impl Host {
    /// Bind a server host to `addr` and start the background tasks.
    pub async fn bind(addr: impl ToSocketAddrs, config: HostConfig) -> Result<Self> {
        let addr = addr
            .to_socket_addrs()
            .map_err(Error::Io)?
            .next()
            .ok_or_else(|| Error::Io(std::io::Error::other("no address")))?;

        let socket = Arc::new(UdpSocket::bind(addr).await?);
        let (accept_tx, accept_rx) = mpsc::channel(64);
        let (host_cmd_tx, host_cmd_rx) = mpsc::channel(256);

        let task = RecvTask::new(
            socket,
            accept_tx,
            host_cmd_rx,
            config.max_peers,
            config.use_checksum,
            config.incoming_bandwidth,
            config.outgoing_bandwidth,
        );
        tokio::spawn(task.run());

        Ok(Self {
            accept_rx,
            _host_cmd_tx: host_cmd_tx,
        })
    }

    /// Wait for the next incoming [`Peer`] connection.
    ///
    /// Returns `None` when the host task has shut down.
    pub async fn accept(&mut self) -> Option<Peer> {
        self.accept_rx.recv().await
    }

    /// Connect to a remote ENet server.
    pub async fn connect(addr: impl ToSocketAddrs, channel_count: u8) -> Result<Peer> {
        Self::connect_with_timeout(addr, channel_count, Duration::from_secs(5)).await
    }

    /// Like [`connect`](Self::connect) but with a custom timeout.
    pub async fn connect_with_timeout(
        addr: impl ToSocketAddrs,
        channel_count: u8,
        connect_timeout: Duration,
    ) -> Result<Peer> {
        let addr = addr
            .to_socket_addrs()
            .map_err(Error::Io)?
            .next()
            .ok_or_else(|| Error::Io(std::io::Error::other("no address")))?;

        let socket = Arc::new(UdpSocket::bind("0.0.0.0:0").await?);
        let (accept_tx, _) = mpsc::channel(1);
        let (host_cmd_tx, host_cmd_rx) = mpsc::channel(4);

        let task = RecvTask::new(socket, accept_tx, host_cmd_rx, 1, false, 0, 0);
        tokio::spawn(task.run());

        let (reply_tx, reply_rx) = tokio::sync::oneshot::channel();
        host_cmd_tx
            .send(HostCmd::Connect {
                addr,
                channel_count,
                data: 0,
                reply: reply_tx,
            })
            .await
            .map_err(|_| Error::ChannelClosed)?;

        match timeout(connect_timeout, reply_rx).await {
            Err(_elapsed) => Err(Error::TimedOut),
            Ok(Err(_canceled)) => Err(Error::ChannelClosed),
            Ok(Ok(result)) => result,
        }
    }
}
