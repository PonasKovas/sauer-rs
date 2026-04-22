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

    // Extract map/item state while holding both locks, then release before sending.
    let (map_name, mode, notgotitems, spawned_items) = {
        let options = get_options(server).read().unwrap();
        let items = get_items(server).read().unwrap();
        let notgotitems = items.notgotitems;
        let spawned = (!notgotitems).then(|| items.spawned_items());
        (options.map.clone(), options.mode, notgotitems, spawned)
    };

    let mapchange = MapChange {
        map_name,
        mode,
        has_items: notgotitems,
    };

    // stupid sauer requires N_ITEMLIST to come together with N_MAPCHANGE in the same enet packet, otherwise wont work lol
    if let Some(items) = spawned_items {
        server.send_chan1(cn, (mapchange, ItemList { items }));
    } else {
        server.send_chan1(cn, mapchange);
    }

    // Notify existing players of the new player.
    {
        let p = get_players(server).get(cn);
        server.broadcast_except_chan1(
            cn,
            &InitClient {
                client_num: cn as i32,
                name: p.name.clone(),
                player_model: p.model,
                team: p.team.clone(),
            },
        );
    }

    // Send the new player a resume snapshot of all existing players.
    {
        let p = get_players(server);
        server.send_chan1(
            cn,
            &Resume {
                clients: server
                    .client_cns()
                    .into_iter()
                    .filter(|&other| other != cn)
                    .filter_map(|other| {
                        let p = p.get(other);
                        p.state.clone().map(|player_state| ClientResumeState {
                            client_num: other as i32,
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

    // Introduce all existing players to the new player and send their clipboards
    // in a single pass so we only iterate client_cns() once.
    for other_cn in server.client_cns().into_iter().filter(|&other| other != cn) {
        let p = get_players(server).get(other_cn);
        let name = p.name.clone();
        let player_model = p.model;
        let team = p.team.clone();
        let clipboard = p.clipboard.as_ref().map(|cb| ClipboardServer {
            client_num: other_cn as i32,
            unpack_len: cb.unpack_len,
            data: cb.data.clone(),
        });
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
        if let Some(cb) = clipboard {
            server.send_chan1(cn, &cb);
        }
    }

    broadcast_spawn_state(server, cn);
}
