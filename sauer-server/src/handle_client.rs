use crate::{Server, plugin::Plugin};
use anyhow::{Result, bail};
use bytes::Bytes;
use cube_string::{Color, CubeString, IntoCubeStringLossy};
use enet_rs::PeerReceiver;
use sauer_proto::{
    Chan0Client, Chan1Client, Chan1Server, Reader,
    data::{ArmourType, GameMode, Gun, PlayerAmmo, PlayerState},
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

    server
        .get_client(cn)
        .send_chan1(&Chan1Server::Welcome)
        .await;
    server
        .get_client(cn)
        .send_chan1(&Chan1Server::MapChange {
            map_name: "complex".lossy_cubestring(),
            mode: GameMode::CoopEdit,
            has_items: true,
        })
        .await;

    server
        .get_client(cn)
        .send_chan1(&Chan1Server::SpawnState {
            client_num: 0,
            state: PlayerState {
                life_sequence: 0,
                health: 100,
                max_health: 100,
                armour: 0,
                armour_type: ArmourType::Yellow,
                gun_select: Gun::Rifle,
                ammo: PlayerAmmo {
                    rifle: 9999,
                    ..PlayerAmmo::default()
                },
            },
        })
        .await;

    loop {
        let pkt = rx.recv().await?;
        info!("received: {pkt:?}");
    }
}

struct SauerReceiver {
    rx: PeerReceiver,
    buffer: VecDeque<Packet>,
}

#[derive(Debug)]
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
