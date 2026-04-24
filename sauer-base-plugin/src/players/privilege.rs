use sauer_proto::{
    channel1::CurrentMaster,
    data::{MasterClient, MasterMode, Privilege},
};
use sauer_server::Server;

use crate::options::get_options;

use super::get_players;

fn build_current_master(server: &Server) -> CurrentMaster {
    let master_mode = get_options(server).read().unwrap().master_mode;
    let clients: Vec<MasterClient> = get_players(server)
        .map
        .iter()
        .filter(|e| e.privilege != Privilege::None)
        .map(|e| MasterClient {
            client_num: *e.key() as i32,
            privilege: e.privilege,
        })
        .collect();
    CurrentMaster { master_mode, clients }
}

/// Sends the current master state to a single client (call during welcome).
pub fn send_current_master(server: &Server, cn: u32) {
    server.send_chan1(cn, &build_current_master(server));
}

/// Broadcasts the current master state to all connected clients.
pub fn broadcast_current_master(server: &Server) {
    server.broadcast_chan1(&build_current_master(server));
}

/// Returns a player's current privilege level.
pub fn get_privilege(server: &Server, cn: u32) -> Privilege {
    get_players(server)
        .try_get(cn)
        .map_or(Privilege::None, |p| p.privilege)
}

/// Sets a player's privilege level and broadcasts the updated master list to everyone.
pub fn set_privilege(server: &Server, cn: u32, privilege: Privilege) {
    if let Some(mut p) = get_players(server).try_get_mut(cn) {
        p.privilege = privilege;
    }
    broadcast_current_master(server);
}

/// Sets the master mode and broadcasts the updated master state to everyone.
pub fn set_master_mode(server: &Server, mode: MasterMode) {
    get_options(server).write().unwrap().master_mode = mode;
    broadcast_current_master(server);
}

/// Returns the current master mode.
pub fn get_master_mode(server: &Server) -> MasterMode {
    get_options(server).read().unwrap().master_mode
}
