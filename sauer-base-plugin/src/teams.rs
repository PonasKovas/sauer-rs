use cube_string::{CubeStr, CubeString, IntoCubeStringLossy};
use dashmap::{DashMap, Entry};
use sauer_proto::{channel1::SetTeamServer, data::SetTeamReason};
use sauer_server::Server;
use std::ops::Deref;

use crate::options::get_options;

#[derive(Clone, Debug)]
pub struct Teams {
    map: DashMap<u32, CubeString>,
}

impl Teams {
    pub(crate) fn new() -> Self {
        Self {
            map: DashMap::new(),
        }
    }

    pub fn try_get(&self, cn: u32) -> Option<impl Deref<Target = CubeString>> {
        self.map.get(&cn)
    }
    pub fn get(&self, cn: u32) -> impl Deref<Target = CubeString> {
        match self.try_get(cn) {
            Some(x) => x,
            None => panic!("cn {cn} not in teams list"),
        }
    }
    /// Sets the tean of the player in local data. Use [`set_team`] to automatically send `N_SETTEAM` packet to all clients
    pub fn raw_set(&self, cn: u32, team: CubeString) {
        // only set if the entry is there at all
        if let Entry::Occupied(mut entry) = self.map.entry(cn) {
            entry.insert(team);
        }
    }
}

impl Default for Teams {
    fn default() -> Self {
        Self::new()
    }
}

/// Does NOT broadcast the team change to clients, only sets internal data
pub(crate) fn set_initial_team(server: &Server, cn: u32) {
    let options = get_options(server).read().unwrap();

    let team = if options.mode.has_teams() {
        let good = "good".lossy_cubestring();
        let evil = "evil".lossy_cubestring();

        if count_players_in(server, &good) <= count_players_in(server, &evil) {
            good
        } else {
            evil
        }
    } else {
        CubeString::new()
    };

    get_teams(server).raw_set(cn, team);
}

pub(crate) fn clear_team(server: &Server, cn: u32) {
    get_teams(server).map.remove(&cn);
}

pub fn get_teams(server: &Server) -> &Teams {
    match server.extra.get() {
        Some(x) => x,
        None => panic!("Teams not initialized"),
    }
}

pub fn set_team(server: &Server, cn: u32, team: CubeString, reason: SetTeamReason) {
    let pkt = SetTeamServer {
        client_num: cn as i32,
        team,
        reason,
    };

    server.broadcast_chan1(&pkt);

    get_teams(server).raw_set(cn, pkt.team);
}

pub fn count_players_in(server: &Server, team: &CubeStr) -> u32 {
    get_teams(server)
        .map
        .iter()
        .fold(0, |acc, t| if &*t == team { acc + 1 } else { acc })
}
