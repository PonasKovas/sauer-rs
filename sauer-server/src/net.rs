//! Networking abstraction module

use enet_rs::{Peer, SendMode};
use sauer_proto::{
    Chan0Client, Chan0Server, Chan1Client, Chan1Server, Chan2Server, Reader, Writer,
    protocol_route::{C2S, Chan0, Chan1, Chan2, S2C},
};
use std::collections::VecDeque;

pub enum ClientMessage {
    Chan0(Chan0Client),
    Chan1(Chan1Client),
    #[allow(dead_code)]
    Chan2(Vec<u8>),
}

pub struct SauerPeer {
    pub raw: Peer,
    buffer: VecDeque<ClientMessage>,
}

impl SauerPeer {
    pub fn new(peer: Peer) -> Self {
        Self {
            raw: peer,
            buffer: VecDeque::new(),
        }
    }

    /// Receives the next parsed message. Handles internal un-batching of Sauerbraten packets.
    pub async fn recv(&mut self) -> anyhow::Result<ClientMessage> {
        loop {
            // If we have buffered messages from the last ENet packet, return them first.
            if let Some(msg) = self.buffer.pop_front() {
                return Ok(msg);
            }

            // Wait for the next ENet packet
            let pkt = self.raw.recv().await?;

            match pkt.channel {
                sauer_proto::CHANNEL_POSITIONS => {
                    for msg in Reader::<C2S<Chan0>>::new(&pkt.data) {
                        self.buffer.push_back(ClientMessage::Chan0(msg?));
                    }
                }
                sauer_proto::CHANNEL_MESSAGES => {
                    for msg in Reader::<C2S<Chan1>>::new(&pkt.data) {
                        self.buffer.push_back(ClientMessage::Chan1(msg?));
                    }
                }
                sauer_proto::CHANNEL_MAPS => {
                    // Channel 2 (File/Map transfer): Just raw bytes, no packet wrappers.
                    self.buffer.push_back(ClientMessage::Chan2(pkt.data.into()));
                }
                _ => panic!(),
            }
        }
    }

    pub async fn send_chan0(&mut self, pkt: &Chan0Server) -> enet_rs::Result<()> {
        let mut writer = Writer::<S2C<Chan0>>::new();
        writer.push(pkt);
        let bytes = writer.consume();

        self.raw
            .send_packet(enet_rs::Packet {
                data: bytes.into(),
                channel: sauer_proto::CHANNEL_POSITIONS,
                mode: SendMode::Unreliable,
            })
            .await
    }

    pub async fn send_chan1(&mut self, pkt: &Chan1Server) -> enet_rs::Result<()> {
        let mut writer = Writer::<S2C<Chan1>>::new();
        writer.push(pkt);
        let bytes = writer.consume();

        self.raw
            .send_packet(enet_rs::Packet {
                data: bytes.into(),
                channel: sauer_proto::CHANNEL_MESSAGES,
                mode: SendMode::Reliable,
            })
            .await
    }

    #[allow(dead_code)]
    pub async fn send_chan2(&mut self, pkt: &Chan2Server) -> enet_rs::Result<()> {
        let mut writer = Writer::<S2C<Chan2>>::new();
        writer.push(pkt);
        let bytes = writer.consume();

        self.raw
            .send_packet(enet_rs::Packet {
                data: bytes.into(),
                channel: sauer_proto::CHANNEL_MESSAGES,
                mode: SendMode::Reliable,
            })
            .await
    }
}
