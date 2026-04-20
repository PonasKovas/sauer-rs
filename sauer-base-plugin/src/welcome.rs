use crate::{
    items::get_items,
    options::get_options,
    players::{get_players, state::set_initial_state, team::set_initial_team},
    util::broadcast_spawn_state,
};
use sauer_proto::{
    channel1::{ClipboardServer, InitClient, ItemList, MapChange, Resume, Welcome},
    data::ClientResumeState,
};
use sauer_server::Server;

/// Sends a `N_WELCOME` packet and so on, only call this if you disabled instant welcome in [`BasePluginOptions`]
pub fn welcome(server: &Server, cn: u32) {
    set_initial_team(server, cn);
    set_initial_state(server, cn);

    server.send_chan1(cn, &Welcome {});

    {
        let options = get_options(server).read().unwrap();
        let items = get_items(server).read().unwrap();
        let has_items = items.notgotitems;
        server.send_chan1(
            cn,
            &MapChange {
                map_name: options.map.clone(),
                mode: options.mode,
                has_items,
            },
        );
        if has_items {
            server.send_chan1(
                cn,
                &ItemList {
                    items: items.spawned_items(),
                },
            );
        }
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

    {
        let p = get_players(server);
        server.send_chan1(
            cn,
            &Resume {
                clients: server
                    .client_cns()
                    .into_iter()
                    .filter(|other_cn| *other_cn != cn)
                    .filter_map(|cn| {
                        let p = p.get(cn);

                        p.state.clone().map(|player_state| ClientResumeState {
                            client_num: cn as i32,
                            state: p.client_state,
                            frags: p.frags,
                            flags: 0, // TODO
                            deaths: p.deaths,
                            quad_millis: 0, // TODO
                            player_state,
                        })
                    })
                    .collect(),
            },
        );
    }

    // notify the new player of existing players
    for other_cn in server.client_cns() {
        if other_cn == cn {
            continue;
        }
        let p = get_players(server).get(other_cn);
        let (name, player_model, team) = (p.name.clone(), p.model, p.team.clone());
        drop(p);
        server.send_chan1(
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

    // Send each existing player's clipboard to the newly connected client
    // so they have the current clipboard state for coop editing.
    for other_cn in server.client_cns() {
        if other_cn == cn {
            continue;
        }
        let p = get_players(server).get(other_cn);
        if let Some(cb) = p.clipboard.as_ref() {
            server.send_chan1(
                cn,
                &ClipboardServer {
                    client_num: other_cn as i32,
                    unpack_len: cb.unpack_len,
                    data: cb.data.clone(),
                },
            );
        }
    }
}
