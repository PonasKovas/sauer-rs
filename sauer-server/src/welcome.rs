use crate::{Server, Status};
use sauer_proto::{
    channel1::{ClipboardServer, InitClient, ItemList, MapChange, Resume, Welcome},
    data::ClientResumeState,
};

impl Server {
    /// Sends the full join handshake to `id`: N_WELCOME, N_MAPCHANGE (+N_ITEMLIST), N_CURRENTMASTER,
    /// N_INITCLIENT to existing players, N_RESUME, N_INITCLIENT for each existing player,
    /// their clipboards, and N_SPAWNSTATE broadcast.
    ///
    /// For `MasterMode::Private`, a plugin must perform auth and then call this manually.
    pub fn welcome(&self, id: u64) {
        self.set_initial_team(id);
        self.set_initial_state(id);
        // From here on, broadcasts reach this client too.
        if let Some(peer) = self.clients.get(&id) {
            peer.set_joined();
        }

        self.send(id, &Welcome {});

        let (map_name, mode, notgotitems, spawned_items) = {
            let game = self.game.read();
            let notgotitems = game.items.is_awaiting_list();
            let spawned = (!notgotitems).then(|| game.items.spawned_items());
            (game.map_name.clone(), game.mode, notgotitems, spawned)
        };

        let mapchange = MapChange {
            map_name,
            mode,
            has_items: notgotitems,
        };

        // Sauerbraten requires N_ITEMLIST in the same ENet packet as N_MAPCHANGE.
        if let Some(items) = spawned_items {
            self.send(id, (mapchange, ItemList { items }));
        } else {
            self.send(id, mapchange);
        }

        self.send_current_master(id);

        // Notify existing players of the new player.
        if let Some(my_cn) = self.get_cn(id)
            && let Some(init) = self.players.with(id, |p| InitClient {
                client_num: my_cn,
                name: p.name.clone(),
                player_model: p.model,
                team: p.team.clone(),
            })
        {
            self.broadcast_except(id, &init);
        }

        // Send the new player a resume snapshot of all existing players.
        let clients = self
            .joined_ids()
            .into_iter()
            .filter(|&other| other != id)
            .filter_map(|other| {
                let other_cn = self.get_cn(other)?;
                self.players
                    .with(other, |pd| {
                        pd.state.clone().map(|player_state| ClientResumeState {
                            client_num: other_cn,
                            state: pd.status.client_state(),
                            frags: pd.frags,
                            flags: pd.flags,
                            deaths: pd.deaths,
                            quad_millis: pd.quad_millis,
                            player_state,
                        })
                    })
                    .flatten()
            })
            .collect();
        self.send(id, &Resume { clients });

        for (cn, fake) in &self.fake_players {
            self.send(
                id,
                (
                    InitClient {
                        client_num: *cn,
                        name: fake.name.clone(),
                        player_model: fake.model,
                        team: fake.team.clone(),
                    },
                    sauer_proto::channel1::SpectatorServer {
                        client_num: *cn,
                        is_spectator: true,
                    },
                ),
            );
        }

        // Introduce all existing players to the new player and send their clipboards.
        for other_id in self.joined_ids().into_iter().filter(|&other| other != id) {
            let Some(other_cn) = self.get_cn(other_id) else {
                continue;
            };
            let Some((init, clipboard)) = self.players.with(other_id, |pd| {
                let init = InitClient {
                    client_num: other_cn,
                    name: pd.name.clone(),
                    player_model: pd.model,
                    team: pd.team.clone(),
                };
                let clipboard = pd.clipboard.as_ref().map(|cb| ClipboardServer {
                    client_num: other_cn,
                    payload: Some(cb.clone()),
                });
                (init, clipboard)
            }) else {
                continue;
            };

            self.send(id, &init);
            if let Some(cb) = clipboard {
                self.send(id, &cb);
            }
        }

        let is_spectator = self
            .players
            .with(id, |p| p.status == Status::Spectator)
            .unwrap_or(false);

        if is_spectator {
            if let Some(my_cn) = self.get_cn(id) {
                self.broadcast(&sauer_proto::channel1::SpectatorServer {
                    client_num: my_cn,
                    is_spectator: true,
                });
            }
        } else {
            self.broadcast_spawn_state(id);
        }
    }
}
