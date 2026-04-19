use cube_string::CubeString;
use sauer_proto::data::{ClientState, PlayerModel, Privilege};

/// Per-client state managed by the base plugin.
///
/// All mutations go through [`ClientRegistry`](crate::ClientRegistry) methods
/// which enforce protocol invariants — e.g. a name change always broadcasts
/// `N_SWITCHNAME` to the other clients.
pub struct ClientData {
    pub(crate) name: CubeString,
    pub(crate) team: CubeString,
    pub(crate) player_model: PlayerModel,
    pub(crate) state: ClientState,
    pub(crate) privilege: Privilege,
    /// Randomly generated at connect time; used as a cryptographic salt.
    /// Never modified after creation.
    session_id: i32,
    /// Last ping value reported by the client via N_CLIENTPING.
    /// Read-only from outside the plugin; updated only through the registry.
    pub(crate) ping: i32,
}

impl ClientData {
    pub(crate) fn new(name: CubeString, team: CubeString, player_model: PlayerModel) -> Self {
        Self {
            name,
            team,
            player_model,
            state: ClientState::Dead,
            privilege: Privilege::None,
            session_id: 0, // filled in by complete_pending from the pending map
            ping: 0,
        }
    }

    pub(crate) fn with_session_id(mut self, session_id: i32) -> Self {
        self.session_id = session_id;
        self
    }

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

    /// The session ID is set once at connection time and never changes.
    /// It is used as a salt for cryptographic authentication challenges.
    pub fn session_id(&self) -> i32 {
        self.session_id
    }

    /// Ping as last reported by the client via `N_CLIENTPING`.
    /// Cannot be set externally; changes only through the registry pipeline.
    pub fn ping(&self) -> i32 {
        self.ping
    }
}
