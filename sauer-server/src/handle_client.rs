use crate::{
    Server,
    plugin::{Context, Plugin},
};
use anyhow::{Result, bail};
use bytes::Bytes;
use lenet_tokio::{ConnEvent, ConnectionReceiver};
use sauer_proto::{
    Chan0Client, Chan1Client, Decode, Reader, Session,
    route::{C2S, Chan0, Chan1},
};
use std::{collections::VecDeque, sync::Arc};

/// Calls the plugin hook for a client's packet on one channel.
macro_rules! dispatch {
    ($enum:ident, $pkt:ident, $plugin:ident, $ctx:ident, $( [$msg_id:ident] $method:ident ($packet:path) $variant:ident, )*) => {
        match $pkt {
            $( $enum::$variant(p) => $plugin.$method(&mut $ctx, &p).await, )*
        }
    };
}

macro_rules! dispatch_chan0 {
    (($pkt:ident, $plugin:ident, $ctx:ident) chan0 { $($c0:tt)* } chan1 { $($c1:tt)* }) => {
        dispatch!(Chan0Client, $pkt, $plugin, $ctx, $($c0)*)
    };
}

macro_rules! dispatch_chan1 {
    (($pkt:ident, $plugin:ident, $ctx:ident) chan0 { $($c0:tt)* } chan1 { $($c1:tt)* }) => {
        dispatch!(Chan1Client, $pkt, $plugin, $ctx, $($c1)*)
    };
}

pub(crate) async fn handle_client<P: Plugin>(
    server: &Arc<Server>,
    plugin: &Arc<P>,
    peer: ConnectionReceiver,
    id: u64,
) -> Result<()> {
    let mut rx = SauerReceiver::new(peer);
    let mut ctx = Context {
        server: Arc::clone(server),
        id,
    };

    loop {
        // What a plugin returns doesn't matter here: the packet is done either way.
        let _ = match rx.recv().await? {
            Packet::Chan0(pkt) => {
                crate::packet_log::trace_in(id, pkt.msg_type(), &pkt);
                crate::for_each_packet_hook!(dispatch_chan0, pkt, plugin, ctx)
            }
            Packet::Chan1(pkt) => {
                crate::packet_log::trace_in(id, pkt.msg_type(), &pkt);
                crate::for_each_packet_hook!(dispatch_chan1, pkt, plugin, ctx)
            }
            Packet::Chan2(data) => plugin.map(&mut ctx, &data).await,
        };
    }
}

// ── Packet buffering / receive loop ──────────────────────────────────────────

fn log_bad_packet(channel: u8, data: &[u8], err: &impl std::fmt::Display) {
    const SHOWN: usize = 256;
    let hex: String = data
        .iter()
        .take(SHOWN)
        .map(|b| format!("{b:02x}"))
        .collect();
    let more = if data.len() > SHOWN { "..." } else { "" };
    tracing::warn!(
        "dropped rest of a packet on channel {channel} ({} bytes): {err}; data: {hex}{more}",
        data.len()
    );
}

/// Decodes every message in a packet into `buffer`. A message that fails to
/// parse (odd values, modded clients) drops the rest of its packet instead of
/// the player: the engine reads plain ints and never kicks for their values.
fn decode_into<R, T: Decode<R>>(
    data: &[u8],
    session: &mut Session,
    buffer: &mut VecDeque<Packet>,
    channel: u8,
    wrap: fn(T) -> Packet,
) {
    let mut reader = Reader::<R>::new(data, session);
    while let Some(msg) = reader.next() {
        match msg {
            Ok(msg) => buffer.push_back(wrap(msg)),
            Err(e) => {
                log_bad_packet(channel, data, &e);
                break;
            }
        }
    }
}

struct SauerReceiver {
    rx: ConnectionReceiver,
    session: Session,
    buffer: VecDeque<Packet>,
}

#[derive(Debug)]
enum Packet {
    Chan0(Chan0Client),
    Chan1(Chan1Client),
    Chan2(Bytes),
}

impl SauerReceiver {
    fn new(peer: ConnectionReceiver) -> Self {
        Self {
            rx: peer,
            session: Session::new(),
            buffer: VecDeque::new(),
        }
    }

    async fn recv(&mut self) -> Result<Packet> {
        loop {
            if let Some(msg) = self.buffer.pop_front() {
                return Ok(msg);
            }

            let (channel, data) = match self.rx.recv().await {
                Some(ConnEvent::Receive {
                    channel_id,
                    payload,
                }) => (channel_id, payload),
                Some(ConnEvent::Disconnect { data }) => bail!("peer disconnected ({data})"),
                None => bail!("connection closed"),
            };

            match channel {
                sauer_proto::CHANNEL_POSITIONS => decode_into::<C2S<Chan0>, _>(
                    &data,
                    &mut self.session,
                    &mut self.buffer,
                    channel,
                    Packet::Chan0,
                ),
                sauer_proto::CHANNEL_MESSAGES => decode_into::<C2S<Chan1>, _>(
                    &data,
                    &mut self.session,
                    &mut self.buffer,
                    channel,
                    Packet::Chan1,
                ),
                sauer_proto::CHANNEL_MAPS => {
                    self.buffer.push_back(Packet::Chan2(data));
                }
                _ => tracing::warn!("ignored packet on unexpected channel {channel}"),
            }
        }
    }
}
