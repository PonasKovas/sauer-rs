use dashmap::DashMap;
use rand::random;

pub mod combat;
mod data;
pub mod model;
pub mod name;
pub mod position;
pub mod privilege;
pub mod spectator;
pub mod state;
mod status;
pub mod team;

pub use data::PlayerData;
pub use status::Status;

/// Per-player data, keyed by client id.
///
/// Access goes through closures, so a lock on a player's entry can't outlive
/// the call or be held across an `.await`. The entry stays locked while the
/// closure runs: don't touch `Players` (or call `Server` methods that do) from
/// inside one, since two entries can share a lock.
#[derive(Default)]
pub struct Players {
    map: DashMap<u64, PlayerData>,
}

impl Players {
    pub(crate) fn new() -> Self {
        Self::default()
    }

    /// Runs `f` on the player's data. `None` if there is no such player.
    pub fn with<R>(&self, id: u64, f: impl FnOnce(&PlayerData) -> R) -> Option<R> {
        self.map.get(&id).map(|p| f(&p))
    }

    /// Runs `f` on the player's data, mutably. `None` if there is no such player.
    pub fn with_mut<R>(&self, id: u64, f: impl FnOnce(&mut PlayerData) -> R) -> Option<R> {
        self.map.get_mut(&id).map(|mut p| f(&mut p))
    }

    /// Collects what `f` returns for each player, skipping `None`.
    pub fn filter_map<R>(&self, mut f: impl FnMut(u64, &PlayerData) -> Option<R>) -> Vec<R> {
        self.map
            .iter()
            .filter_map(|e| f(*e.key(), e.value()))
            .collect()
    }

    /// Whether the player exists.
    pub fn contains(&self, id: u64) -> bool {
        self.map.contains_key(&id)
    }

    /// Whether `f` holds for any player.
    pub fn any(&self, mut f: impl FnMut(u64, &PlayerData) -> bool) -> bool {
        self.map.iter().any(|e| f(*e.key(), e.value()))
    }

    /// How many players `f` holds for.
    pub fn count(&self, mut f: impl FnMut(u64, &PlayerData) -> bool) -> usize {
        self.map.iter().filter(|e| f(*e.key(), e.value())).count()
    }

    /// Runs `f` on every player's data, mutably.
    pub fn for_each_mut(&self, mut f: impl FnMut(u64, &mut PlayerData)) {
        for mut e in self.map.iter_mut() {
            let id = *e.key();
            f(id, e.value_mut());
        }
    }
}

/// Creates a new player entry with a random session id. Returns the session id.
pub(crate) fn init_player(players: &Players, id: u64) -> i32 {
    let session_id = random();
    players.map.insert(id, PlayerData::new(session_id));
    session_id
}

pub(crate) fn clear_player(players: &Players, id: u64) {
    players.map.remove(&id);
}
