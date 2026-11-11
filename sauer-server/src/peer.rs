//! Networking abstraction module

use lenet::SendFlags;
use lenet_tokio::ConnectionSender;
use sauer_proto::{
    PacketSequence, Writer,
    route::{Channel, S2C},
};
use std::net::SocketAddr;

pub struct Peer {
    raw: ConnectionSender,
}

impl Peer {
    pub(crate) fn new(peer: ConnectionSender) -> Self {
        Self { raw: peer }
    }

    pub fn try_send<C: Channel>(&self, pkt: impl PacketSequence<S2C<C>>) {
        let mut writer = Writer::<S2C<C>>::new();
        pkt.push_to(&mut writer);
        let (bytes, mode) = writer.consume();
        if let Err(e) = self.raw.try_send(C::ID, convert_mode(mode), bytes) {
            // Only fails once the connection is closed (peer leaving).
            tracing::debug!("dropped channel {} packet to closed peer: {e}", C::ID);
        }
    }

    /// Disconnects with an engine `DISC_*` reason the client shows (0 for none).
    pub fn try_disconnect(&self, reason: u32) {
        let _ = self.raw.try_disconnect(reason);
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
