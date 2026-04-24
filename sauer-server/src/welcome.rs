use crate::Server;
use sauer_proto::{
    channel1::{ClipboardServer, InitClient, ItemList, MapChange, Resume, Welcome},
    data::ClientResumeState,
};

impl Server {
    /// Sends the full join handshake to `cn`: N_WELCOME, N_MAPCHANGE (+N_ITEMLIST), N_CURRENTMASTER,
    /// N_INITCLIENT to existing players, N_RESUME, N_INITCLIENT for each existing player,
    /// their clipboards, and N_SPAWNSTATE broadcast.
    ///
    /// For `MasterMode::Private`, a plugin must perform auth and then call this manually.
    pub fn welcome(&self, cn: u32) {
        self.set_initial_team(cn);
        self.set_initial_state(cn);

        self.send_chan1(cn, &Welcome {});

        // Extract map/item state while holding both locks, then release before sending.
        let (map_name, mode, notgotitems, spawned_items) = {
            let items = self.items.read().unwrap();
            let notgotitems = items.notgotitems;
            let spawned = (!notgotitems).then(|| items.spawned_items());
            (
                self.map_name.read().unwrap().clone(),
                *self.game_mode.read().unwrap(),
                notgotitems,
                spawned,
            )
        };

        let mapchange = MapChange {
            map_name,
            mode,
            has_items: notgotitems,
        };

        // Sauerbraten requires N_ITEMLIST in the same ENet packet as N_MAPCHANGE.
        if let Some(items) = spawned_items {
            self.send_chan1(cn, (mapchange, ItemList { items }));
        } else {
            self.send_chan1(cn, mapchange);
        }

        self.send_current_master(cn);

        // Notify existing players of the new player.
        {
            let p = self.players.get(cn);
            self.broadcast_except_chan1(
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
            let p = &self.players;
            self.send_chan1(
                cn,
                &Resume {
                    clients: self
                        .client_cns()
                        .into_iter()
                        .filter(|&other| other != cn)
                        .filter_map(|other| {
                            let pd = p.get(other);
                            pd.state.clone().map(|player_state| ClientResumeState {
                                client_num: other as i32,
                                state: pd.client_state,
                                frags: pd.frags,
                                flags: 0, // TODO
                                deaths: pd.deaths,
                                quad_millis: 0, // TODO
                                player_state,
                            })
                        })
                        .collect(),
                },
            );
        }

        // Introduce all existing players to the new player and send their clipboards.
        for other_cn in self.client_cns().into_iter().filter(|&other| other != cn) {
            let pd = self.players.get(other_cn);
            let name = pd.name.clone();
            let player_model = pd.model;
            let team = pd.team.clone();
            let clipboard = pd.clipboard.as_ref().map(|cb| ClipboardServer {
                client_num: other_cn as i32,
                unpack_len: cb.unpack_len,
                data: cb.data.clone(),
            });
            drop(pd);

            self.send_chan1(
                cn,
                &InitClient {
                    client_num: other_cn as i32,
                    name,
                    player_model,
                    team,
                },
            );
            if let Some(cb) = clipboard {
                self.send_chan1(cn, &cb);
            }
        }

        self.broadcast_spawn_state(cn);
    }
}
