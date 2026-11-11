use crate::Server;
use cube_string::{CubeStr, CubeString, IntoCubeStringLossy};
use sauer_proto::{channel1::SetTeamServer, data::SetTeamReason};

impl Server {
    /// Assigns the initial team for a newly welcomed player. Only call from `welcome()`.
    pub(crate) fn set_initial_team(&self, id: u64) {
        let team = {
            let mode = *self.game_mode.read();
            if mode.has_teams() {
                let good = "good".lossy_cubestring();
                let evil = "evil".lossy_cubestring();
                if self.count_players_in(&good) <= self.count_players_in(&evil) {
                    good
                } else {
                    evil
                }
            } else {
                CubeString::new()
            }
        };
        self.players.with_mut(id, |p| p.team = team);
    }

    /// Sets a player's team and broadcasts N_SETTEAM to all clients.
    pub fn set_team(&self, id: u64, team: CubeString, reason: SetTeamReason) {
        if let Some(cn) = self.get_cn(id) {
            let pkt = SetTeamServer {
                client_num: cn as i32,
                team,
                reason,
            };
            self.broadcast_chan1(&pkt);
            self.players.with_mut(id, |p| p.team = pkt.team);
        }
    }

    pub fn count_players_in(&self, team: &CubeStr) -> u32 {
        self.players.count(|_, p| &p.team == team) as u32
    }
}
