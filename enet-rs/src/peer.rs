use std::{
    net::SocketAddr,
    pin::Pin,
    task::{Context, Poll},
};

use futures::{Sink, Stream};
use tokio::sync::mpsc;
use tokio_util::sync::PollSender;

use crate::{
    error::{Error, Result},
    peer_task::{PeerCmd, PeerMsg},
    Packet,
};

// ── PeerSender ────────────────────────────────────────────────────────────────

/// The sending half of a [`Peer`] connection.
///
/// Implements [`Sink<Packet>`]. Dropping this triggers a graceful disconnect.
pub struct PeerSender {
    cmd_tx: mpsc::Sender<PeerCmd>,
    poll_sender: PollSender<PeerCmd>,
    pub(crate) addr: SocketAddr,
}

impl PeerSender {
    pub(crate) fn new(cmd_tx: mpsc::Sender<PeerCmd>, addr: SocketAddr) -> Self {
        let poll_sender = PollSender::new(cmd_tx.clone());
        Self { cmd_tx, poll_sender, addr }
    }

    /// The remote address of the peer.
    pub fn addr(&self) -> SocketAddr {
        self.addr
    }

    /// Send a packet, waiting until the internal send buffer has room.
    pub async fn send_packet(&self, pkt: Packet) -> Result<()> {
        self.cmd_tx.send(PeerCmd::Send(pkt)).await.map_err(|_| Error::ChannelClosed)
    }

    /// Send a packet without blocking.
    ///
    /// Returns `Err(Error::ChannelFull)` if the internal send buffer is at capacity.
    pub fn try_send_packet(&self, pkt: Packet) -> Result<()> {
        self.cmd_tx.try_send(PeerCmd::Send(pkt)).map_err(|e| match e {
            tokio::sync::mpsc::error::TrySendError::Full(_) => Error::ChannelFull,
            tokio::sync::mpsc::error::TrySendError::Closed(_) => Error::ChannelClosed,
        })
    }

    /// Initiate a graceful disconnect without blocking.
    pub fn try_disconnect(&self, data: u32) {
        let _ = self.cmd_tx.try_send(PeerCmd::Disconnect { data });
    }

    /// Initiate a graceful disconnect.
    pub async fn disconnect(&self, data: u32) -> Result<()> {
        self.cmd_tx.send(PeerCmd::Disconnect { data }).await.map_err(|_| Error::ChannelClosed)
    }
}

impl Drop for PeerSender {
    fn drop(&mut self) {
        // Best-effort disconnect when sender is dropped without an explicit call.
        // PeerTask also detects cmd_rx closure as a fallback.
        let _ = self.cmd_tx.try_send(PeerCmd::Disconnect { data: 0 });
    }
}

impl Sink<Packet> for PeerSender {
    type Error = Error;

    fn poll_ready(mut self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Result<()>> {
        Pin::new(&mut self.poll_sender)
            .poll_reserve(cx)
            .map_err(|_| Error::ChannelClosed)
    }

    fn start_send(mut self: Pin<&mut Self>, item: Packet) -> Result<()> {
        Pin::new(&mut self.poll_sender)
            .send_item(PeerCmd::Send(item))
            .map_err(|_| Error::ChannelClosed)
    }

    fn poll_flush(self: Pin<&mut Self>, _cx: &mut Context<'_>) -> Poll<Result<()>> {
        Poll::Ready(Ok(()))
    }

    fn poll_close(mut self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Result<()>> {
        Pin::new(&mut self.poll_sender)
            .poll_close(cx)
            .map_err(|_| Error::ChannelClosed)
    }
}

// ── PeerReceiver ──────────────────────────────────────────────────────────────

/// The receiving half of a [`Peer`] connection.
///
/// Implements [`Stream<Item = Result<Packet>>`].
pub struct PeerReceiver {
    pub rx: mpsc::Receiver<PeerMsg>,
    pub(crate) addr: SocketAddr,
}

impl PeerReceiver {
    /// The remote address of the peer.
    pub fn addr(&self) -> SocketAddr {
        self.addr
    }
}

impl Stream for PeerReceiver {
    type Item = Result<Packet>;

    fn poll_next(mut self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Option<Result<Packet>>> {
        match self.rx.poll_recv(cx) {
            Poll::Ready(Some(Ok(pkt))) => Poll::Ready(Some(Ok(pkt))),
            Poll::Ready(Some(Err(data))) => Poll::Ready(Some(Err(Error::Disconnected(data)))),
            Poll::Ready(None) => Poll::Ready(None),
            Poll::Pending => Poll::Pending,
        }
    }
}

// ── Peer ──────────────────────────────────────────────────────────────────────

/// A connected ENet peer.
///
/// Implements [`Sink<Packet>`] and [`Stream<Item = Result<Packet>>`].
/// Dropping this (or the [`PeerSender`] half after a split) triggers a graceful disconnect.
pub struct Peer {
    sender: PeerSender,
    receiver: PeerReceiver,
}

impl Peer {
    pub(crate) fn new(
        cmd_tx: mpsc::Sender<PeerCmd>,
        rx: mpsc::Receiver<PeerMsg>,
        addr: SocketAddr,
    ) -> Self {
        Self {
            sender: PeerSender::new(cmd_tx, addr),
            receiver: PeerReceiver { rx, addr },
        }
    }

    /// Split into independent sender and receiver halves.
    pub fn split(self) -> (PeerSender, PeerReceiver) {
        (self.sender, self.receiver)
    }

    /// The remote address of this peer.
    pub fn addr(&self) -> SocketAddr {
        self.sender.addr
    }

    /// Send a packet.
    pub async fn send_packet(&self, pkt: Packet) -> Result<()> {
        self.sender.send_packet(pkt).await
    }

    /// Receive the next packet or disconnect event.
    ///
    /// - `Ok(pkt)` — data arrived.
    /// - `Err(Disconnected(Some(data)))` — remote sent a graceful disconnect.
    /// - `Err(Disconnected(None))` — connection timed out.
    /// - `Err(ChannelClosed)` — the host task shut down.
    pub async fn recv(&mut self) -> Result<Packet> {
        match self.receiver.rx.recv().await {
            Some(Ok(pkt)) => Ok(pkt),
            Some(Err(data)) => Err(Error::Disconnected(data)),
            None => Err(Error::ChannelClosed),
        }
    }

    /// Initiate a graceful disconnect.
    pub async fn disconnect(&self, data: u32) -> Result<()> {
        self.sender.disconnect(data).await
    }
}

impl Sink<Packet> for Peer {
    type Error = Error;
    fn poll_ready(mut self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Result<()>> {
        Pin::new(&mut self.sender).poll_ready(cx)
    }
    fn start_send(mut self: Pin<&mut Self>, item: Packet) -> Result<()> {
        Pin::new(&mut self.sender).start_send(item)
    }
    fn poll_flush(mut self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Result<()>> {
        Pin::new(&mut self.sender).poll_flush(cx)
    }
    fn poll_close(mut self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Result<()>> {
        Pin::new(&mut self.sender).poll_close(cx)
    }
}

impl Stream for Peer {
    type Item = Result<Packet>;
    fn poll_next(mut self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Option<Result<Packet>>> {
        Pin::new(&mut self.receiver).poll_next(cx)
    }
}
