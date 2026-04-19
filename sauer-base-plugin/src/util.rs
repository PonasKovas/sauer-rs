use sauer_proto::channel1::SpawnState;
use sauer_server::Server;

use crate::player_states::get_playerstates;

pub(crate) fn broadcast_spawn_state(server: &Server, cn: u32) {
    let state = get_playerstates(server).get(cn);

    server.broadcast_chan1(&SpawnState {
        client_num: cn as i32,
        state,
    });
}
