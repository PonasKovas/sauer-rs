use crate::Server;
use sauer_proto::{channel1::SpectatorServer, data::ClientState};

impl Server {
    /// Sets a player's spectator state and broadcasts `N_SPECTATOR` to all clients.
    /// Does nothing if the player is not found or already in the target state.
    pub fn set_spectator(&self, cn: u32, spectate: bool) {
        let changed = if let Some(mut p) = self.players.try_get_mut(cn) {
            if spectate {
                if p.client_state != ClientState::Spectator {
                    p.client_state = ClientState::Spectator;
                    true
                } else {
                    false
                }
            } else {
                if p.client_state == ClientState::Spectator {
                    p.client_state = ClientState::Dead;
                    true
                } else {
                    false
                }
            }
        } else {
            false
        };

        if changed {
            self.broadcast_chan1(&SpectatorServer {
                client_num: cn as i32,
                is_spectator: spectate,
            });
        }
    }
}
