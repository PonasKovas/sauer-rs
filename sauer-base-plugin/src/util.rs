use sauer_proto::channel1::SpawnState;
use sauer_server::Server;

use crate::players::get_players;

pub(crate) fn broadcast_spawn_state(server: &Server, cn: u32) {
    let state = get_players(server)
        .get(cn)
        .state
        .clone()
        .expect("broadcast_spawn_state called before state was initialized");

    server.broadcast_chan1(&SpawnState {
        client_num: cn as i32,
        state,
    });
}
