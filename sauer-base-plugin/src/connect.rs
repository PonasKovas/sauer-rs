use crate::{client::ClientData, options::BasePluginOptions, registry::ClientRegistry};
use cube_string::CubeString;
use rand::Rng;
use sauer_proto::{
    PROTOCOL_VERSION,
    channel1::{self, Connect},
};
use sauer_server::{
    Server,
    plugin::{Control, PASS},
};
use std::sync::Arc;
use tracing::info;

/// Called when a peer physically establishes an ENet connection.
/// Generates a session ID and sends N_SERVINFO immediately so the client
/// can hash its password before sending N_CONNECT.
pub async fn on_connected(
    server: &Arc<Server>,
    cn: u32,
    options: &BasePluginOptions,
) -> Control {
    let session_id: i32 = rand::thread_rng().gen_range(0..0x1000000i32);

    let registry = server.extra.get::<ClientRegistry>().unwrap();
    registry.begin_pending(cn, session_id);

    server.send_chan1(
        cn,
        &channel1::ServInfo {
            client_num: cn as i32,
            protocol: PROTOCOL_VERSION,
            session_id,
            has_password: options.has_password,
            server_desc: options.server_desc.clone(),
            server_auth: options.server_auth.clone(),
        },
    );

    PASS
}

/// Called when the client sends N_CONNECT (name, model, password hash).
pub async fn on_connect(server: &Arc<Server>, cn: u32, packet: &Connect) -> Control {
    let registry = server.extra.get::<ClientRegistry>().unwrap();

    let client = ClientData::new(
        packet.name.clone(),
        CubeString::new(), // team is assigned by game-mode logic
        packet.player_model,
    );

    // Promote from pending → connected. If not pending (shouldn't happen),
    // the connection is in a bad state; the server will clean it up on disconnect.
    if !registry.complete_pending(cn, client) {
        return PASS;
    }

    // 1. N_WELCOME — client is now accepted and should start rendering.
    server.send_chan1(cn, &channel1::Welcome {});

    // 2. N_RESUME — snapshot of all other connected clients' state so the
    //    joining client can populate its scoreboard.
    let resume_states = registry.resume_states(cn);
    server.send_chan1(
        cn,
        &channel1::Resume {
            clients: resume_states,
        },
    );

    // 3. N_INITCLIENT — tell all other clients about the new player.
    let client_ref = registry.get(cn).unwrap();
    server.broadcast_except_chan1(
        cn,
        &channel1::InitClient {
            client_num: cn as i32,
            name: client_ref.name().clone(),
            team: client_ref.team().clone(),
            player_model: client_ref.player_model(),
        },
    );

    info!("cn {} ({}) joined", cn, packet.name);

    PASS
}

pub async fn on_disconnect(server: &Arc<Server>, cn: u32) {
    let registry = server.extra.get::<ClientRegistry>().unwrap();

    // Only broadcast N_CDIS if the client had completed the N_CONNECT handshake.
    if registry.remove(cn) {
        server.broadcast_chan1(&channel1::ClientDisconnected {
            client_num: cn as i32,
        });
        info!("cn {} disconnected", cn);
    }
}
