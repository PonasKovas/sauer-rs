/// PeerTask send path: outgoing datagrams, reliability tracking, disconnect.
use bytes::{BufMut, Bytes, BytesMut};
use tracing::warn;

use crate::protocol::{
    self, CHANNEL_ID_CONNECTION, CMD_BANDWIDTH_LIMIT, CMD_CONNECT, CMD_DISCONNECT,
    CMD_SEND_FRAGMENT, CMD_SEND_RELIABLE, CMD_SEND_UNRELIABLE, CMD_SEND_UNRELIABLE_FRAGMENT,
    CMD_SEND_UNSEQUENCED, COMMAND_FLAG_ACKNOWLEDGE, COMMAND_FLAG_UNSEQUENCED, FREE_RELIABLE_WINDOWS,
    HEADER_FLAG_SENT_TIME, HEADER_SESSION_SHIFT, PACKET_THROTTLE_SCALE, RELIABLE_WINDOW_SIZE,
    THROTTLE_ACCELERATION, THROTTLE_DECELERATION, THROTTLE_INTERVAL_MS, WINDOW_SIZE_MAX,
    WINDOW_SIZE_MIN,
};
use crate::{Packet, SendMode};

use super::{OutstandingReliable, PeerState, PeerTask};

/// Encode a command into bytes (without the datagram header).
fn encode_command(
    cmd_type: u8,
    flags: u8,
    channel_id: u8,
    reliable_seq: u16,
    body_writer: impl FnOnce(&mut BytesMut),
) -> Bytes {
    let mut buf = BytesMut::with_capacity(64);
    buf.put_u8(cmd_type | flags);
    buf.put_u8(channel_id);
    buf.put_u16(reliable_seq);
    body_writer(&mut buf);
    buf.freeze()
}

/// Maximum reliable packets queued while the send window is full.
const MAX_PENDING_SEND: usize = 1024;

impl PeerTask {
    // ── Datagram construction ─────────────────────────────────────────────────

    pub(super) fn make_datagram(&self, cmds: &[Bytes], now_ms: u32) -> Bytes {
        let mut buf = BytesMut::new();
        let peer_id_raw = HEADER_FLAG_SENT_TIME
            | ((self.outgoing_session_id as u16 & 0x3) << HEADER_SESSION_SHIFT)
            | (self.outgoing_peer_id & 0x0FFF);
        buf.put_u16(peer_id_raw);
        buf.put_u16(now_ms as u16);

        if self.use_checksum {
            let ck_offset = buf.len();
            buf.put_u32(self.connect_id);
            for c in cmds {
                buf.extend_from_slice(c);
            }
            let ck = protocol::crc32(&buf);
            buf[ck_offset..ck_offset + 4].copy_from_slice(&ck.to_be_bytes());
        } else {
            for c in cmds {
                buf.extend_from_slice(c);
            }
        }
        buf.freeze()
    }

    pub(super) async fn emit(&mut self, cmds: &[Bytes], now_ms: u32) {
        if cmds.is_empty() {
            return;
        }
        let datagram = self.make_datagram(cmds, now_ms);
        if let Err(e) = self.socket.send_to(&datagram, self.addr).await {
            warn!("send_to {}: {e}", self.addr);
        }
        self.last_send_time = now_ms;
    }

    pub(super) async fn flush_batch(&mut self, cmds: Vec<Bytes>, now_ms: u32) {
        let mtu = self.mtu as usize;
        let hdr = 4 + if self.use_checksum { 4 } else { 0 };
        let mut batch = Vec::new();
        let mut sz = hdr;
        for cmd in cmds {
            if sz + cmd.len() > mtu && !batch.is_empty() {
                self.emit(&batch, now_ms).await;
                batch.clear();
                sz = hdr;
            }
            sz += cmd.len();
            batch.push(cmd);
        }
        if !batch.is_empty() {
            self.emit(&batch, now_ms).await;
        }
    }

    // ── Reliable command tracking ─────────────────────────────────────────────

    pub(super) fn make_reliable(
        &mut self,
        channel_id: u8,
        cmd_type: u8,
        extra_flags: u8,
        now_ms: u32,
        body: impl FnOnce(&mut BytesMut),
    ) -> Bytes {
        let seq = if channel_id == CHANNEL_ID_CONNECTION {
            self.next_connect_seq()
        } else {
            let idx = channel_id as usize;
            if idx >= self.channels.len() {
                warn!("make_reliable: channel_id {channel_id} out of range");
                return Bytes::new();
            }
            self.channels[idx].next_reliable_seq()
        };

        let cmd = encode_command(cmd_type, COMMAND_FLAG_ACKNOWLEDGE | extra_flags, channel_id, seq, body);
        let timeout = self.initial_rtt_timeout();

        self.outgoing_reliable.insert(
            (channel_id, seq),
            OutstandingReliable {
                reliable_seq: seq,
                channel_id,
                original_sent_time: now_ms,
                sent_time: now_ms,
                rtt_timeout: timeout,
                send_attempts: 1,
                packet_bytes: cmd.clone(),
            },
        );
        cmd
    }

    pub(super) fn ack(channel_id: u8, seq: u16, sent_time: u16) -> Bytes {
        encode_command(crate::protocol::CMD_ACKNOWLEDGE, 0, channel_id, 0, |buf| {
            buf.put_u16(seq);
            buf.put_u16(sent_time);
        })
    }

    // ── Client-side initial CONNECT ───────────────────────────────────────────

    pub(super) fn build_connect_cmd(&mut self, user_data: u32, now_ms: u32) -> Bytes {
        let host_mtu = self.host_mtu;
        let incoming_bw = self.incoming_bw;
        let outgoing_bw = self.outgoing_bw;
        let channel_count = self.channels.len() as u32;
        let connect_id = self.connect_id;
        let peer_id_wire = self.key.0 as u16;

        let cmd = self.make_reliable(CHANNEL_ID_CONNECTION, CMD_CONNECT, 0, now_ms, move |buf| {
            buf.put_u16(peer_id_wire);
            buf.put_u8(0xFF); // incoming_session_id: fresh
            buf.put_u8(0xFF); // outgoing_session_id: fresh
            buf.put_u32(host_mtu);
            buf.put_u32(WINDOW_SIZE_MAX);
            buf.put_u32(channel_count);
            buf.put_u32(incoming_bw);
            buf.put_u32(outgoing_bw);
            buf.put_u32(THROTTLE_INTERVAL_MS);
            buf.put_u32(THROTTLE_ACCELERATION);
            buf.put_u32(THROTTLE_DECELERATION);
            buf.put_u32(connect_id);
            buf.put_u32(user_data);
        });
        self.connect_seq = self.outgoing_reliable_seq;
        cmd
    }

    // ── Outgoing packet send ──────────────────────────────────────────────────

    pub(super) async fn send_packet(&mut self, pkt: Packet, now_ms: u32) {
        if self.state != PeerState::Connected {
            return;
        }
        let mtu = self.mtu;
        let ck_overhead = if self.use_checksum { 4 } else { 0 };
        let hdr_overhead = 4 + ck_overhead;
        let ch = pkt.channel;

        match pkt.mode {
            SendMode::Reliable => self.send_reliable(pkt, ch, mtu, hdr_overhead, now_ms).await,
            SendMode::Unreliable => self.send_unreliable(pkt, ch, mtu, hdr_overhead, now_ms).await,
            SendMode::Unsequenced => self.send_unsequenced(pkt, ch, now_ms).await,
        }
    }

    async fn send_reliable(
        &mut self,
        pkt: Packet,
        ch: u8,
        mtu: u32,
        hdr_overhead: usize,
        now_ms: u32,
    ) {
        if ch as usize >= self.channels.len() {
            return;
        }
        // Check the reliable send window.
        let next_seq = self.channels[ch as usize]
            .outgoing_reliable_seq
            .wrapping_add(1);
        let oldest = self
            .outgoing_reliable
            .values()
            .filter(|r| r.channel_id == ch)
            .map(|r| r.reliable_seq)
            .reduce(|a, b| if protocol::seq_lt(a, b) { a } else { b });

        let effective_window = ((self.packet_throttle as u64 * self.window_size as u64)
            / PACKET_THROTTLE_SCALE as u64)
            .min(FREE_RELIABLE_WINDOWS as u64 * RELIABLE_WINDOW_SIZE as u64)
            as u32;

        let window_open = match oldest {
            None => true,
            Some(oldest_seq) => (next_seq.wrapping_sub(oldest_seq) as u32) < effective_window,
        };
        if !window_open {
            if self.pending_send.len() < MAX_PENDING_SEND {
                self.pending_send.push_back(pkt);
            }
            return;
        }

        let max_payload = mtu as usize - hdr_overhead - 4 - 2; // cmd_hdr=4, dataLen=2

        if pkt.data.len() <= max_payload {
            let data = pkt.data;
            let data_len = data.len() as u16;
            let cmd = self.make_reliable(ch, CMD_SEND_RELIABLE, 0, now_ms, move |buf| {
                buf.put_u16(data_len);
                buf.put_slice(&data);
            });
            self.emit(&[cmd], now_ms).await;
        } else {
            self.send_reliable_fragmented(pkt.data, ch, mtu, hdr_overhead, now_ms)
                .await;
        }
    }

    async fn send_reliable_fragmented(
        &mut self,
        data: Bytes,
        ch: u8,
        mtu: u32,
        hdr_overhead: usize,
        now_ms: u32,
    ) {
        let frag_payload = mtu as usize - hdr_overhead - 4 - 20; // cmd_hdr=4, frag_fields=20
        let total = data.len();
        let frag_count = total.div_ceil(frag_payload);
        let start_seq = self.channels[ch as usize].next_reliable_seq();
        let mut cmds = Vec::with_capacity(frag_count);

        for i in 0..frag_count {
            let offset = i * frag_payload;
            let frag = data.slice(offset..(offset + frag_payload).min(total));
            let frag_len = frag.len() as u16;
            let seq = if i == 0 {
                start_seq
            } else {
                self.channels[ch as usize].next_reliable_seq()
            };
            let timeout = self.initial_rtt_timeout();

            let cmd = encode_command(CMD_SEND_FRAGMENT, COMMAND_FLAG_ACKNOWLEDGE, ch, seq, move |buf| {
                buf.put_u16(start_seq);
                buf.put_u16(frag_len);
                buf.put_u32(frag_count as u32);
                buf.put_u32(i as u32);
                buf.put_u32(total as u32);
                buf.put_u32(offset as u32);
                buf.put_slice(&frag);
            });
            self.outgoing_reliable.insert(
                (ch, seq),
                OutstandingReliable {
                    reliable_seq: seq,
                    channel_id: ch,
                    original_sent_time: now_ms,
                    sent_time: now_ms,
                    rtt_timeout: timeout,
                    send_attempts: 1,
                    packet_bytes: cmd.clone(),
                },
            );
            cmds.push(cmd);
        }
        self.flush_batch(cmds, now_ms).await;
    }

    async fn send_unreliable(
        &mut self,
        pkt: Packet,
        ch: u8,
        mtu: u32,
        hdr_overhead: usize,
        now_ms: u32,
    ) {
        if ch as usize >= self.channels.len() {
            return;
        }
        let max_payload = mtu as usize - hdr_overhead - 4 - 4; // cmd_hdr=4, fields=4

        if pkt.data.len() <= max_payload {
            let unreliable_seq = self.channels[ch as usize].next_unreliable_seq();
            let reliable_seq = self.channels[ch as usize].outgoing_reliable_seq;
            let data = pkt.data;
            let data_len = data.len() as u16;
            let cmd = encode_command(CMD_SEND_UNRELIABLE, 0, ch, reliable_seq, move |buf| {
                buf.put_u16(unreliable_seq);
                buf.put_u16(data_len);
                buf.put_slice(&data);
            });
            self.emit(&[cmd], now_ms).await;
        } else {
            self.send_unreliable_fragmented(pkt.data, ch, mtu, hdr_overhead, now_ms)
                .await;
        }
    }

    async fn send_unreliable_fragmented(
        &mut self,
        data: Bytes,
        ch: u8,
        mtu: u32,
        hdr_overhead: usize,
        now_ms: u32,
    ) {
        let frag_payload = mtu as usize - hdr_overhead - 4 - 20;
        let total = data.len();
        let frag_count = total.div_ceil(frag_payload);
        let start_seq = self.channels[ch as usize].next_unreliable_seq();
        let reliable_seq = self.channels[ch as usize].outgoing_reliable_seq;
        let mut cmds = Vec::with_capacity(frag_count);

        for i in 0..frag_count {
            let offset = i * frag_payload;
            let frag = data.slice(offset..(offset + frag_payload).min(total));
            let frag_len = frag.len() as u16;
            let cmd = encode_command(CMD_SEND_UNRELIABLE_FRAGMENT, 0, ch, reliable_seq, move |buf| {
                buf.put_u16(start_seq);
                buf.put_u16(frag_len);
                buf.put_u32(frag_count as u32);
                buf.put_u32(i as u32);
                buf.put_u32(total as u32);
                buf.put_u32(offset as u32);
                buf.put_slice(&frag);
            });
            cmds.push(cmd);
        }
        self.flush_batch(cmds, now_ms).await;
    }

    async fn send_unsequenced(&mut self, pkt: Packet, ch: u8, now_ms: u32) {
        let Some(chan) = self.channels.get_mut(ch as usize) else {
            return;
        };
        // ENet unsequenced sends are never fragmented; reject oversized data before
        // the length field silently truncates and corrupts the receiver's parser.
        let max_payload = self.mtu as usize
            - (4 + if self.use_checksum { 4 } else { 0 })
            - 4  // cmd_hdr
            - 4; // unsequenced_group + data_len fields
        if pkt.data.len() > max_payload {
            warn!(
                "dropping oversized unsequenced packet ({} > {max_payload})",
                pkt.data.len()
            );
            return;
        }
        let group = chan.next_unsequenced_group();
        let data = pkt.data;
        let data_len = data.len() as u16;
        let cmd = encode_command(CMD_SEND_UNSEQUENCED, COMMAND_FLAG_UNSEQUENCED, ch, 0, move |buf| {
            buf.put_u16(group);
            buf.put_u16(data_len);
            buf.put_slice(&data);
        });
        self.emit(&[cmd], now_ms).await;
    }

    // ── Disconnect ────────────────────────────────────────────────────────────

    pub(super) async fn begin_disconnect(&mut self, data: u32, now_ms: u32) {
        if matches!(self.state, PeerState::Disconnecting | PeerState::DisconnectLater(_)) {
            return;
        }
        if self.state == PeerState::AcknowledgingConnect {
            return;
        }
        if !self.outgoing_reliable.is_empty() {
            self.state = PeerState::DisconnectLater(data);
            return;
        }
        self.state = PeerState::Disconnecting;
        let cmd = self.make_reliable(CHANNEL_ID_CONNECTION, CMD_DISCONNECT, 0, now_ms, |buf| {
            buf.put_u32(data);
        });
        self.emit(&[cmd], now_ms).await;
    }

    // ── Bandwidth update from host ────────────────────────────────────────────

    pub(super) async fn apply_bw_update(&mut self, incoming_bw: u32, per_peer_ob: u32, now_ms: u32) {
        if self.state != PeerState::Connected {
            return;
        }
        if per_peer_ob > 0 {
            self.window_size = per_peer_ob.clamp(WINDOW_SIZE_MIN, WINDOW_SIZE_MAX);
        }
        if incoming_bw == 0 && per_peer_ob == 0 {
            return;
        }
        let ib = incoming_bw;
        let ob = per_peer_ob;
        let cmd = self.make_reliable(CHANNEL_ID_CONNECTION, CMD_BANDWIDTH_LIMIT, 0, now_ms, move |buf| {
            buf.put_u32(ib);
            buf.put_u32(ob);
        });
        self.emit(&[cmd], now_ms).await;
    }
}
