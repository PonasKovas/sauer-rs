use dashmap::DashMap;
use peer::Peer;

mod builder;
mod handle_client;
mod peer;
pub mod plugin;
pub mod utils;

pub use builder::ServerBuilder;

pub struct Server {
    clients: DashMap<u32, Peer>,
}

impl Server {
    pub fn get_client(&self, client_number: u32) -> dashmap::mapref::one::Ref<'_, u32, Peer> {
        match self.clients.get(&client_number) {
            Some(x) => x,
            None => panic!("tried to get client number which doesnt exist"),
        }
    }
}

//
//
//
//
//
////
//
//
//
//
//
//
//
//

/*
async fn handle_client(mut peer: SauerPeer, ogz: Arc<OgzMap>) -> anyhow::Result<()> {
    let map = World::from_ogz(&ogz);

    peer.send_chan1(&Chan1Server::ServInfo {
        client_num: 0,
        protocol: sauer_proto::PROTOCOL_VERSION,
        session_id: 0,
        has_password: false,
        server_desc: CubeString::builder()
            .color(Color::Magenta)
            .push_str("Retard server v2")
            .build(),
        server_auth: CubeString::new(),
    })
    .await?;

    let player_name = 'outer: loop {
        let pkt = peer.recv().await?;

        match pkt {
            ClientMessage::Chan1(Chan1Client::Connect {
                name,
                player_model: _,
                password_hash: _,
                auth_desc: _,
                auth_name: _,
            }) => break 'outer name,
            _ => continue,
        };
    };

    println!("{player_name} connected");

    peer.send_chan1(&Chan1Server::Welcome).await?;
    peer.send_chan1(&Chan1Server::MapChange {
        map_name: MAP.try_into().unwrap(),
        mode: GameMode::CoopEdit,
        has_items: true,
    })
    .await?;

    // Announce the fake player
    peer.send_chan1(&Chan1Server::InitClient {
        client_num: FAKE_CN,
        name: "FakeBot".lossy_cubestring(),
        team: "".lossy_cubestring(),
        player_model: PlayerModel::Ogro2,
    })
    .await?;

    // Spawn the real player
    peer.send_chan1(&Chan1Server::SpawnState {
        client_num: 0,
        state: PlayerState {
            life_sequence: 0,
            health: 100,
            max_health: 100,
            armour: 0,
            armour_type: ArmourType::Yellow,
            gun_select: Gun::Fist,
            ammo: PlayerAmmo::default(),
        },
    })
    .await?;

    // Spawn the fake player via SpawnState so the client sets
    // state=CS_ALIVE and calls pickgamespawn (placing it on map).
    // N_POS updates from us will override the position immediately.
    peer.send_chan1(&Chan1Server::SpawnState {
        client_num: FAKE_CN,
        state: PlayerState {
            life_sequence: 0,
            health: 100,
            max_health: 100,
            armour: 0,
            armour_type: ArmourType::Yellow,
            gun_select: Gun::Fist,
            ammo: PlayerAmmo::default(),
        },
    })
    .await?;
}*/
