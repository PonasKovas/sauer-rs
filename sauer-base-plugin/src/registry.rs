use crate::client::ClientData;
use cube_string::CubeString;
use dashmap::DashMap;
use sauer_proto::{
    channel1,
    data::{ArmourType, ClientResumeState, ClientState, Gun, MasterClient, PlayerAmmo, PlayerModel, PlayerState, Privilege},
};
use sauer_server::Server;
use std::ops::{Deref, DerefMut};

/// Registry of all connected clients, stored in [`Server::extra`].
///
/// State mutations that must be visible to other clients go through methods here
/// so the data update and the broadcast always happen together.
pub struct ClientRegistry {
    /// Fully connected clients that have completed the N_CONNECT handshake.
    clients: DashMap<u32, ClientData>,
    /// Session IDs for clients that have physically connected but not yet sent N_CONNECT.
    pending: DashMap<u32, i32>,
    /// Clients that have sent N_CONNECT on a Private server and are awaiting ECC auth.
    connect_pending: DashMap<u32, ClientData>,
}

// ── Construction ──────────────────────────────────────────────────────────────

impl ClientRegistry {
    pub(crate) fn new() -> Self {
        Self {
            clients: DashMap::new(),
            pending: DashMap::new(),
            connect_pending: DashMap::new(),
        }
    }
}

// ── Internal lifecycle (pub(crate)) ───────────────────────────────────────────

impl ClientRegistry {
    /// Record a newly connected peer before N_CONNECT is received.
    pub(crate) fn begin_pending(&self, cn: u32, session_id: i32) {
        self.pending.insert(cn, session_id);
    }

    /// Return the session ID for a pending (pre-N_CONNECT) client.
    pub(crate) fn pending_session_id(&self, cn: u32) -> Option<i32> {
        self.pending.get(&cn).map(|v| *v)
    }

    /// Promote a pending connection to a full client after N_CONNECT.
    /// Returns `false` if `cn` was not in the pending map.
    pub(crate) fn complete_pending(&self, cn: u32, data: ClientData) -> bool {
        let Some((_, session_id)) = self.pending.remove(&cn) else {
            return false;
        };
        self.clients.insert(cn, data.with_session_id(session_id));
        true
    }

    /// Park a post-N_CONNECT client that is awaiting Private-mode ECC auth.
    /// The session ID stays in `pending` until `complete_connect_pending` is called.
    pub(crate) fn begin_connect_pending(&self, cn: u32, data: ClientData) {
        self.connect_pending.insert(cn, data);
    }

    /// Admit a connect-pending client: move from connect_pending → clients.
    /// Returns `false` if either map is missing the cn.
    pub(crate) fn complete_connect_pending(&self, cn: u32) -> bool {
        let Some((_, client)) = self.connect_pending.remove(&cn) else {
            return false;
        };
        let Some((_, session_id)) = self.pending.remove(&cn) else {
            return false;
        };
        self.clients.insert(cn, client.with_session_id(session_id));
        true
    }

    /// Remove a client from all lifecycle maps.
    /// Returns `true` if the client had completed the handshake (was in `clients`).
    pub(crate) fn remove(&self, cn: u32) -> bool {
        self.pending.remove(&cn);
        self.connect_pending.remove(&cn);
        self.clients.remove(&cn).is_some()
    }

    /// Update a client's privilege without broadcasting.
    /// Always follow up with N_CURRENTMASTER via [`MasterState::set_client_privilege`].
    pub(crate) fn set_privilege_data(&self, cn: u32, privilege: Privilege) {
        if let Some(mut c) = self.clients.get_mut(&cn) {
            c.privilege = privilege;
        }
    }

    /// Collect `N_RESUME` state for all clients except `exclude_cn`.
    pub(crate) fn resume_states(&self, exclude_cn: u32) -> Vec<ClientResumeState> {
        self.clients
            .iter()
            .filter(|e| *e.key() != exclude_cn)
            .map(|e| {
                let c = e.value();
                let player_state = if c.state == ClientState::Alive {
                    PlayerState {
                        life_sequence: c.life_sequence,
                        health: c.health,
                        max_health: c.max_health,
                        armour: c.armour,
                        armour_type: c.armour_type,
                        gun_select: c.gun_select,
                        ammo: c.ammo.clone(),
                    }
                } else {
                    dead_player_state()
                };
                ClientResumeState {
                    client_num: *e.key() as i32,
                    state: c.state,
                    frags: c.frags,
                    flags: 0,
                    deaths: c.deaths,
                    quad_millis: 0,
                    player_state,
                }
            })
            .collect()
    }

    /// Record the ping reported by a client via N_CLIENTPING.
    pub(crate) fn record_ping(&self, cn: u32, ping: i32) {
        if let Some(mut c) = self.clients.get_mut(&cn) {
            c.ping = ping;
        }
    }
}

// ── Public read API ───────────────────────────────────────────────────────────

impl ClientRegistry {
    pub fn get(&self, cn: u32) -> Option<impl Deref<Target = ClientData> + '_> {
        self.clients.get(&cn)
    }

    pub(crate) fn get_mut(&self, cn: u32) -> Option<impl DerefMut<Target = ClientData> + '_> {
        self.clients.get_mut(&cn)
    }

    pub fn client_count(&self) -> usize {
        self.clients.len()
    }

    pub fn all_cns(&self) -> Vec<u32> {
        self.clients.iter().map(|e| *e.key()).collect()
    }

    /// All clients with privilege above None, formatted for N_CURRENTMASTER.
    pub fn privileged_clients(&self) -> Vec<MasterClient> {
        self.clients
            .iter()
            .filter(|e| e.value().privilege != Privilege::None)
            .map(|e| MasterClient {
                client_num: *e.key() as i32,
                privilege: e.value().privilege,
            })
            .collect()
    }
}

// ── Public mutation API ───────────────────────────────────────────────────────
//
// Each method updates the stored data AND broadcasts the corresponding packet
// to all connected clients so the requester also gets the confirmation.

impl ClientRegistry {
    /// Update a client's display name and broadcast N_SWITCHNAME to all clients.
    pub fn set_name(&self, cn: u32, name: CubeString, server: &Server) {
        if let Some(mut c) = self.clients.get_mut(&cn) {
            c.name = name.clone();
        }
        server.broadcast_chan1(&channel1::Client {
            client_num: cn as i32,
            packet: channel1::SwitchName { name },
        });
    }

    /// Update a client's player model and broadcast N_SWITCHMODEL to all clients.
    pub fn set_model(&self, cn: u32, model: PlayerModel, server: &Server) {
        if let Some(mut c) = self.clients.get_mut(&cn) {
            c.player_model = model;
        }
        server.broadcast_chan1(&channel1::Client {
            client_num: cn as i32,
            packet: channel1::SwitchModel { model },
        });
    }

    /// Update a client's team and broadcast N_SETTEAM to all clients.
    pub fn set_team(&self, cn: u32, team: CubeString, server: &Server) {
        if let Some(mut c) = self.clients.get_mut(&cn) {
            c.team = team.clone();
        }
        server.broadcast_chan1(&channel1::SetTeamServer {
            client_num: cn as i32,
            team,
            reason: 0,
        });
    }

    /// Set a client's spectator state and broadcast N_SPECTATOR to all clients.
    pub fn set_spectator(&self, cn: u32, is_spectator: bool, server: &Server) {
        if let Some(mut c) = self.clients.get_mut(&cn) {
            c.state = if is_spectator { ClientState::Spectator } else { ClientState::Dead };
        }
        server.broadcast_chan1(&channel1::SpectatorServer {
            client_num: cn as i32,
            is_spectator,
        });
    }
}

// ── Helpers ───────────────────────────────────────────────────────────────────

fn dead_player_state() -> PlayerState {
    PlayerState {
        life_sequence: 0,
        health: 0,
        max_health: 100,
        armour: 0,
        armour_type: ArmourType::Blue,
        gun_select: Gun::Fist,
        ammo: PlayerAmmo {
            shotgun: 0,
            chaingun: 0,
            rocket_launcher: 0,
            rifle: 0,
            grenade_launcher: 0,
            pistol: 0,
        },
    }
}
