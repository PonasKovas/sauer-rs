//! Networking abstraction module

use enet_rs::SendMode;
use sauer_proto::{
    Encode, Writer,
    route::{Chan0, Chan1, Chan2, S2C},
};
use std::net::SocketAddr;
use tracing::error;

pub struct Peer {
    raw: enet_rs::PeerSender,
}

impl Peer {
    pub(crate) fn new(peer: enet_rs::PeerSender) -> Self {
        Self { raw: peer }
    }

    pub async fn send_chan0(&self, pkt: &impl Encode<S2C<Chan0>>) {
        let mut writer = Writer::<S2C<Chan0>>::new();
        writer.push(pkt);
        let bytes = writer.consume();

        if let Err(e) = self
            .raw
            .send_packet(enet_rs::Packet {
                data: bytes.into(),
                channel: sauer_proto::CHANNEL_POSITIONS,
                mode: SendMode::Unreliable,
            })
            .await
        {
            error!("error sending packet: {e}");
        }
    }

    /// Send a channel-1 packet without blocking. Returns `false` and logs a
    /// warning if the peer's send buffer is full or the channel is closed.
    pub fn try_send_chan1(&self, pkt: &impl Encode<S2C<Chan1>>) -> bool {
        let mut writer = Writer::<S2C<Chan1>>::new();
        writer.push(pkt);
        let bytes = writer.consume();

        self.raw
            .try_send_packet(enet_rs::Packet {
                data: bytes.into(),
                channel: sauer_proto::CHANNEL_MESSAGES,
                mode: SendMode::Reliable,
            })
            .is_ok()
    }

    pub async fn send_chan1(&self, pkt: &impl Encode<S2C<Chan1>>) {
        let mut writer = Writer::<S2C<Chan1>>::new();
        writer.push(pkt);
        let bytes = writer.consume();

        if let Err(e) = self
            .raw
            .send_packet(enet_rs::Packet {
                data: bytes.into(),
                channel: sauer_proto::CHANNEL_MESSAGES,
                mode: SendMode::Reliable,
            })
            .await
        {
            error!("error sending packet: {e}");
        }
    }

    pub async fn send_chan2(&self, pkt: &impl Encode<S2C<Chan2>>) {
        let mut writer = Writer::<S2C<Chan2>>::new();
        writer.push(pkt);
        let bytes = writer.consume();

        if let Err(e) = self
            .raw
            .send_packet(enet_rs::Packet {
                data: bytes.into(),
                channel: sauer_proto::CHANNEL_MESSAGES,
                mode: SendMode::Reliable,
            })
            .await
        {
            error!("error sending packet: {e}");
        }
    }

    pub fn addr(&self) -> SocketAddr {
        self.raw.addr()
    }
}
