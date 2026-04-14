use cube_string::{Color, CubeString};
use enet_rs::{Host, HostConfig, Peer, SendMode};
use ogz_rs::OgzMap;
use sauer_map::World;
use sauer_proto::{
    Chan0Client, Chan1Client, Chan1Server, Reader, Writer,
    data::{ArmourType, GameMode, Gun, PlayerState},
    protocol_route::{C2S, Chan0, Chan1, S2C},
};
use std::sync::Arc;

const MAP: &str = "glasstest";

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    let ogz_map = Arc::new(OgzMap::parse(
        &tokio::fs::read("/home/mykolas/.config/sauerbraten/packages/base/glasstest.ogz").await?,
    )?);

    let mut host = Host::bind(
        "127.0.0.1:28785",
        HostConfig {
            max_peers: 1024,
            channel_count: sauer_proto::CHANNEL_COUNT as u8,
            use_checksum: false,
            incoming_bandwidth: 0,
            outgoing_bandwidth: 0,
        },
    )
    .await?;

    while let Some(peer) = host.accept().await {
        let ogz_map = Arc::clone(&ogz_map);
        tokio::spawn(async move {
            if let Err(e) = handle_client(peer, ogz_map).await {
                println!("Error: {e}");
            }
        });
    }

    Ok(())
}

async fn handle_client(mut peer: Peer, ogz: Arc<OgzMap>) -> anyhow::Result<()> {
    let map = World::from_ogz(&ogz);

    send_chan1(
        &mut peer,
        &Chan1Server::ServInfo {
            client_num: 0,
            protocol: sauer_proto::PROTOCOL_VERSION,
            session_id: 0,
            has_password: false,
            server_desc: CubeString::builder()
                .color(Color::Magenta)
                .push_str("Retard server v2")
                .build(),
            server_auth: "".into(),
        },
    )
    .await?;

    let player_name = 'outer: loop {
        let pkt = peer.recv().await?;
        if pkt.channel != sauer_proto::CHANNEL_MESSAGES {
            continue;
        }

        for pkt in Reader::<C2S<Chan1>>::new(&pkt.data) {
            match pkt? {
                Chan1Client::Connect {
                    name,
                    player_model: _,
                    password_hash: _,
                    auth_desc: _,
                    auth_name: _,
                } => break 'outer name,
                _ => continue,
            }
        }
    };

    println!("{player_name} connected");

    send_chan1(&mut peer, &Chan1Server::Welcome).await?;
    send_chan1(
        &mut peer,
        &Chan1Server::MapChange {
            map_name: MAP.into(),
            mode: GameMode::CoopEdit,
            has_items: true,
        },
    )
    .await?;

    loop {
        let pkt = peer.recv().await?;
        match pkt.channel {
            sauer_proto::CHANNEL_MESSAGES => {
                for pkt in Reader::<C2S<Chan1>>::new(&pkt.data) {
                    match pkt? {
                        Chan1Client::TrySpawn => {
                            send_chan1(
                                &mut peer,
                                &Chan1Server::SpawnState {
                                    client_num: 0,
                                    state: PlayerState {
                                        life_sequence: 0,
                                        health: 100,
                                        max_health: 100,
                                        armour: 0,
                                        armour_type: ArmourType::Yellow,
                                        gun_select: Gun::Fist,
                                        ammo: [0; 6],
                                    },
                                },
                            )
                            .await?;
                        }
                        other => {
                            println!("rcv: {other:?}");
                        }
                    }
                }
            }
            sauer_proto::CHANNEL_POSITIONS => {
                for pkt in Reader::<C2S<Chan0>>::new(&pkt.data) {
                    match pkt? {
                        Chan0Client::Position { pos } => {
                            let cube = map.get_cube(pos.pos.x, pos.pos.y, pos.pos.z - 15.);

                            let text = CubeString::builder()
                                .color(Color::Orange)
                                .push_str(&format!("{:?}", cube.map(|c| c.0.material)))
                                .build();

                            send_chan1(
                                &mut peer,
                                &Chan1Server::Text {
                                    client_num: 0,
                                    text,
                                },
                            )
                            .await?;
                        }
                        _ => {}
                    }
                }
            }
            _ => {}
        }
    }
}

async fn send_chan1(peer: &mut Peer, pkt: &Chan1Server) -> enet_rs::Result<()> {
    let mut writer = Writer::<S2C<Chan1>>::new();
    writer.push(pkt);
    let bytes = writer.consume();

    peer.send_packet(enet_rs::Packet {
        data: bytes.into(),
        channel: 1,
        mode: SendMode::Reliable,
    })
    .await?;

    Ok(())
}
