//! Loaded maps and the map rotation.

use super::{Afk, RoundState};
use crate::map_state::{DeathrunState, Trap};
use sauer_map::Map;

use std::collections::HashMap;
use std::collections::HashSet;
use tokio::time::Duration;
use tracing::info;

pub struct LoadedMap {
    pub state: Map,
    pub traps: Vec<Trap>,
}

pub struct PlayMaps {
    pub maps: HashMap<String, LoadedMap>,
    pub rotation: Vec<String>,
    pub rotation_idx: usize,
    pub current_map: Option<String>,
    pub evil_id: Option<u64>,
    pub round_state: RoundState,
    pub round_epoch: u64,
    pub round_task: Option<tokio::task::JoinHandle<()>>,
    pub spawned_this_round: HashSet<u64>,
    pub finished_players: Vec<(u64, Duration)>,
    /// Its own lock: every position packet updates it.
    pub afk: parking_lot::Mutex<Afk>,
}

impl Default for PlayMaps {
    fn default() -> Self {
        Self {
            maps: HashMap::new(),
            rotation: Vec::new(),
            rotation_idx: 0,
            current_map: None,
            evil_id: None,
            round_state: RoundState::NotStarted,
            round_epoch: 0,
            round_task: None,
            spawned_this_round: HashSet::new(),
            finished_players: Vec::new(),
            afk: Default::default(),
        }
    }
}

impl LoadedMap {
    /// Loads a map file with its traps. Slow for big maps: call it without
    /// holding the `PlayMaps` lock, then [`PlayMaps::insert`] the result.
    pub fn parse(name: &str, data: &[u8]) -> Result<Self, String> {
        let mut state = Map::from_bytes(data).map_err(|e| e.to_string())?;
        let loaded = crate::trap::persist::ingest(&mut state);
        for w in &loaded.warnings {
            tracing::warn!("map '{name}': {w}");
        }
        let traps = loaded.traps;
        if !traps.is_empty() {
            info!("Loaded {} traps from map '{name}'", traps.len());
        }
        // Players never save play maps, and the server keeps trap geometry in
        // memory, so the vault is just dead weight for them.
        if let Some(v) = loaded.vault {
            crate::trap::vault::erase(&mut state, &v);
        }

        Ok(Self { state, traps })
    }
}

impl PlayMaps {
    /// Adds a map to the rotation, or replaces the one with that name.
    pub fn insert(&mut self, name: String, map: LoadedMap) {
        if !self.rotation.contains(&name) {
            self.rotation.push(name.clone());
        }

        if self.current_map.is_none() && !self.rotation.is_empty() {
            self.current_map = Some(self.rotation[0].clone());
        }

        self.maps.insert(name, map);
    }

    pub fn remove_map(&mut self, name: &str) {
        self.maps.remove(name);
        if let Some(pos) = self.rotation.iter().position(|x| x == name) {
            self.rotation.remove(pos);
            if pos < self.rotation_idx {
                self.rotation_idx = self.rotation_idx.saturating_sub(1);
            }
        }
        if self.rotation_idx >= self.rotation.len() {
            self.rotation_idx = 0;
        }
        if self.current_map.as_deref() == Some(name) {
            self.current_map = self.rotation.first().cloned();
        }
    }

    /// A fresh game state on the current map, `None` if no map is loaded.
    pub(super) fn make_active_state(&self) -> Option<DeathrunState> {
        let loaded = self.maps.get(self.current_map.as_ref()?)?;
        Some(DeathrunState {
            map: loaded.state.clone(),
            traps: loaded.traps.clone(),
            ..Default::default()
        })
    }
}
