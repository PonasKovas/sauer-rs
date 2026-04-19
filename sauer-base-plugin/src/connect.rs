use crate::{
    client::ClientData,
    crypto,
    master::MasterState,
    options::BasePluginOptions,
    registry::ClientRegistry,
};
use cube_string::CubeString;
use rand::Rng;
use sauer_proto::{
    PROTOCOL_VERSION,
    channel1::{self, Connect},
    data::MasterMode,
};
use sauer_server::{
    Server,
    plugin::{Control, PASS},
};
use std::sync::Arc;
use tracing::info;

/// Called when a peer physically establishes an ENet connection.
/// Generates a session ID and sends N_SERVINFO immediately so the client
/// can hash its password and prepare its auth key before sending N_CONNECT.
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
            has_password: options.password.is_some(),
            server_desc: options.server_desc.clone(),
            server_auth: options.server_auth.clone(),
        },
    );

    PASS
}

/// Called when the client sends N_CONNECT (name, model, password hash, auth info).
pub async fn on_connect(
    server: &Arc<Server>,
    cn: u32,
    packet: &Connect,
    options: &BasePluginOptions,
) -> Control {
    let registry = server.extra.get::<ClientRegistry>().unwrap();

    // Verify the join password before accepting the client.
    if let Some(password) = &options.password {
        let session_id = match registry.pending_session_id(cn) {
            Some(id) => id,
            None => return PASS,
        };
        let expected = crypto::hash_password(cn, session_id, password);
        if expected != packet.password_hash {
            server.send_chan1(
                cn,
                &channel1::ServMsg {
                    text: CubeString::from_str_lossy("wrong password"),
                },
            );
            server.disconnect(cn);
            return PASS;
        }
    }

    let client = ClientData::new(
        packet.name.clone(),
        CubeString::new(), // team is assigned by game-mode logic
        packet.player_model,
    );

    let mastermode = server
        .extra
        .get::<MasterState>()
        .map(|m| m.mastermode())
        .unwrap_or(MasterMode::Open);

    if mastermode == MasterMode::Private {
        // The client included auth credentials in N_CONNECT (triggered by seeing
        // server_auth in N_SERVINFO). Check if they match the server auth domain
        // and a known user, then send N_AUTHCHAL. N_AUTHTRY is not involved.
        let auth_accepted = crate::master::try_connect_auth(
            server,
            cn,
            client,
            &packet.auth_desc,
            &packet.auth_name,
            options,
        )
        .await;

        if !auth_accepted {
            server.disconnect(cn);
        }
        return PASS;
    }

    // Non-private mastermodes: admit client immediately.
    if !registry.complete_pending(cn, client) {
        return PASS;
    }

    complete_connection(server, cn, &registry, mastermode).await;
    info!("cn {} ({}) joined", cn, packet.name);

    PASS
}

/// Finish admitting a client: send N_WELCOME / N_RESUME, broadcast N_INITCLIENT,
/// and enforce Locked spectator state.
///
/// Called from `on_connect` (normal flow) and from `on_auth_ans` (Private mode).
pub(crate) async fn complete_connection(
    server: &Arc<Server>,
    cn: u32,
    registry: &ClientRegistry,
    mastermode: MasterMode,
) {
    server.send_chan1(cn, &channel1::Welcome {});

    // Tell the client which map and mode are currently active.
    crate::map::send_map_info(server, cn);

    let resume_states = registry.resume_states(cn);
    server.send_chan1(cn, &channel1::Resume { clients: resume_states });

    let client_ref = registry.get(cn).unwrap();
    server.broadcast_except_chan1(
        cn,
        &channel1::InitClient {
            client_num: cn as i32,
            name: client_ref.name().to_cube_string(),
            team: client_ref.team().to_cube_string(),
            player_model: client_ref.player_model(),
        },
    );
    drop(client_ref);

    // Locked forces spectator; Private does not (auth was the gate).
    if mastermode == MasterMode::Locked {
        registry.set_spectator(cn, true, server);
    }
}

pub async fn on_disconnect(server: &Arc<Server>, cn: u32) {
    if let Some(master) = server.extra.get::<MasterState>() {
        master.cancel_pending_auth(cn);
    }

    let registry = server.extra.get::<ClientRegistry>().unwrap();

    if registry.remove(cn) {
        server.broadcast_chan1(&channel1::ClientDisconnected {
            client_num: cn as i32,
        });
        info!("cn {} disconnected", cn);
    }
}
