use crate::{
    options::get_options,
    players::{
        get_players,
        state::set_initial_state,
        team::set_initial_team,
    },
    util::broadcast_spawn_state,
};
use sauer_proto::channel1::{InitClient, MapChange, Welcome};
use sauer_server::Server;

/// Sends a `N_WELCOME` packet and so on, only call this if you disabled instant welcome in [`BasePluginOptions`]
pub fn welcome(server: &Server, cn: u32) {
    set_initial_team(server, cn);
    set_initial_state(server, cn);

    server.send_chan1(cn, &Welcome {});

    {
        let options = get_options(server).read().unwrap();
        server.send_chan1(
            cn,
            &MapChange {
                map_name: options.map.clone(),
                mode: options.mode,
                has_items: false, // TODO
            },
        );
    }

    // notify existing players of the new player
    {
        let p = get_players(server).get(cn);
        let (name, player_model, team) = (p.name.clone(), p.model, p.team.clone());
        drop(p);
        server.broadcast_except_chan1(
            cn,
            &InitClient {
                client_num: cn as i32,
                name,
                player_model,
                team,
            },
        );
    }

    // notify the new player of existing players
    for other_cn in server.client_cns() {
        let p = get_players(server).get(other_cn);
        let (name, player_model, team) = (p.name.clone(), p.model, p.team.clone());
        drop(p);
        server.broadcast_except_chan1(
            cn,
            &InitClient {
                client_num: other_cn as i32,
                name,
                player_model,
                team,
            },
        );
    }

    broadcast_spawn_state(server, cn);
}
