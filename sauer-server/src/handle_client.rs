use crate::{Server, plugin::Plugin};
use anyhow::{Result, bail};
use bytes::Bytes;
use cube_string::{Color, CubeString};
use enet_rs::PeerReceiver;
use sauer_proto::{
    Chan0Client, Chan1Client, Chan1Server, Reader,
    protocol_route::{C2S, Chan0, Chan1},
};
use std::{collections::VecDeque, sync::Arc};
use tracing::info;

pub(crate) async fn handle_client<P: Plugin>(
    server: &Arc<Server>,
    plugin: &Arc<P>,
    peer: PeerReceiver,
    cn: u32,
) -> Result<()> {
    let mut rx = SauerReceiver::new(peer);

    server
        .get_client(cn)
        .send_chan1(&Chan1Server::ServInfo {
            client_num: cn as i32,
            protocol: sauer_proto::PROTOCOL_VERSION,
            session_id: 0,
            has_password: false,
            server_desc: CubeString::builder()
                .color(Color::Magenta)
                .push_str("Retard server v2")
                .build(),
            server_auth: CubeString::new(),
        })
        .await;

    let player_name = 'outer: loop {
        let pkt = rx.recv().await?;

        match pkt {
            Packet::Chan1(Chan1Client::Connect {
                name,
                player_model: _,
                password_hash: _,
                auth_desc: _,
                auth_name: _,
            }) => break 'outer name,
            _ => continue,
        };
    };

    info!("{player_name} connected");

    Ok(())
}

struct SauerReceiver {
    rx: PeerReceiver,
    buffer: VecDeque<Packet>,
}

enum Packet {
    Chan0(Chan0Client),
    Chan1(Chan1Client),
    Chan2(Bytes),
}

impl SauerReceiver {
    fn new(peer: PeerReceiver) -> Self {
        Self {
            rx: peer,
            buffer: VecDeque::new(),
        }
    }
    async fn recv(&mut self) -> Result<Packet> {
        loop {
            // If we have buffered messages from the last ENet packet, return them first.
            if let Some(msg) = self.buffer.pop_front() {
                return Ok(msg);
            }

            // Wait for the next ENet packet
            let pkt = match self.rx.rx.recv().await {
                Some(Ok(x)) => x,
                Some(Err(e)) => bail!("enet connection error: {e:?}"),
                None => bail!("enet packet read channel lost sender"),
            };

            match pkt.channel {
                sauer_proto::CHANNEL_POSITIONS => {
                    for msg in Reader::<C2S<Chan0>>::new(&pkt.data) {
                        self.buffer.push_back(Packet::Chan0(msg?));
                    }
                }
                sauer_proto::CHANNEL_MESSAGES => {
                    for msg in Reader::<C2S<Chan1>>::new(&pkt.data) {
                        self.buffer.push_back(Packet::Chan1(msg?));
                    }
                }
                sauer_proto::CHANNEL_MAPS => {
                    // Channel 2 (File/Map transfer): Just raw bytes, no packet wrappers.
                    self.buffer.push_back(Packet::Chan2(pkt.data));
                }
                _ => panic!(),
            }
        }
    }
}
