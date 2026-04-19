use crate::{
    crypto,
    options::BasePluginOptions,
    registry::ClientRegistry,
};
use cube_string::CubeString;
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

// ── Pending auth challenge ────────────────────────────────────────────────────

struct PendingAuth {
    id: u32,
    name: CubeString,
    privilege: Privilege,
    /// Expected x-coordinate (24-byte big-endian) of `pubkey * scalar`.
    expected_x: [u8; 24],
}

// ── MasterState ───────────────────────────────────────────────────────────────

/// Server-wide master and authentication state, stored in [`Server::extra`].
pub struct MasterState {
    mastermode: Mutex<MasterMode>,
    /// Monotonically increasing counter for auth challenge IDs.
    authreq: AtomicU32,
    /// Pending auth challenges indexed by client cn.
    pending_auth: DashMap<u32, PendingAuth>,
}

impl MasterState {
    pub(crate) fn new() -> Self {
        Self {
            mastermode: Mutex::new(MasterMode::Open),
            authreq: AtomicU32::new(0),
            pending_auth: DashMap::new(),
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

    /// Remove any pending auth for a client (e.g. on disconnect).
    pub(crate) fn cancel_pending_auth(&self, cn: u32) {
        self.pending_auth.remove(&cn);
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
        let expected = crypto::hash_password(cn, session_id, &admin_pwd.to_string());
        if CubeString::from_str_lossy(&expected) == packet.hash {
            master.set_client_privilege(cn, Privilege::Admin, server, &registry);
            server.broadcast_chan1(&channel1::ServMsg {
                text: CubeString::from_str_lossy(&format!(
                    "{} claimed admin",
                    registry.get(cn).map(|c| c.name().to_string()).unwrap_or_default()
                )),
            });
            info!("cn {} claimed admin", cn);
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
        let kicker = registry.get(cn).map(|c| c.name().to_string()).unwrap_or_default();
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

    // No ban list yet; this will matter once ban tracking is added.
    server.broadcast_chan1(&channel1::ServMsg {
        text: CubeString::from_str_lossy("bans cleared"),
    });
    PASS
}

pub async fn on_auth_try(
    server: &Arc<Server>,
    cn: u32,
    packet: &AuthTry,
    options: &BasePluginOptions,
) -> Control {
    if server.extra.get::<ClientRegistry>().unwrap().get(cn).is_none() {
        return PASS;
    }

    // Look up the auth user matching name + desc.
    let Some(user) = options
        .auth_users
        .iter()
        .find(|u| u.name == packet.name && u.desc == packet.desc)
    else {
        // Unknown user — silently ignore (don't reveal which names are valid).
        return PASS;
    };

    // Generate ECC challenge: secret = G*scalar (sent to client), expected = pubkey*scalar.
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
    let registry = server.extra.get::<ClientRegistry>().unwrap();

    let Some(pending) = master.take_pending_auth(cn, packet.id) else {
        return PASS;
    };

    if crypto::verify_auth_answer(&packet.answer, &pending.expected_x) {
        master.set_client_privilege(cn, pending.privilege, server, &registry);
        server.broadcast_chan1(&channel1::ServMsg {
            text: CubeString::from_str_lossy(&format!(
                "{} authenticated",
                registry.get(cn).map(|c| c.name().to_string()).unwrap_or_default()
            )),
        });
        info!("cn {} authenticated as {}", cn, pending.name);
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
