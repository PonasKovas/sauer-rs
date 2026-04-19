use cube_string::CubeString;
use sauer_proto::data::{ArmourType, ClientState, Gun, PlayerAmmo, PlayerModel, Privilege};

/// Per-client state managed by the base plugin.
///
/// All mutations that require broadcasting (name, team, spectator, etc.) go
/// through [`ClientRegistry`](crate::ClientRegistry) methods. Combat-specific
/// fields are mutated directly in the combat module under `pub(crate)` access.
pub struct ClientData {
    // ── Identity & session ────────────────────────────────────────────────────
    pub(crate) name: CubeString,
    pub(crate) team: CubeString,
    pub(crate) player_model: PlayerModel,
    pub(crate) state: ClientState,
    pub(crate) privilege: Privilege,
    /// Randomly generated at connect time; used as a cryptographic salt.
    session_id: i32,
    pub(crate) ping: i32,

    // ── Combat ────────────────────────────────────────────────────────────────
    /// Incremented each time the client spawns. Used to discard stale packets
    /// (e.g. a rocket that lands after the shooter has already respawned).
    pub(crate) life_sequence: i32,
    pub(crate) health: i32,
    pub(crate) max_health: i32,
    pub(crate) armour: i32,
    pub(crate) armour_type: ArmourType,
    pub(crate) ammo: PlayerAmmo,
    pub(crate) gun_select: Gun,
    pub(crate) frags: i32,
    pub(crate) deaths: i32,
    /// gamemillis of the last weapon discharge — used to enforce fire-rate limits.
    pub(crate) last_shot_ms: u64,
    /// gamemillis when the client last died, or None if alive / never died yet.
    pub(crate) death_ms: Option<u64>,
}

impl ClientData {
    pub(crate) fn new(name: CubeString, team: CubeString, player_model: PlayerModel) -> Self {
        Self {
            name,
            team,
            player_model,
            state: ClientState::Dead,
            privilege: Privilege::None,
            session_id: 0,
            ping: 0,
            life_sequence: 0,
            health: 0,
            max_health: 100,
            armour: 0,
            armour_type: ArmourType::Blue,
            ammo: PlayerAmmo {
                shotgun: 0,
                chaingun: 0,
                rocket_launcher: 0,
                rifle: 0,
                grenade_launcher: 1,
                pistol: 30,
            },
            gun_select: Gun::Fist,
            frags: 0,
            deaths: 0,
            last_shot_ms: 0,
            death_ms: None,
        }
    }

    pub(crate) fn with_session_id(mut self, session_id: i32) -> Self {
        self.session_id = session_id;
        self
    }

    // ── Public read API ───────────────────────────────────────────────────────

    pub fn name(&self) -> &CubeString {
        &self.name
    }

    pub fn team(&self) -> &CubeString {
        &self.team
    }

    pub fn player_model(&self) -> PlayerModel {
        self.player_model
    }

    pub fn state(&self) -> ClientState {
        self.state
    }

    pub fn privilege(&self) -> Privilege {
        self.privilege
    }

    pub fn session_id(&self) -> i32 {
        self.session_id
    }

    pub fn ping(&self) -> i32 {
        self.ping
    }

    pub fn life_sequence(&self) -> i32 {
        self.life_sequence
    }

    pub fn health(&self) -> i32 {
        self.health
    }

    pub fn armour(&self) -> i32 {
        self.armour
    }

    pub fn frags(&self) -> i32 {
        self.frags
    }

    pub fn deaths(&self) -> i32 {
        self.deaths
    }
}
