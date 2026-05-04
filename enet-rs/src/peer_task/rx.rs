/// PeerTask receive path: parse incoming datagrams and dispatch protocol commands.
use tracing::{debug, warn};

use crate::channel::FragmentBuffer;
use crate::protocol::{
    self, AcknowledgeCmd, CommandBody, CommandHeader, ConnectCmd, VerifyConnectCmd,
    CHANNEL_ID_CONNECTION, CMD_ACKNOWLEDGE, CMD_BANDWIDTH_LIMIT, CMD_CONNECT, CMD_DISCONNECT,
    CMD_PING, CMD_SEND_FRAGMENT, CMD_SEND_RELIABLE, CMD_SEND_UNRELIABLE,
    CMD_SEND_UNRELIABLE_FRAGMENT, CMD_SEND_UNSEQUENCED, CMD_THROTTLE_CONFIGURE, CMD_VERIFY_CONNECT,
    MAX_CHANNELS, MTU_MAX, MTU_MIN, PACKET_THROTTLE_SCALE, WINDOW_SIZE_MAX, WINDOW_SIZE_MIN,
};
use crate::{Packet, SendMode};

use super::{PeerState, PeerTask};

const MAX_FRAGMENT_BUFFERS_PER_CHANNEL: usize = 64;
const MAX_FRAGMENT_BYTES_PER_PEER: usize = 16 * 1024 * 1024;

impl PeerTask {
    // ── Datagram entry point ──────────────────────────────────────────────────

    pub(super) async fn on_datagram(&mut self, raw: &[u8], now_ms: u32) {
        let mut slice = raw;
        let Some(hdr) = protocol::ProtocolHeader::parse(&mut slice) else {
            return;
        };
        if hdr.is_compressed() {
            return;
        }

        // Session validation for established peers.
        let first_cmd_type = slice.first().map(|b| b & 0x0F);
        let is_connect = first_cmd_type == Some(CMD_CONNECT);
        if !is_connect
            && self.incoming_session_id != 0xFF
            && hdr.session_id() != self.incoming_session_id
        {
            return;
        }

        if self.use_checksum {
            if slice.len() < 4 {
                return;
            }
            let stored = u32::from_be_bytes([slice[0], slice[1], slice[2], slice[3]]);
            let hdr_len = raw.len() - slice.len();
            let mut scratch = Vec::with_capacity(raw.len());
            scratch.extend_from_slice(&raw[..hdr_len]);
            scratch.extend_from_slice(&self.connect_id.to_be_bytes());
            scratch.extend_from_slice(&slice[4..]);
            if protocol::crc32(&scratch) != stored {
                return;
            }
            slice = &slice[4..];
        }

        let sent_time = hdr.sent_time;
        let cmds = protocol::parse_commands(slice);
        let mut pending_acks = Vec::new();

        for cmd in cmds {
            let cmd_type = cmd.header.command_type();
            let hdr_has_ack = cmd.header.has_ack_flag();
            let hdr_channel = cmd.header.channel_id;
            let hdr_seq = cmd.header.reliable_seq;

            if hdr_has_ack && !matches!(cmd_type, CMD_CONNECT | CMD_DISCONNECT | CMD_ACKNOWLEDGE) {
                if let Some(st) = sent_time {
                    pending_acks.push(Self::ack(hdr_channel, hdr_seq, st));
                }
            }

            self.dispatch(
                cmd_type,
                cmd.header,
                cmd.body,
                sent_time,
                now_ms,
                &mut pending_acks,
            )
            .await;
        }

        if !pending_acks.is_empty() {
            self.flush_batch(pending_acks, now_ms).await;
        }
    }

    async fn dispatch(
        &mut self,
        cmd_type: u8,
        hdr: CommandHeader,
        body: CommandBody,
        sent_time: Option<u16>,
        now_ms: u32,
        _pending_acks: &mut Vec<bytes::Bytes>,
    ) {
        let hdr_has_ack = hdr.has_ack_flag();
        let hdr_channel = hdr.channel_id;
        let hdr_seq = hdr.reliable_seq;

        match cmd_type {
            CMD_CONNECT => {
                if let CommandBody::Connect(c) = body {
                    self.on_connect(c, hdr, sent_time, now_ms).await;
                }
            }
            CMD_VERIFY_CONNECT => {
                if let CommandBody::VerifyConnect(vc) = body {
                    self.on_verify_connect(vc, hdr, now_ms).await;
                }
            }
            CMD_ACKNOWLEDGE => {
                if let CommandBody::Acknowledge(ack) = body {
                    self.on_ack(hdr_channel, ack, now_ms).await;
                }
            }
            CMD_DISCONNECT => {
                if let CommandBody::Disconnect(d) = body {
                    if self.state != PeerState::AcknowledgingConnect {
                        if hdr_has_ack {
                            if let Some(st) = sent_time {
                                let a = Self::ack(hdr_channel, hdr_seq, st);
                                self.emit(&[a], now_ms).await;
                            }
                        }
                        let _ = self.to_user_tx.try_send(Err(Some(d.data)));
                        self.removed = true;
                    }
                }
            }
            CMD_PING => {
                self.last_receive_time = now_ms;
            }
            CMD_SEND_RELIABLE => {
                if let CommandBody::SendReliable(sr) = body {
                    self.last_receive_time = now_ms;
                    if (hdr_channel as usize) < self.channels.len() {
                        let ready = self.channels[hdr_channel as usize]
                            .receive_reliable(hdr_seq, sr.data, 1);
                        for data in ready {
                            self.deliver(Packet::reliable(data, hdr_channel));
                        }
                    }
                }
            }
            CMD_SEND_UNRELIABLE => {
                if let CommandBody::SendUnreliable(su) = body {
                    self.last_receive_time = now_ms;
                    let ch = hdr_channel as usize;
                    if ch < self.channels.len() {
                        if !protocol::seq_lt(hdr_seq, self.channels[ch].incoming_reliable_seq) {
                            if let Some(data) =
                                self.channels[ch].receive_unreliable(su.unreliable_seq, su.data)
                            {
                                self.deliver(Packet::unreliable(data, hdr_channel));
                            }
                        }
                    }
                }
            }
            CMD_SEND_FRAGMENT => {
                if let CommandBody::SendFragment(sf) = body {
                    self.on_fragment(hdr_channel, sf, true, hdr_seq, now_ms);
                }
            }
            CMD_SEND_UNRELIABLE_FRAGMENT => {
                if let CommandBody::SendUnreliableFragment(sf) = body {
                    self.on_fragment(hdr_channel, sf, false, hdr_seq, now_ms);
                }
            }
            CMD_SEND_UNSEQUENCED => {
                if let CommandBody::SendUnsequenced(su) = body {
                    self.last_receive_time = now_ms;
                    if self.unsequenced_window.check_and_set(su.unsequenced_group) {
                        self.deliver(Packet::unsequenced(su.data));
                    }
                }
            }
            CMD_BANDWIDTH_LIMIT => {
                if let CommandBody::BandwidthLimit(bl) = body {
                    if bl.incoming_bandwidth != 0 || bl.outgoing_bandwidth != 0 {
                        let throttle = if bl.incoming_bandwidth == 0 {
                            WINDOW_SIZE_MAX
                        } else {
                            ((bl.incoming_bandwidth / WINDOW_SIZE_MAX) * WINDOW_SIZE_MIN)
                                .max(WINDOW_SIZE_MIN)
                        };
                        self.window_size = throttle.clamp(WINDOW_SIZE_MIN, WINDOW_SIZE_MAX);
                    }
                }
            }
            CMD_THROTTLE_CONFIGURE => {
                if let CommandBody::ThrottleConfigure(tc) = body {
                    if tc.throttle_interval > 0 {
                        self.throttle_interval = tc.throttle_interval;
                    }
                    self.throttle_acceleration =
                        tc.throttle_acceleration.min(PACKET_THROTTLE_SCALE);
                    self.throttle_deceleration =
                        tc.throttle_deceleration.min(PACKET_THROTTLE_SCALE);
                }
            }
            _ => {}
        }
    }

    // ── CONNECT (server side) ─────────────────────────────────────────────────

    async fn on_connect(
        &mut self,
        connect: ConnectCmd,
        hdr: CommandHeader,
        sent_time: Option<u16>,
        now_ms: u32,
    ) {
        if self.state != PeerState::AcknowledgingConnect {
            return;
        }

        let in_sess = if connect.incoming_session_id == 0xFF {
            1u8
        } else {
            (connect.incoming_session_id.wrapping_add(1)) & 0x3
        };
        let out_sess = if connect.outgoing_session_id == 0xFF {
            2u8
        } else {
            (connect.outgoing_session_id.wrapping_add(1)) & 0x3
        };

        let mtu = connect.mtu.clamp(MTU_MIN, MTU_MAX).min(self.host_mtu);

        let server_window = if self.incoming_bw == 0 {
            WINDOW_SIZE_MAX
        } else {
            ((self.incoming_bw / WINDOW_SIZE_MAX) * WINDOW_SIZE_MIN).max(WINDOW_SIZE_MIN)
        };
        let peer_window = if self.outgoing_bw == 0 && connect.incoming_bandwidth == 0 {
            WINDOW_SIZE_MAX
        } else if self.outgoing_bw == 0 || connect.incoming_bandwidth == 0 {
            ((self.outgoing_bw.max(connect.incoming_bandwidth) / WINDOW_SIZE_MAX) * WINDOW_SIZE_MIN)
                .max(WINDOW_SIZE_MIN)
        } else {
            ((self.outgoing_bw.min(connect.incoming_bandwidth) / WINDOW_SIZE_MAX) * WINDOW_SIZE_MIN)
                .max(WINDOW_SIZE_MIN)
        };
        let window_size = server_window
            .min(peer_window)
            .min(connect.window_size)
            .clamp(WINDOW_SIZE_MIN, WINDOW_SIZE_MAX);
        let ch = connect.channel_count.clamp(1, MAX_CHANNELS) as u8;

        // Apply connect state.
        self.outgoing_peer_id = connect.outgoing_peer_id;
        self.connect_id = connect.connect_id;
        self.incoming_session_id = in_sess;
        self.outgoing_session_id = out_sess;
        self.mtu = mtu;
        self.window_size = window_size;
        self.throttle_interval = connect.throttle_interval;
        self.throttle_acceleration = connect.throttle_acceleration;
        self.throttle_deceleration = connect.throttle_deceleration;
        self.last_send_time = now_ms;
        self.last_receive_time = now_ms;
        self.channels = (0..ch).map(|_| crate::channel::Channel::new()).collect();

        let ack_opt = sent_time.map(|st| Self::ack(CHANNEL_ID_CONNECTION, hdr.reliable_seq, st));

        let peer_id_wire = self.key.0 as u16;
        let host_ib = self.incoming_bw;
        let host_ob = self.outgoing_bw;
        let throttle_interval = self.throttle_interval;
        let throttle_acceleration = self.throttle_acceleration;
        let throttle_deceleration = self.throttle_deceleration;
        let connect_id = self.connect_id;

        let verify = self.make_reliable(
            CHANNEL_ID_CONNECTION,
            crate::protocol::CMD_VERIFY_CONNECT,
            0,
            now_ms,
            move |buf| {
                buf.extend_from_slice(&peer_id_wire.to_be_bytes());
                buf.extend_from_slice(&[out_sess, in_sess]);
                buf.extend_from_slice(&mtu.to_be_bytes());
                buf.extend_from_slice(&window_size.to_be_bytes());
                buf.extend_from_slice(&(ch as u32).to_be_bytes());
                buf.extend_from_slice(&host_ib.to_be_bytes());
                buf.extend_from_slice(&host_ob.to_be_bytes());
                buf.extend_from_slice(&throttle_interval.to_be_bytes());
                buf.extend_from_slice(&throttle_acceleration.to_be_bytes());
                buf.extend_from_slice(&throttle_deceleration.to_be_bytes());
                buf.extend_from_slice(&connect_id.to_be_bytes());
            },
        );
        self.connect_seq = self.outgoing_reliable_seq;

        let mut cmds = Vec::new();
        if let Some(a) = ack_opt {
            cmds.push(a);
        }
        cmds.push(verify);
        self.emit(&cmds, now_ms).await;
    }

    // ── VERIFY_CONNECT (client side) ──────────────────────────────────────────

    async fn on_verify_connect(&mut self, vc: VerifyConnectCmd, _hdr: CommandHeader, now_ms: u32) {
        if self.state != PeerState::Connecting || vc.connect_id != self.connect_id {
            return;
        }
        self.outgoing_peer_id = vc.outgoing_peer_id;
        self.incoming_session_id = vc.incoming_session_id;
        self.outgoing_session_id = vc.outgoing_session_id;
        self.mtu = vc.mtu.clamp(MTU_MIN, MTU_MAX).min(self.host_mtu);
        self.window_size = vc.window_size.clamp(WINDOW_SIZE_MIN, WINDOW_SIZE_MAX);
        self.throttle_interval = vc.throttle_interval;
        self.throttle_acceleration = vc.throttle_acceleration;
        self.throttle_deceleration = vc.throttle_deceleration;
        self.state = PeerState::Connected;
        self.last_receive_time = now_ms;

        // Remove CMD_CONNECT from outstanding (VERIFY_CONNECT implicitly ACKs it).
        let connect_seq = self.connect_seq;
        self.outgoing_reliable
            .remove(&(CHANNEL_ID_CONNECTION, connect_seq));

        let peer = self.take_peer();
        if let Some(reply) = self.connect_reply.take() {
            let _ = reply.send(Ok(peer));
        }
    }

    // ── ACK ───────────────────────────────────────────────────────────────────

    async fn on_ack(&mut self, channel_id: u8, ack: AcknowledgeCmd, now_ms: u32) {
        self.last_receive_time = now_ms;

        // Reconstruct full 32-bit sent time from 16-bit wire value.
        let t16 = ack.received_sent_time;
        let mut t32 = t16 as u32 | (now_ms & 0xFFFF_0000);
        if (t32 & 0x8000) > (now_ms & 0x8000) {
            t32 = t32.wrapping_sub(0x10000);
        }
        if now_ms >= t32 {
            self.update_rtt((now_ms - t32).max(1));
        }

        // Adaptive throttle.
        if self.packet_throttle_epoch == 0
            || now_ms.wrapping_sub(self.packet_throttle_epoch) >= self.throttle_interval
        {
            let prev_rtt = self.rtt_at_throttle_epoch;
            self.packet_throttle_epoch = now_ms;
            self.rtt_at_throttle_epoch = self.rtt;

            if prev_rtt > 0 && self.rtt_initialized {
                if self.rtt < prev_rtt {
                    self.packet_throttle = (self.packet_throttle + self.throttle_acceleration)
                        .min(PACKET_THROTTLE_SCALE);
                } else if self.rtt > prev_rtt.saturating_add(2 * self.rtt_var) {
                    self.packet_throttle = self
                        .packet_throttle
                        .saturating_sub(self.throttle_deceleration);
                }
            }
        }

        let seq = ack.received_reliable_seq;
        self.outgoing_reliable.remove(&(channel_id, seq));

        let completing_handshake = self.state == PeerState::AcknowledgingConnect
            && channel_id == CHANNEL_ID_CONNECTION
            && seq == self.connect_seq;
        let disconnect_done =
            self.state == PeerState::Disconnecting && self.outgoing_reliable.is_empty();

        if completing_handshake {
            self.state = PeerState::Connected;
            let peer = self.take_peer();
            if let Some(tx) = &self.accept_tx {
                if tx.send(peer).await.is_err() {
                    // Host dropped — clean shutdown.
                    self.removed = true;
                }
            }
        } else if disconnect_done {
            self.removed = true;
        }
    }

    // ── Fragment reassembly ───────────────────────────────────────────────────

    fn on_fragment(
        &mut self,
        ch: u8,
        sf: crate::protocol::SendFragmentCmd,
        reliable: bool,
        hdr_seq: u16,
        now_ms: u32,
    ) {
        if sf.fragment_count == 0
            || sf.fragment_count > crate::protocol::MAX_FRAGMENT_COUNT
            || sf.fragment_number >= sf.fragment_count
            || sf.total_length as usize > crate::protocol::MAX_PACKET_SIZE
        {
            return;
        }
        let frag_end = (sf.fragment_offset as usize).saturating_add(sf.data.len());
        if frag_end > sf.total_length as usize {
            return;
        }

        self.last_receive_time = now_ms;
        if ch as usize >= self.channels.len() {
            return;
        }

        if !reliable {
            if protocol::seq_lt(hdr_seq, self.channels[ch as usize].incoming_reliable_seq) {
                return;
            }
        }

        let is_new = !self.channels[ch as usize]
            .fragment_buffers
            .contains_key(&sf.start_seq);
        if is_new {
            if self.channels[ch as usize].fragment_buffers.len() >= MAX_FRAGMENT_BUFFERS_PER_CHANNEL
            {
                debug!("fragment buffer limit reached for peer {}", self.addr);
                return;
            }
            let new_total = self
                .fragment_bytes_in_flight
                .saturating_add(sf.total_length as usize);
            if new_total > MAX_FRAGMENT_BYTES_PER_PEER {
                debug!("fragment byte budget exceeded for peer {}", self.addr);
                return;
            }
            self.fragment_bytes_in_flight = new_total;
        }

        let done = {
            let chan = &mut self.channels[ch as usize];
            let buf = chan
                .fragment_buffers
                .entry(sf.start_seq)
                .or_insert_with(|| {
                    FragmentBuffer::new(sf.total_length as usize, sf.fragment_count)
                });
            buf.receive(sf.fragment_number, sf.fragment_offset as usize, &sf.data)
        };

        if done {
            let frag_count = sf.fragment_count;
            let assembled = self.channels[ch as usize]
                .fragment_buffers
                .remove(&sf.start_seq)
                .expect("fragment buffer must exist when done")
                .finish();
            // Use the buffer's actual allocated size for the budget decrement; a
            // malicious peer could have sent fragments with inconsistent total_length
            // values for the same start_seq.
            self.fragment_bytes_in_flight = self
                .fragment_bytes_in_flight
                .saturating_sub(assembled.len());

            let packets = if reliable {
                self.channels[ch as usize]
                    .receive_reliable(sf.start_seq, assembled, frag_count)
                    .into_iter()
                    .map(|d| Packet {
                        data: d,
                        channel: ch,
                        mode: SendMode::Reliable,
                    })
                    .collect::<Vec<_>>()
            } else {
                match self.channels[ch as usize].receive_unreliable(sf.start_seq, assembled) {
                    Some(d) => vec![Packet {
                        data: d,
                        channel: ch,
                        mode: SendMode::Unreliable,
                    }],
                    None => vec![],
                }
            };
            for pkt in packets {
                self.deliver(pkt);
            }
        }
    }

    // ── Deliver to user ───────────────────────────────────────────────────────

    pub(super) fn deliver(&self, pkt: Packet) {
        if let Err(e) = self.to_user_tx.try_send(Ok(pkt)) {
            warn!(
                "dropping packet for {}: user receive channel full ({})",
                self.addr, e
            );
        }
    }
}
