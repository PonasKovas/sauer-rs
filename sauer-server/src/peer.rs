//! Networking abstraction module

use sauer_proto::{
    PacketSequence, Writer,
    route::{Chan0, Chan1, Chan2, S2C},
};
use lenet::SendFlags;
use lenet_tokio::ConnectionSender;
use std::net::SocketAddr;

pub struct Peer {
    raw: ConnectionSender,
}

impl Peer {
    pub(crate) fn new(peer: ConnectionSender) -> Self {
        Self { raw: peer }
    }

    pub fn try_send_chan0(&self, pkt: impl PacketSequence<S2C<Chan0>>) {
        let mut writer = Writer::<S2C<Chan0>>::new();
        pkt.push_to(&mut writer);
        let (bytes, mode) = writer.consume();
        if let Err(e) = self
            .raw
            .try_send(sauer_proto::CHANNEL_POSITIONS, convert_mode(mode), bytes)
        {
            tracing::error!("ENet dropped Chan0 packet: {:?}", e);
        }
    }

    pub fn try_send_chan1(&self, pkt: impl PacketSequence<S2C<Chan1>>) {
        let mut writer = Writer::<S2C<Chan1>>::new();
        pkt.push_to(&mut writer);
        let (bytes, mode) = writer.consume();
        if let Err(e) = self
            .raw
            .try_send(sauer_proto::CHANNEL_MESSAGES, convert_mode(mode), bytes)
        {
            tracing::error!("ENet dropped Chan1 packet: {:?}", e);
        }
    }

    pub fn try_send_chan2(&self, pkt: impl PacketSequence<S2C<Chan2>>) {
        let mut writer = Writer::<S2C<Chan2>>::new();
        pkt.push_to(&mut writer);
        let (bytes, mode) = writer.consume();
        if let Err(e) = self
            .raw
            .try_send(sauer_proto::CHANNEL_MAPS, convert_mode(mode), bytes)
        {
            tracing::error!("ENet dropped Chan2 packet: {:?}", e);
        }
    }

    pub fn try_disconnect(&self) {
        let _ = self.raw.try_disconnect(0);
    }

    pub fn addr(&self) -> SocketAddr {
        self.raw.remote_addr().into()
    }
}

fn convert_mode(sauer_proto_mode: sauer_proto::SendMode) -> SendFlags {
    match sauer_proto_mode {
        sauer_proto::SendMode::Reliable => SendFlags::RELIABLE,
        sauer_proto::SendMode::Unreliable => SendFlags::UNRELIABLE,
    }
}
