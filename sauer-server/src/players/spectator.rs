use crate::Server;
use crate::Status;
use sauer_proto::channel1::SpectatorServer;

impl Server {
    /// Sets a player's spectator state and broadcasts `N_SPECTATOR` to all clients.
    /// Does nothing if the player is not found or already in the target state.
    pub fn set_spectator(&self, id: u64, spectate: bool) {
        let changed = self
            .players
            .with_mut(id, |p| {
                if spectate {
                    if p.status != Status::Spectator {
                        p.status = Status::Spectator;
                        true
                    } else {
                        false
                    }
                } else {
                    if p.status == Status::Spectator {
                        p.status = Status::Dead;
                        true
                    } else {
                        false
                    }
                }
            })
            .unwrap_or(false);

        if changed {
            if let Some(cn) = self.get_cn(id) {
                self.broadcast_chan1(&SpectatorServer {
                    client_num: cn,
                    is_spectator: spectate,
                });
            }
        }
    }
}
