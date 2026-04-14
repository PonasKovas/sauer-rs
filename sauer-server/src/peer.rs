//! Networking abstraction module

use enet_rs::SendMode;
use sauer_proto::{
    Chan0Server, Chan1Server, Chan2Server, Writer,
    protocol_route::{Chan0, Chan1, Chan2, S2C},
};
use std::net::SocketAddr;
use tracing::error;

pub struct Peer {
    raw: enet_rs::PeerSender,
}

impl Peer {
    pub fn new(peer: enet_rs::PeerSender) -> Self {
        Self { raw: peer }
    }

    pub async fn send_chan0(&self, pkt: &Chan0Server) {
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

    pub async fn send_chan1(&self, pkt: &Chan1Server) {
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

    pub async fn send_chan2(&self, pkt: &Chan2Server) {
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
