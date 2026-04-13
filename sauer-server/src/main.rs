use cube_string::{Color, CubeString};
use enet_rs::{Host, HostConfig, Peer, SendMode};
use sauer_proto::{
    Chan1Client, Chan1Server, Reader, Writer,
    data::{Gun, PlayerState},
    protocol_route::{C2S, Chan1, S2C},
};

#[tokio::main]
async fn main() -> anyhow::Result<()> {
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
        tokio::spawn(async move {
            if let Err(e) = handle_client(peer).await {
                println!("Error: {e}");
            }
        });
    }

    Ok(())
}

async fn handle_client(mut peer: Peer) -> anyhow::Result<()> {
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
            map_name: "face-capture".into(),
            mode: 0,
            has_items: 0,
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
                                        armour_type: 0,
                                        gun_select: Gun::Fist,
                                        ammo: vec![],
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
