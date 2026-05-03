/// PeerTask — owns a single ENet connection's protocol state machine.
mod rx;
mod tx;

use std::collections::{BTreeMap, VecDeque};
use std::net::SocketAddr;
use std::sync::Arc;
use std::time::{Duration, Instant};

use bytes::Bytes;
use rand::Rng;
use tokio::net::UdpSocket;
use tokio::sync::{mpsc, oneshot};
use tokio::time;

use crate::channel::{Channel, UnsequencedWindow};
use crate::error::Error;
use crate::peer::Peer;
use crate::protocol::{
    DEFAULT_PACKET_THROTTLE, DEFAULT_PING_INTERVAL_MS, DEFAULT_RTT_MS, THROTTLE_ACCELERATION,
    THROTTLE_DECELERATION, THROTTLE_INTERVAL_MS, TIMEOUT_LIMIT, TIMEOUT_MAXIMUM_MS,
    TIMEOUT_MINIMUM_MS, WINDOW_SIZE_MAX,
};
use crate::recv_task::{PeerKey, PeerNotify};
use crate::Packet;

pub(crate) type PeerMsg = Result<Packet, Option<u32>>;

/// Datagram or control message sent from RecvTask → PeerTask.
#[derive(Clone)]
pub(crate) enum InboundMsg {
    Datagram(Bytes),
    BandwidthUpdate { incoming_bw: u32, per_peer_ob: u32 },
    Evicted,
}

/// Command from the user-facing Peer handle → PeerTask.
pub(crate) enum PeerCmd {
    Send(Packet),
    Disconnect { data: u32 },
}

// ── Peer state ────────────────────────────────────────────────────────────────

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum PeerState {
    Connecting,
    AcknowledgingConnect,
    Connected,
    DisconnectLater(u32),
    Disconnecting,
}

pub(crate) struct OutstandingReliable {
    pub reliable_seq: u16,
    pub channel_id: u8,
    pub original_sent_time: u32,
    pub sent_time: u32,
    pub rtt_timeout: u32,
    pub send_attempts: u32,
    pub packet_bytes: Bytes,
}

// ── PeerTask ──────────────────────────────────────────────────────────────────

pub(crate) struct PeerTask {
    // Identity
    pub(super) key: PeerKey,
    pub(super) addr: SocketAddr,

    // Channels
    pub(super) socket: Arc<UdpSocket>,
    pub(super) inbox_rx: mpsc::Receiver<InboundMsg>,
    pub(super) cmd_tx: Option<mpsc::Sender<PeerCmd>>,
    pub(super) cmd_rx: mpsc::Receiver<PeerCmd>,
    pub(super) to_user_tx: mpsc::Sender<PeerMsg>,
    pub(super) to_user_rx: Option<mpsc::Receiver<PeerMsg>>,
    pub(super) notify_tx: mpsc::Sender<PeerNotify>,

    // How the Peer handle reaches the user
    pub(super) accept_tx: Option<mpsc::Sender<Peer>>,
    pub(super) connect_reply: Option<oneshot::Sender<Result<Peer, Error>>>,

    // Protocol state
    pub(super) state: PeerState,
    pub(super) outgoing_peer_id: u16,
    pub(super) connect_id: u32,
    pub(super) incoming_session_id: u8,
    pub(super) outgoing_session_id: u8,
    pub(super) mtu: u32,
    pub(super) window_size: u32,
    pub(super) packet_throttle: u32,
    pub(super) throttle_interval: u32,
    pub(super) throttle_acceleration: u32,
    pub(super) throttle_deceleration: u32,
    pub(super) rtt: u32,
    pub(super) rtt_var: u32,
    pub(super) rtt_initialized: bool,
    pub(super) last_send_time: u32,
    pub(super) last_receive_time: u32,
    pub(super) channels: Vec<Channel>,
    pub(super) unsequenced_window: UnsequencedWindow,
    pub(super) outgoing_reliable: BTreeMap<(u8, u16), OutstandingReliable>,
    pub(super) pending_send: VecDeque<Packet>,
    pub(super) fragment_bytes_in_flight: usize,
    pub(super) packet_throttle_epoch: u32,
    pub(super) rtt_at_throttle_epoch: u32,
    pub(super) connect_user_data: u32,
    pub(super) outgoing_reliable_seq: u16,
    pub(super) connect_seq: u16,

    // Immutable config
    pub(super) use_checksum: bool,
    pub(super) host_mtu: u32,
    pub(super) incoming_bw: u32,
    pub(super) outgoing_bw: u32,
    pub(super) epoch: Instant,

    pub(super) removed: bool,
}

#[allow(clippy::too_many_arguments)]
impl PeerTask {
    fn base(
        key: PeerKey,
        socket: Arc<UdpSocket>,
        inbox_rx: mpsc::Receiver<InboundMsg>,
        notify_tx: mpsc::Sender<PeerNotify>,
        addr: SocketAddr,
        use_checksum: bool,
        host_mtu: u32,
        incoming_bw: u32,
        outgoing_bw: u32,
    ) -> Self {
        let (cmd_tx, cmd_rx) = mpsc::channel(64);
        let (to_user_tx, to_user_rx) = mpsc::channel(256);
        Self {
            key,
            addr,
            socket,
            inbox_rx,
            cmd_tx: Some(cmd_tx),
            cmd_rx,
            to_user_tx,
            to_user_rx: Some(to_user_rx),
            notify_tx,
            accept_tx: None,
            connect_reply: None,
            state: PeerState::Connecting,
            outgoing_peer_id: crate::protocol::PEER_ID_UNASSIGNED,
            connect_id: 0,
            incoming_session_id: 0xFF,
            outgoing_session_id: 0xFF,
            mtu: host_mtu,
            window_size: WINDOW_SIZE_MAX,
            packet_throttle: DEFAULT_PACKET_THROTTLE,
            throttle_interval: THROTTLE_INTERVAL_MS,
            throttle_acceleration: THROTTLE_ACCELERATION,
            throttle_deceleration: THROTTLE_DECELERATION,
            rtt: DEFAULT_RTT_MS,
            rtt_var: DEFAULT_RTT_MS / 2,
            rtt_initialized: false,
            last_send_time: 0,
            last_receive_time: 0,
            channels: Vec::new(),
            unsequenced_window: UnsequencedWindow::new(),
            outgoing_reliable: BTreeMap::new(),
            pending_send: VecDeque::new(),
            fragment_bytes_in_flight: 0,
            packet_throttle_epoch: 0,
            rtt_at_throttle_epoch: DEFAULT_RTT_MS,
            connect_user_data: 0,
            outgoing_reliable_seq: 0,
            connect_seq: 0,
            use_checksum,
            host_mtu,
            incoming_bw,
            outgoing_bw,
            epoch: Instant::now(),
            removed: false,
        }
    }

    pub(crate) fn new_server(
        key: PeerKey,
        socket: Arc<UdpSocket>,
        inbox_rx: mpsc::Receiver<InboundMsg>,
        notify_tx: mpsc::Sender<PeerNotify>,
        accept_tx: mpsc::Sender<Peer>,
        from: SocketAddr,
        use_checksum: bool,
        host_mtu: u32,
        incoming_bw: u32,
        outgoing_bw: u32,
    ) -> Self {
        let mut t = Self::base(key, socket, inbox_rx, notify_tx, from, use_checksum, host_mtu, incoming_bw, outgoing_bw);
        t.accept_tx = Some(accept_tx);
        t.state = PeerState::AcknowledgingConnect;
        t
    }

    pub(crate) fn new_client(
        key: PeerKey,
        socket: Arc<UdpSocket>,
        inbox_rx: mpsc::Receiver<InboundMsg>,
        notify_tx: mpsc::Sender<PeerNotify>,
        addr: SocketAddr,
        channel_count: u8,
        user_data: u32,
        reply: oneshot::Sender<Result<Peer, Error>>,
        use_checksum: bool,
        host_mtu: u32,
        incoming_bw: u32,
        outgoing_bw: u32,
    ) -> Self {
        let mut t = Self::base(key, socket, inbox_rx, notify_tx, addr, use_checksum, host_mtu, incoming_bw, outgoing_bw);
        t.connect_id = rand::thread_rng().gen();
        t.channels = (0..channel_count.max(1)).map(|_| Channel::new()).collect();
        t.connect_reply = Some(reply);
        t.connect_user_data = user_data;
        t
    }

    pub(super) fn now_ms(&self) -> u32 {
        self.epoch.elapsed().as_millis() as u32
    }

    pub(super) fn update_rtt(&mut self, sample: u32) {
        if !self.rtt_initialized {
            self.rtt = sample;
            self.rtt_var = sample.div_ceil(2);
            self.rtt_initialized = true;
        } else {
            self.rtt_var = self.rtt_var.saturating_sub(self.rtt_var / 4);
            if sample >= self.rtt {
                let diff = sample - self.rtt;
                self.rtt_var += diff / 4;
                self.rtt += diff / 8;
            } else {
                let diff = self.rtt - sample;
                self.rtt_var += diff / 4;
                self.rtt = self.rtt.saturating_sub(diff / 8);
            }
        }
    }

    pub(super) fn initial_rtt_timeout(&self) -> u32 {
        self.rtt.saturating_add(self.rtt_var.saturating_mul(4)).max(200)
    }

    pub(super) fn next_connect_seq(&mut self) -> u16 {
        self.outgoing_reliable_seq = self.outgoing_reliable_seq.wrapping_add(1);
        self.outgoing_reliable_seq
    }

    pub(super) fn take_peer(&mut self) -> Peer {
        let cmd_tx = self.cmd_tx.take().expect("cmd_tx already taken");
        let to_user_rx = self.to_user_rx.take().expect("to_user_rx already taken");
        Peer::new(cmd_tx, to_user_rx, self.addr)
    }

    // ── Service (retransmit / ping / deferred disconnect) ─────────────────────

    pub(super) async fn service(&mut self, now_ms: u32) {
        let (timed_out, retransmits, ping_needed, disconnect_later) =
            self.collect_service_actions(now_ms);

        if timed_out {
            let _ = self.to_user_tx.try_send(Err(None));
            self.removed = true;
            return;
        }

        if !retransmits.is_empty() {
            self.flush_batch(retransmits, now_ms).await;
        }

        if ping_needed {
            let ping = self.make_reliable(
                crate::protocol::CHANNEL_ID_CONNECTION,
                crate::protocol::CMD_PING,
                0,
                now_ms,
                |_| {},
            );
            self.emit(&[ping], now_ms).await;
        }

        if let Some(data) = disconnect_later {
            self.state = PeerState::Disconnecting;
            let cmd = self.make_reliable(
                crate::protocol::CHANNEL_ID_CONNECTION,
                crate::protocol::CMD_DISCONNECT,
                0,
                now_ms,
                move |buf| buf.extend_from_slice(&data.to_be_bytes()),
            );
            self.emit(&[cmd], now_ms).await;
        }

        // Drain packets that were queued because the reliable window was full.
        let pending: Vec<Packet> = self.pending_send.drain(..).collect();
        for pkt in pending {
            self.send_packet(pkt, now_ms).await;
        }
    }

    fn collect_service_actions(
        &mut self,
        now_ms: u32,
    ) -> (bool, Vec<Bytes>, bool, Option<u32>) {
        let mut timed_out = false;
        let mut retransmits = Vec::new();

        for r in self.outgoing_reliable.values_mut() {
            let elapsed_since_last = now_ms.wrapping_sub(r.sent_time);
            if elapsed_since_last < r.rtt_timeout {
                continue;
            }
            let elapsed = now_ms.wrapping_sub(r.original_sent_time);
            let shift = r.send_attempts.saturating_sub(1).min(31);
            let attempts_over = (1u32 << shift) >= TIMEOUT_LIMIT;
            if elapsed >= TIMEOUT_MAXIMUM_MS || (attempts_over && elapsed >= TIMEOUT_MINIMUM_MS) {
                timed_out = true;
                break;
            }
            r.send_attempts += 1;
            r.rtt_timeout = r.rtt_timeout.saturating_mul(2);
            r.sent_time = now_ms;
            retransmits.push(r.packet_bytes.clone());
        }

        let ping_needed = !timed_out
            && self.state == PeerState::Connected
            && self.outgoing_reliable.is_empty()
            && now_ms.wrapping_sub(self.last_send_time) >= DEFAULT_PING_INTERVAL_MS as u32;

        let disconnect_later = if !timed_out {
            if let PeerState::DisconnectLater(data) = self.state {
                if self.outgoing_reliable.is_empty() { Some(data) } else { None }
            } else {
                None
            }
        } else {
            None
        };

        (timed_out, retransmits, ping_needed, disconnect_later)
    }

    // ── Main run loop ─────────────────────────────────────────────────────────

    pub async fn run(mut self) {
        // Client side: send CMD_CONNECT immediately.
        if self.connect_reply.is_some() {
            let now_ms = self.now_ms();
            let user_data = self.connect_user_data;
            let cmd = self.build_connect_cmd(user_data, now_ms);
            self.emit(&[cmd], now_ms).await;
        }

        let mut ticker = time::interval(Duration::from_millis(20));
        ticker.set_missed_tick_behavior(time::MissedTickBehavior::Skip);
        let mut cmd_rx_closed = false;

        loop {
            tokio::select! {
                biased;
                msg = self.inbox_rx.recv() => match msg {
                    None | Some(InboundMsg::Evicted) => {
                        let _ = self.to_user_tx.try_send(Err(None));
                        self.removed = true;
                    }
                    Some(InboundMsg::Datagram(data)) => {
                        let now_ms = self.now_ms();
                        self.on_datagram(&data, now_ms).await;
                    }
                    Some(InboundMsg::BandwidthUpdate { incoming_bw, per_peer_ob }) => {
                        let now_ms = self.now_ms();
                        self.apply_bw_update(incoming_bw, per_peer_ob, now_ms).await;
                    }
                },
                cmd = self.cmd_rx.recv(), if !cmd_rx_closed => match cmd {
                    None => {
                        cmd_rx_closed = true;
                        if !matches!(self.state, PeerState::Disconnecting | PeerState::DisconnectLater(_)) {
                            let now_ms = self.now_ms();
                            self.begin_disconnect(0, now_ms).await;
                        }
                    }
                    Some(PeerCmd::Send(pkt)) => {
                        let now_ms = self.now_ms();
                        self.send_packet(pkt, now_ms).await;
                    }
                    Some(PeerCmd::Disconnect { data }) => {
                        cmd_rx_closed = true;
                        let now_ms = self.now_ms();
                        self.begin_disconnect(data, now_ms).await;
                    }
                },
                _ = ticker.tick() => {
                    let now_ms = self.now_ms();
                    self.service(now_ms).await;
                }
            }

            if self.removed {
                break;
            }
        }

        let _ = self.notify_tx.try_send(PeerNotify::Removed { key: self.key });
    }
}
