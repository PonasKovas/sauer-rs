use crate::{
    client::ClientData,
    crypto,
    options::BasePluginOptions,
    registry::ClientRegistry,
};
use cube_string::{CubeStr, CubeString};
use dashmap::DashMap;
use sauer_proto::{
    channel1::{self, AuthAnswer, AuthTry, ClearBans, Kick, MasterModeClient, SetMaster},
    data::{MasterMode, Privilege},
};
use sauer_server::{
    Server,
    plugin::{Control, PASS},
};
use std::sync::{
    Arc,
    Mutex,
    atomic::{AtomicU32, Ordering},
};
use tracing::info;

// ── Pending auth challenges ───────────────────────────────────────────────────

/// Pending challenge for a privilege auth (N_AUTHTRY / N_AUTHANS).
struct PendingAuth {
    id: u32,
    name: CubeString,
    privilege: Privilege,
    expected_x: [u8; 24],
}

/// Pending challenge for a connect auth (Private mastermode gate).
/// The client's [`ClientData`] is parked in `registry.connect_pending`.
struct ConnectPendingAuth {
    id: u32,
    expected_x: [u8; 24],
}

// ── MasterState ───────────────────────────────────────────────────────────────

/// Server-wide master and authentication state, stored in [`Server::extra`].
pub struct MasterState {
    mastermode: Mutex<MasterMode>,
    /// Monotonically increasing counter for auth challenge IDs.
    authreq: AtomicU32,
    /// Pending privilege auth challenges, keyed by client cn.
    pending_auth: DashMap<u32, PendingAuth>,
    /// Pending connect auth challenges (Private mastermode gate), keyed by client cn.
    connect_pending_auth: DashMap<u32, ConnectPendingAuth>,
}

impl MasterState {
    pub(crate) fn new() -> Self {
        Self {
            mastermode: Mutex::new(MasterMode::Open),
            authreq: AtomicU32::new(0),
            pending_auth: DashMap::new(),
            connect_pending_auth: DashMap::new(),
        }
    }

    pub fn mastermode(&self) -> MasterMode {
        *self.mastermode.lock().unwrap()
    }

    fn next_authreq(&self) -> u32 {
        self.authreq.fetch_add(1, Ordering::Relaxed)
    }

    fn begin_auth(
        &self,
        cn: u32,
        name: CubeString,
        privilege: Privilege,
        expected_x: [u8; 24],
    ) -> u32 {
        let id = self.next_authreq();
        self.pending_auth.insert(cn, PendingAuth { id, name, privilege, expected_x });
        id
    }

    fn take_pending_auth(&self, cn: u32, id: u32) -> Option<PendingAuth> {
        let (_, pending) = self.pending_auth.remove(&cn)?;
        (pending.id == id).then_some(pending)
    }

    fn begin_connect_auth(&self, cn: u32, expected_x: [u8; 24]) -> u32 {
        let id = self.next_authreq();
        self.connect_pending_auth.insert(cn, ConnectPendingAuth { id, expected_x });
        id
    }

    fn take_connect_pending_auth(&self, cn: u32, id: u32) -> Option<ConnectPendingAuth> {
        let (_, pending) = self.connect_pending_auth.remove(&cn)?;
        (pending.id == id).then_some(pending)
    }

    /// Remove all pending auth state for a client (called on disconnect).
    pub(crate) fn cancel_pending_auth(&self, cn: u32) {
        self.pending_auth.remove(&cn);
        self.connect_pending_auth.remove(&cn);
    }

    // ── Invariant-enforcing setters ───────────────────────────────────────

    /// Update the server mastermode and broadcast N_CURRENTMASTER to all clients.
    pub fn set_mastermode(&self, mode: MasterMode, server: &Server, registry: &ClientRegistry) {
        *self.mastermode.lock().unwrap() = mode;
        broadcast_currentmaster(server, registry, self);
    }

    /// Update a client's privilege and broadcast N_CURRENTMASTER to all clients.
    pub fn set_client_privilege(
        &self,
        cn: u32,
        privilege: Privilege,
        server: &Server,
        registry: &ClientRegistry,
    ) {
        registry.set_privilege_data(cn, privilege);
        broadcast_currentmaster(server, registry, self);
    }
}

// ── Helpers ───────────────────────────────────────────────────────────────────

fn broadcast_currentmaster(server: &Server, registry: &ClientRegistry, master: &MasterState) {
    server.broadcast_chan1(&channel1::CurrentMaster {
        master_mode: master.mastermode(),
        clients: registry.privileged_clients(),
    });
}

// ── Handlers ──────────────────────────────────────────────────────────────────

pub async fn on_set_master(
    server: &Arc<Server>,
    cn: u32,
    packet: &SetMaster,
    options: &BasePluginOptions,
) -> Control {
    let registry = server.extra.get::<ClientRegistry>().unwrap();
    let master = server.extra.get::<MasterState>().unwrap();

    if !packet.val {
        master.set_client_privilege(cn, Privilege::None, server, &registry);
        server.broadcast_chan1(&channel1::ServMsg {
            text: CubeString::from_str_lossy("master relinquished"),
        });
        info!("cn {} released master", cn);
        return PASS;
    }

    let session_id = match registry.get(cn) {
        Some(c) => c.session_id(),
        None => return PASS,
    };

    if let Some(admin_pwd) = &options.admin_password {
        let expected = crypto::hash_password(cn, session_id, admin_pwd);
        if expected == packet.hash {
            let name = registry.get(cn).map(|c| c.name().to_cube_string()).unwrap_or_default();
            master.set_client_privilege(cn, Privilege::Admin, server, &registry);
            server.broadcast_chan1(&channel1::ServMsg {
                text: CubeString::from_str_lossy(&format!("{} claimed admin", name)),
            });
            info!("cn {} ({}) claimed admin", cn, name);
            return PASS;
        }
    }

    server.send_chan1(
        cn,
        &channel1::ServMsg {
            text: CubeString::from_str_lossy("authentication failed"),
        },
    );
    PASS
}

pub async fn on_master_mode(
    server: &Arc<Server>,
    cn: u32,
    packet: &MasterModeClient,
) -> Control {
    let registry = server.extra.get::<ClientRegistry>().unwrap();
    let master = server.extra.get::<MasterState>().unwrap();

    let privilege = registry.get(cn).map(|c| c.privilege()).unwrap_or(Privilege::None);
    if privilege < Privilege::Master {
        return PASS;
    }

    master.set_mastermode(packet.mode, server, &registry);
    info!("cn {} set mastermode to {:?}", cn, packet.mode);
    PASS
}

pub async fn on_kick(server: &Arc<Server>, cn: u32, packet: &Kick) -> Control {
    let registry = server.extra.get::<ClientRegistry>().unwrap();

    let privilege = registry.get(cn).map(|c| c.privilege()).unwrap_or(Privilege::None);
    if privilege < Privilege::Master {
        return PASS;
    }

    let victim = packet.victim_cn as u32;
    if registry.get(victim).is_some() {
        let kicker = registry.get(cn).map(|c| c.name().to_cube_string()).unwrap_or_default();
        info!("cn {} ({}) kicked cn {} (reason: {})", cn, kicker, victim, packet.reason);
        server.disconnect(victim);
    }

    PASS
}

pub async fn on_clear_bans(server: &Arc<Server>, cn: u32, _packet: &ClearBans) -> Control {
    let registry = server.extra.get::<ClientRegistry>().unwrap();

    let privilege = registry.get(cn).map(|c| c.privilege()).unwrap_or(Privilege::None);
    if privilege < Privilege::Master {
        return PASS;
    }

    server.broadcast_chan1(&channel1::ServMsg {
        text: CubeString::from_str_lossy("bans cleared"),
    });
    PASS
}

/// N_AUTHTRY: already-connected client requesting privilege elevation.
/// This is NOT used for the Private mastermode connect gate.
pub async fn on_auth_try(
    server: &Arc<Server>,
    cn: u32,
    packet: &AuthTry,
    options: &BasePluginOptions,
) -> Control {
    // Only handle fully connected clients.
    let registry = server.extra.get::<ClientRegistry>().unwrap();
    if registry.get(cn).is_none() {
        return PASS;
    }

    let Some(user) = options
        .auth_users
        .iter()
        .find(|u| u.name == packet.name && u.desc == packet.desc)
    else {
        return PASS;
    };

    let Some((challenge_str, expected_x)) = crypto::gen_auth_challenge(&user.pubkey) else {
        return PASS;
    };

    let master = server.extra.get::<MasterState>().unwrap();
    let id = master.begin_auth(cn, user.name.clone(), user.privilege, expected_x);

    server.send_chan1(
        cn,
        &channel1::AuthChal {
            desc: packet.desc.clone(),
            id,
            challenge: challenge_str,
        },
    );

    PASS
}

pub async fn on_auth_ans(server: &Arc<Server>, cn: u32, packet: &AuthAnswer) -> Control {
    let master = server.extra.get::<MasterState>().unwrap();

    // Connect auth (Private mastermode gate) takes priority.
    if let Some(pending) = master.take_connect_pending_auth(cn, packet.id) {
        if crypto::verify_auth_answer(&packet.answer, &pending.expected_x) {
            let registry = server.extra.get::<ClientRegistry>().unwrap();
            let mastermode = master.mastermode();
            if registry.complete_connect_pending(cn) {
                let name = registry.get(cn).map(|c| c.name().to_cube_string()).unwrap_or_default();
                crate::connect::complete_connection(server, cn, &registry, mastermode).await;
                info!("cn {} ({}) joined via auth", cn, name);
            }
        } else {
            server.send_chan1(
                cn,
                &channel1::ServMsg {
                    text: CubeString::from_str_lossy("authentication failed"),
                },
            );
            server.disconnect(cn);
        }
        return PASS;
    }

    // Privilege auth for an already-connected client.
    let registry = server.extra.get::<ClientRegistry>().unwrap();
    let Some(pending) = master.take_pending_auth(cn, packet.id) else {
        return PASS;
    };

    if crypto::verify_auth_answer(&packet.answer, &pending.expected_x) {
        master.set_client_privilege(cn, pending.privilege, server, &registry);
        let name = registry.get(cn).map(|c| c.name().to_cube_string()).unwrap_or_default();
        server.broadcast_chan1(&channel1::ServMsg {
            text: CubeString::from_str_lossy(&format!("{} authenticated", name)),
        });
        info!("cn {} ({}) authenticated as {}", cn, name, pending.name);
    } else {
        server.send_chan1(
            cn,
            &channel1::ServMsg {
                text: CubeString::from_str_lossy("authentication failed"),
            },
        );
    }

    PASS
}

/// Called from `on_connect` when mastermode is Private.
///
/// Validates the auth credentials from the N_CONNECT packet against known users
/// whose desc matches `server_auth`, generates a challenge, parks the client in
/// connect-pending state, and sends N_AUTHCHAL.
///
/// Returns `false` if no matching auth user was found (caller should disconnect).
pub(crate) async fn try_connect_auth(
    server: &Arc<Server>,
    cn: u32,
    client: ClientData,
    auth_desc: &CubeStr,
    auth_name: &CubeStr,
    options: &BasePluginOptions,
) -> bool {
    // The auth_desc must match the server's own auth domain.
    if auth_desc != &*options.server_auth {
        return false;
    }

    let Some(user) = options
        .auth_users
        .iter()
        .find(|u| u.name == auth_name && u.desc == auth_desc)
    else {
        return false;
    };

    let Some((challenge_str, expected_x)) = crypto::gen_auth_challenge(&user.pubkey) else {
        return false;
    };

    let registry = server.extra.get::<ClientRegistry>().unwrap();
    registry.begin_connect_pending(cn, client);

    let master = server.extra.get::<MasterState>().unwrap();
    let id = master.begin_connect_auth(cn, expected_x);

    server.send_chan1(
        cn,
        &channel1::AuthChal {
            desc: user.desc.clone(),
            id,
            challenge: challenge_str,
        },
    );

    true
}
