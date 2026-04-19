use crate::registry::ClientRegistry;
use sauer_proto::channel1::{SpectatorClient, SwitchModel, SwitchName, SwitchTeam};
use sauer_server::{
    Server,
    plugin::{Control, PASS},
};
use std::sync::Arc;
use tracing::debug;

pub async fn on_switch_name(server: &Arc<Server>, cn: u32, packet: &SwitchName) -> Control {
    let registry = server.extra.get::<ClientRegistry>().unwrap();
    registry.set_name(cn, packet.name.clone(), server);
    debug!("cn {} changed name to {}", cn, packet.name);
    PASS
}

pub async fn on_switch_model(server: &Arc<Server>, cn: u32, packet: &SwitchModel) -> Control {
    let registry = server.extra.get::<ClientRegistry>().unwrap();
    registry.set_model(cn, packet.model, server);
    PASS
}

pub async fn on_switch_team(server: &Arc<Server>, cn: u32, packet: &SwitchTeam) -> Control {
    let registry = server.extra.get::<ClientRegistry>().unwrap();
    registry.set_team(cn, packet.team.clone(), server);
    PASS
}

pub async fn on_spectator(server: &Arc<Server>, cn: u32, packet: &SpectatorClient) -> Control {
    // Only handle self-spectate requests for now.
    // Admin-forced spectation (spectator_cn != cn) requires privilege checks
    // that belong to a higher-level plugin.
    if packet.spectator_cn != cn as i32 {
        return PASS;
    }

    // TODO: deny if mastermode is Locked or Private and client lacks privilege.

    let registry = server.extra.get::<ClientRegistry>().unwrap();
    registry.set_spectator(cn, packet.val, server);
    PASS
}
