//! Networking abstraction module

use sauer_proto::{
    PacketSequence, Writer,
    route::{Chan0, Chan1, Chan2, S2C},
};
use std::net::SocketAddr;

pub struct Peer {
    raw: enet_rs::PeerSender,
}

impl Peer {
    pub(crate) fn new(peer: enet_rs::PeerSender) -> Self {
        Self { raw: peer }
    }

    pub fn try_send_chan0(&self, pkt: impl PacketSequence<S2C<Chan0>>) {
        let mut writer = Writer::<S2C<Chan0>>::new();
        pkt.push_to(&mut writer);
        let (bytes, mode) = writer.consume();
        if let Err(e) = self.raw.try_send_packet(enet_rs::Packet {
            data: bytes.into(),
            channel: sauer_proto::CHANNEL_POSITIONS,
            mode: convert_mode(mode),
        }) {
            tracing::error!("ENet dropped Chan0 packet: {:?}", e);
        }
    }

    pub fn try_send_chan1(&self, pkt: impl PacketSequence<S2C<Chan1>>) {
        let mut writer = Writer::<S2C<Chan1>>::new();
        pkt.push_to(&mut writer);
        let (bytes, mode) = writer.consume();
        if let Err(e) = self.raw.try_send_packet(enet_rs::Packet {
            data: bytes.into(),
            channel: sauer_proto::CHANNEL_MESSAGES,
            mode: convert_mode(mode),
        }) {
            tracing::error!("ENet dropped Chan1 packet: {:?}", e);
        }
    }

    pub fn try_send_chan2(&self, pkt: impl PacketSequence<S2C<Chan2>>) {
        let mut writer = Writer::<S2C<Chan2>>::new();
        pkt.push_to(&mut writer);
        let (bytes, mode) = writer.consume();
        if let Err(e) = self.raw.try_send_packet(enet_rs::Packet {
            data: bytes.into(),
            channel: sauer_proto::CHANNEL_MAPS,
            mode: convert_mode(mode),
        }) {
            tracing::error!("ENet dropped Chan2 packet: {:?}", e);
        }
    }

    pub fn try_disconnect(&self) {
        self.raw.try_disconnect(0);
    }

    pub fn addr(&self) -> SocketAddr {
        self.raw.addr()
    }
}

fn convert_mode(sauer_proto_mode: sauer_proto::SendMode) -> enet_rs::SendMode {
    match sauer_proto_mode {
        sauer_proto::SendMode::Reliable => enet_rs::SendMode::Reliable,
        sauer_proto::SendMode::Unreliable => enet_rs::SendMode::Unreliable,
    }
}
