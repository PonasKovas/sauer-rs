use cube_string::{CubeStr, CubeString, IntoCubeStringLossy};
use sauer_proto::{channel1::SetTeamServer, data::SetTeamReason};
use sauer_server::Server;

use crate::options::get_options;

use super::get_players;

pub(crate) fn set_initial_team(server: &Server, cn: u32) {
    let team = {
        let options = get_options(server).read().unwrap();
        if options.mode.has_teams() {
            let good = "good".lossy_cubestring();
            let evil = "evil".lossy_cubestring();
            if count_players_in(server, &good) <= count_players_in(server, &evil) {
                good
            } else {
                evil
            }
        } else {
            CubeString::new()
        }
    };

    if let Some(mut p) = get_players(server).try_get_mut(cn) {
        p.team = team;
    }
}

pub fn set_team(server: &Server, cn: u32, team: CubeString, reason: SetTeamReason) {
    let pkt = SetTeamServer {
        client_num: cn as i32,
        team,
        reason,
    };
    server.broadcast_chan1(&pkt);
    if let Some(mut p) = get_players(server).try_get_mut(cn) {
        p.team = pkt.team;
    }
}

pub fn count_players_in(server: &Server, team: &CubeStr) -> u32 {
    get_players(server)
        .map
        .iter()
        .fold(0, |acc, entry| if &entry.team == team { acc + 1 } else { acc })
}
