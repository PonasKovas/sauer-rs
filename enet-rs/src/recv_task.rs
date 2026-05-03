/// RecvTask — owns the UDP socket receive loop and routes datagrams to PeerTasks.
use std::collections::HashMap;
use std::net::SocketAddr;
use std::sync::Arc;
use std::time::{Duration, Instant};

use bytes::{Bytes, BytesMut};
use tokio::net::UdpSocket;
use tokio::sync::{mpsc, oneshot};
use tokio::time;
use tracing::warn;

use crate::error::Error;
use crate::peer::Peer;
use crate::peer_task::{InboundMsg, PeerTask};
use crate::protocol::{
    BANDWIDTH_THROTTLE_INTERVAL_MS, CHANNEL_ID_CONNECTION, CMD_CONNECT, CMD_DISCONNECT,
    HEADER_FLAG_SENT_TIME, HEADER_PEER_ID_MAX, MTU_DEFAULT,
};

/// Opaque handle identifying a peer's slot in the routing table.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub(crate) struct PeerKey(pub usize);

/// Notification sent from a PeerTask back to RecvTask.
pub(crate) enum PeerNotify {
    Removed { key: PeerKey },
}

/// Commands sent from the `Host` API into the RecvTask.
pub(crate) enum HostCmd {
    Connect {
        addr: SocketAddr,
        channel_count: u8,
        data: u32,
        reply: oneshot::Sender<Result<Peer, Error>>,
    },
}

struct PeerSlot {
    addr: SocketAddr,
    inbox_tx: mpsc::Sender<InboundMsg>,
}

pub(crate) struct RecvTask {
    socket: Arc<UdpSocket>,
    slots: Vec<Option<PeerSlot>>,
    addr_map: HashMap<SocketAddr, PeerKey>,
    accept_tx: mpsc::Sender<Peer>,
    host_cmd_rx: mpsc::Receiver<HostCmd>,
    notify_rx: mpsc::Receiver<PeerNotify>,
    notify_tx: mpsc::Sender<PeerNotify>,
    epoch: Instant,
    max_peers: usize,
    use_checksum: bool,
    incoming_bw: u32,
    outgoing_bw: u32,
    host_mtu: u32,
}

impl RecvTask {
    pub fn new(
        socket: Arc<UdpSocket>,
        accept_tx: mpsc::Sender<Peer>,
        host_cmd_rx: mpsc::Receiver<HostCmd>,
        max_peers: usize,
        use_checksum: bool,
        incoming_bw: u32,
        outgoing_bw: u32,
    ) -> Self {
        let (notify_tx, notify_rx) = mpsc::channel(128);
        let mut slots = Vec::with_capacity(max_peers);
        slots.resize_with(max_peers, || None);
        Self {
            socket,
            slots,
            addr_map: HashMap::new(),
            accept_tx,
            host_cmd_rx,
            notify_rx,
            notify_tx,
            epoch: Instant::now(),
            max_peers,
            use_checksum,
            incoming_bw,
            outgoing_bw,
            host_mtu: MTU_DEFAULT,
        }
    }

    fn now_ms(&self) -> u32 {
        self.epoch.elapsed().as_millis() as u32
    }

    fn alloc_slot(&self) -> Option<usize> {
        self.slots.iter().position(|s| s.is_none())
    }

    fn active_count(&self) -> usize {
        self.slots.iter().filter(|s| s.is_some()).count()
    }

    fn remove_slot(&mut self, key: PeerKey) {
        if let Some(Some(slot)) = self.slots.get(key.0) {
            self.addr_map.remove(&slot.addr);
        }
        if let Some(s) = self.slots.get_mut(key.0) {
            *s = None;
        }
    }

    fn spawn_server_peer(&mut self, from: SocketAddr) -> Option<PeerKey> {
        let idx = self.alloc_slot()?;
        let key = PeerKey(idx);
        let (inbox_tx, inbox_rx) = mpsc::channel(256);

        let task = PeerTask::new_server(
            key,
            Arc::clone(&self.socket),
            inbox_rx,
            self.notify_tx.clone(),
            self.accept_tx.clone(),
            from,
            self.use_checksum,
            self.host_mtu,
            self.incoming_bw,
            self.outgoing_bw,
        );
        tokio::spawn(task.run());

        self.slots[idx] = Some(PeerSlot { addr: from, inbox_tx });
        self.addr_map.insert(from, key);
        Some(key)
    }

    fn spawn_client_peer(
        &mut self,
        addr: SocketAddr,
        channel_count: u8,
        user_data: u32,
        reply: oneshot::Sender<Result<Peer, Error>>,
    ) -> Option<PeerKey> {
        let idx = self.alloc_slot()?;
        let key = PeerKey(idx);
        let (inbox_tx, inbox_rx) = mpsc::channel(256);

        let task = PeerTask::new_client(
            key,
            Arc::clone(&self.socket),
            inbox_rx,
            self.notify_tx.clone(),
            addr,
            channel_count,
            user_data,
            reply,
            self.use_checksum,
            self.host_mtu,
            self.incoming_bw,
            self.outgoing_bw,
        );
        tokio::spawn(task.run());

        self.slots[idx] = Some(PeerSlot { addr, inbox_tx });
        self.addr_map.insert(addr, key);
        Some(key)
    }

    async fn route_datagram(&mut self, raw: &[u8], from: SocketAddr) {
        use crate::protocol::ProtocolHeader;

        let mut slice = raw;
        let Some(hdr) = ProtocolHeader::parse(&mut slice) else { return };
        if hdr.is_compressed() {
            return;
        }

        let peer_id = hdr.peer_id();
        let first_cmd_type = slice.first().map(|b| b & 0x0F);

        if first_cmd_type == Some(CMD_CONNECT) {
            // Evict any existing peer from this address (reconnect case).
            if let Some(old_key) = self.addr_map.get(&from).copied() {
                if let Some(Some(slot)) = self.slots.get(old_key.0) {
                    let _ = slot.inbox_tx.try_send(InboundMsg::Evicted);
                }
                self.remove_slot(old_key);
            }

            if self.active_count() >= self.max_peers {
                self.send_reject(slice, from).await;
                return;
            }

            let Some(key) = self.spawn_server_peer(from) else { return };
            if let Some(Some(slot)) = self.slots.get(key.0) {
                let data = Bytes::copy_from_slice(raw);
                let _ = slot.inbox_tx.send(InboundMsg::Datagram(data)).await;
            }
            return;
        }

        let key = if peer_id == HEADER_PEER_ID_MAX {
            self.addr_map.get(&from).copied()
        } else if (peer_id as usize) < self.slots.len() {
            Some(PeerKey(peer_id as usize))
        } else {
            None
        };

        let Some(key) = key else { return };

        if let Some(Some(slot)) = self.slots.get(key.0) {
            let data = Bytes::copy_from_slice(raw);
            if slot.inbox_tx.send(InboundMsg::Datagram(data)).await.is_err() {
                self.remove_slot(key);
            }
        }
    }

    /// Send a raw CMD_DISCONNECT to reject a connection when no slot is available.
    async fn send_reject(&self, cmd_payload: &[u8], from: SocketAddr) {
        // cmd_payload is the slice after the protocol header (starts with the command header).
        // CONNECT command header is 4 bytes, then outgoing_peer_id is the first 2 payload bytes.
        if cmd_payload.len() < 6 {
            return;
        }
        let outgoing_peer_id = u16::from_be_bytes([cmd_payload[4], cmd_payload[5]]);
        let now_ms = self.now_ms();

        let mut buf = BytesMut::new();
        let peer_id_raw = HEADER_FLAG_SENT_TIME | (outgoing_peer_id & HEADER_PEER_ID_MAX);
        buf.extend_from_slice(&peer_id_raw.to_be_bytes());
        buf.extend_from_slice(&(now_ms as u16).to_be_bytes());
        buf.extend_from_slice(&[CMD_DISCONNECT, CHANNEL_ID_CONNECTION, 0, 0, 0, 0, 0, 0]);
        let _ = self.socket.send_to(&buf, from).await;
    }

    async fn broadcast_bw_update(&self) {
        if self.incoming_bw == 0 && self.outgoing_bw == 0 {
            return;
        }
        let count = self.active_count() as u32;
        if count == 0 {
            return;
        }
        let per_peer_ob = if self.outgoing_bw > 0 { self.outgoing_bw / count } else { 0 };
        let msg = InboundMsg::BandwidthUpdate { incoming_bw: self.incoming_bw, per_peer_ob };
        for slot in self.slots.iter().flatten() {
            let _ = slot.inbox_tx.try_send(msg.clone());
        }
    }

    pub async fn run(mut self) {
        let mut buf = vec![0u8; 65536];
        let mut bw_tick =
            time::interval(Duration::from_millis(BANDWIDTH_THROTTLE_INTERVAL_MS));
        bw_tick.set_missed_tick_behavior(time::MissedTickBehavior::Skip);

        // When the Host is dropped (host_cmd_tx dropped), host_gone becomes true.
        // RecvTask then exits only after all active peers also disconnect.
        // This keeps client-side RecvTasks alive for the lifetime of their PeerTask.
        let mut host_gone = false;

        loop {
            tokio::select! {
                res = self.socket.recv_from(&mut buf) => match res {
                    Ok((len, from)) => self.route_datagram(&buf[..len], from).await,
                    Err(e) => warn!("recv_from: {e}"),
                },
                msg = self.host_cmd_rx.recv(), if !host_gone => match msg {
                    None => {
                        host_gone = true;
                        if self.active_count() == 0 {
                            break;
                        }
                    }
                    Some(HostCmd::Connect { addr, channel_count, data, reply }) => {
                        if self.spawn_client_peer(addr, channel_count, data, reply).is_none() {
                            // No slot — drop reply, caller will get TimedOut/ChannelClosed.
                        }
                    }
                },
                notify = self.notify_rx.recv() => match notify {
                    None => break,
                    Some(PeerNotify::Removed { key }) => {
                        self.remove_slot(key);
                        if host_gone && self.active_count() == 0 {
                            break;
                        }
                    }
                },
                _ = bw_tick.tick() => self.broadcast_bw_update().await,
            }
        }
    }
}
