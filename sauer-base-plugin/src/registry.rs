use crate::client::ClientData;
use cube_string::CubeString;
use dashmap::DashMap;
use sauer_proto::{
    channel1,
    data::{
        ArmourType, ClientResumeState, ClientState, Gun, MasterClient, PlayerAmmo, PlayerModel,
        PlayerState, Privilege,
    },
};
use sauer_server::Server;
use std::ops::Deref;

/// Registry of all connected clients, stored in [`Server::extra`].
///
/// All state mutations go through methods here so invariants can be enforced.
/// For example, [`set_name`] both updates the stored name **and** broadcasts
/// `N_SWITCHNAME` to all other clients — these two actions must stay in sync.
pub struct ClientRegistry {
    /// Fully connected clients that have completed the N_CONNECT handshake.
    clients: DashMap<u32, ClientData>,
    /// Session IDs for clients that have physically connected but not yet
    /// sent N_CONNECT. Stored here so the id is available when N_CONNECT arrives.
    pending: DashMap<u32, i32>,
}

impl ClientRegistry {
    pub(crate) fn new() -> Self {
        Self {
            clients: DashMap::new(),
            pending: DashMap::new(),
        }
    }

    // ── Internal lifecycle ──────────────────────────────────────────────

    /// Record a newly connected peer before N_CONNECT is received.
    pub(crate) fn begin_pending(&self, cn: u32, session_id: i32) {
        self.pending.insert(cn, session_id);
    }

    /// Return the session ID for a pending (pre-N_CONNECT) client.
    pub(crate) fn pending_session_id(&self, cn: u32) -> Option<i32> {
        self.pending.get(&cn).map(|v| *v)
    }

    /// Promote a pending connection to a full client entry after N_CONNECT.
    /// Returns `false` if `cn` was not in the pending map.
    pub(crate) fn complete_pending(&self, cn: u32, data: ClientData) -> bool {
        let Some((_, session_id)) = self.pending.remove(&cn) else {
            return false;
        };
        self.clients.insert(cn, data.with_session_id(session_id));
        true
    }

    /// Remove a client entirely (both pending and connected state).
    /// Returns whether the client had completed the handshake.
    pub(crate) fn remove(&self, cn: u32) -> bool {
        self.pending.remove(&cn);
        self.clients.remove(&cn).is_some()
    }

    // ── Read access ─────────────────────────────────────────────────────

    pub fn get(&self, cn: u32) -> Option<impl Deref<Target = ClientData> + '_> {
        self.clients.get(&cn)
    }

    pub fn client_count(&self) -> usize {
        self.clients.len()
    }

    pub fn all_cns(&self) -> Vec<u32> {
        self.clients.iter().map(|e| *e.key()).collect()
    }

    /// Collect `N_RESUME` state for all clients except `exclude_cn`.
    /// Called during the connection handshake to inform the joining client.
    pub(crate) fn resume_states(&self, exclude_cn: u32) -> Vec<ClientResumeState> {
        self.clients
            .iter()
            .filter(|e| *e.key() != exclude_cn)
            .map(|e| ClientResumeState {
                client_num: *e.key() as i32,
                state: e.value().state,
                frags: 0,
                flags: 0,
                deaths: 0,
                quad_millis: 0,
                player_state: dead_player_state(),
            })
            .collect()
    }

    // ── Privilege ───────────────────────────────────────────────────────

    /// Update the stored privilege for a client.
    /// Does **not** broadcast — callers must follow up with N_CURRENTMASTER.
    /// Use [`MasterState::set_client_privilege`] for the full invariant-safe version.
    pub(crate) fn set_privilege_data(&self, cn: u32, privilege: Privilege) {
        if let Some(mut c) = self.clients.get_mut(&cn) {
            c.privilege = privilege;
        }
    }

    /// All clients whose privilege is above None, formatted for N_CURRENTMASTER.
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

    // ── Invariant-enforcing setters ─────────────────────────────────────

    /// Update a client's display name and broadcast `N_SWITCHNAME` to all
    /// other clients. The requesting client already updated locally.
    pub fn set_name(&self, cn: u32, name: CubeString, server: &Server) {
        if let Some(mut c) = self.clients.get_mut(&cn) {
            c.name = name.clone();
        }
        server.broadcast_except_chan1(
            cn,
            &channel1::Client {
                client_num: cn as i32,
                packet: channel1::SwitchName { name },
            },
        );
    }

    /// Update a client's player model and broadcast `N_SWITCHMODEL` to all
    /// other clients.
    pub fn set_model(&self, cn: u32, model: PlayerModel, server: &Server) {
        if let Some(mut c) = self.clients.get_mut(&cn) {
            c.player_model = model;
        }
        server.broadcast_except_chan1(
            cn,
            &channel1::Client {
                client_num: cn as i32,
                packet: channel1::SwitchModel { model },
            },
        );
    }

    /// Update a client's team and broadcast `N_SETTEAM` to all other clients.
    pub fn set_team(&self, cn: u32, team: CubeString, server: &Server) {
        if let Some(mut c) = self.clients.get_mut(&cn) {
            c.team = team.clone();
        }
        server.broadcast_except_chan1(
            cn,
            &channel1::SetTeamServer {
                client_num: cn as i32,
                team,
                reason: 0,
            },
        );
    }

    /// Set whether a client is spectating and broadcast `N_SPECTATOR` to all
    /// clients (including the one whose state changed, so they know it was accepted).
    pub fn set_spectator(&self, cn: u32, is_spectator: bool, server: &Server) {
        if let Some(mut c) = self.clients.get_mut(&cn) {
            c.state = if is_spectator {
                ClientState::Spectator
            } else {
                ClientState::Dead
            };
        }
        server.broadcast_chan1(&channel1::SpectatorServer {
            client_num: cn as i32,
            is_spectator,
        });
    }

    /// Record the ping value reported by a client via `N_CLIENTPING`.
    /// Not exposed publicly — ping updates go through the handler pipeline.
    pub(crate) fn record_ping(&self, cn: u32, ping: i32) {
        if let Some(mut c) = self.clients.get_mut(&cn) {
            c.ping = ping;
        }
    }
}

/// Baseline player state for a client that has not yet spawned.
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
