use crate::{modes, options::BasePluginOptions, registry::ClientRegistry};
use cube_string::CubeString;
use sauer_proto::{
    channel1,
    data::{ClientState, GameMode, ItemSpawn},
};
use sauer_server::Server;
use std::{
    collections::HashMap,
    sync::{
        Arc, Mutex,
        atomic::{AtomicBool, AtomicI32, Ordering},
    },
};
use tokio::time::Instant;
use tracing::info;

pub const GAMELIMIT_MS: u64 = 10 * 60 * 1000;

// ── GameState ─────────────────────────────────────────────────────────────────

pub struct GameState {
    map_name: Mutex<CubeString>,
    mode: Mutex<GameMode>,
    /// Accumulated match time in milliseconds (paused intervals excluded).
    gamemillis: Mutex<u64>,
    /// Match time limit in milliseconds.
    pub gamelimit: u64,
    pub(crate) paused: AtomicBool,
    pub(crate) gamespeed: AtomicI32,
    interm: AtomicBool,
    /// Wall-clock instant when intermission began; drives the post-interm map change.
    pub(crate) interm_start: Mutex<Option<Instant>>,
    pub(crate) notgotitems: AtomicBool,
    pub(crate) items: Mutex<Vec<ItemSpawn>>,
    votes: Mutex<HashMap<(CubeString, GameMode), Vec<u32>>>,
    pub(crate) rotation_index: Mutex<usize>,
}

impl GameState {
    pub(crate) fn new(map_name: CubeString, mode: GameMode) -> Self {
        Self {
            map_name: Mutex::new(map_name),
            mode: Mutex::new(mode),
            gamemillis: Mutex::new(0),
            gamelimit: GAMELIMIT_MS,
            paused: AtomicBool::new(false),
            gamespeed: AtomicI32::new(100),
            interm: AtomicBool::new(false),
            interm_start: Mutex::new(None),
            notgotitems: AtomicBool::new(true),
            items: Mutex::new(Vec::new()),
            votes: Mutex::new(HashMap::new()),
            rotation_index: Mutex::new(0),
        }
    }

    pub fn map_name(&self) -> CubeString {
        self.map_name.lock().unwrap().clone()
    }

    pub fn mode(&self) -> GameMode {
        *self.mode.lock().unwrap()
    }

    pub fn gamemillis(&self) -> u64 {
        *self.gamemillis.lock().unwrap()
    }

    pub fn is_paused(&self) -> bool {
        self.paused.load(Ordering::Acquire)
    }

    pub fn gamespeed(&self) -> i32 {
        self.gamespeed.load(Ordering::Acquire)
    }

    pub fn is_intermission(&self) -> bool {
        self.interm.load(Ordering::Acquire)
    }

    pub fn notgotitems(&self) -> bool {
        self.notgotitems.load(Ordering::Acquire)
    }

    pub fn remaining_seconds(&self) -> i32 {
        let millis = *self.gamemillis.lock().unwrap();
        let limit = self.gamelimit;
        if millis >= limit {
            0
        } else {
            ((limit - millis + 999) / 1000) as i32
        }
    }

    /// Begin intermission: record wall time and broadcast N_TIMEUP 0.
    /// No-op if already in intermission.
    pub fn start_intermission(&self, server: &Server) {
        if self.interm.swap(true, Ordering::AcqRel) {
            return;
        }
        *self.interm_start.lock().unwrap() = Some(Instant::now());
        server.broadcast_chan1(&channel1::TimeUp { seconds_remaining: 0 });
        info!("intermission started — map change in 10s");
    }

    /// Advance gamemillis by `elapsed_ms * speed / 100`. Returns new value.
    pub(crate) fn advance_time(&self, elapsed_ms: u64) -> u64 {
        if self.paused.load(Ordering::Acquire) {
            return *self.gamemillis.lock().unwrap();
        }
        let speed = self.gamespeed.load(Ordering::Acquire).max(0) as u64;
        let delta = elapsed_ms * speed / 100;
        let mut guard = self.gamemillis.lock().unwrap();
        *guard += delta;
        *guard
    }

    /// Wipe state and apply a new map/mode; broadcast N_MAPCHANGE; respawn clients.
    pub async fn change_map(
        &self,
        map_name: CubeString,
        mode: GameMode,
        server: &Arc<Server>,
        registry: &ClientRegistry,
        options: &BasePluginOptions,
    ) {
        info!("map change → {} ({:?})", map_name, mode);

        *self.map_name.lock().unwrap() = map_name.clone();
        *self.mode.lock().unwrap() = mode;
        *self.gamemillis.lock().unwrap() = 0;
        *self.interm_start.lock().unwrap() = None;
        self.interm.store(false, Ordering::Release);
        self.paused.store(false, Ordering::Release);
        self.notgotitems.store(true, Ordering::Release);
        self.items.lock().unwrap().clear();
        self.votes.lock().unwrap().clear();

        if !options.maprotations.is_empty() {
            let mut idx = self.rotation_index.lock().unwrap();
            *idx = options
                .maprotations
                .iter()
                .position(|r| r.map_name == map_name && r.mode == mode)
                .map(|i| (i + 1) % options.maprotations.len())
                .unwrap_or((*idx + 1) % options.maprotations.len());
        }

        server.broadcast_chan1(&channel1::MapChange {
            map_name,
            mode,
            has_items: modes::has_items(mode),
        });

        if modes::is_multiplayer(mode) {
            for cn in registry.all_cns() {
                let Some(client) = registry.get(cn) else { continue };
                if client.state() == ClientState::Spectator {
                    continue;
                }
                drop(client);
                let state = modes::default_spawn_state(mode);
                server.send_chan1(cn, &channel1::SpawnState { client_num: cn as i32, state });
            }
        }
    }

    pub(crate) fn record_vote(&self, cn: u32, map_name: CubeString, mode: GameMode) -> usize {
        let mut votes = self.votes.lock().unwrap();
        let entry = votes.entry((map_name, mode)).or_default();
        if !entry.contains(&cn) {
            entry.push(cn);
        }
        entry.len()
    }

    /// Returns the (map, mode) pair with the most votes, if any.
    pub(crate) fn best_voted(&self) -> Option<(CubeString, GameMode)> {
        let votes = self.votes.lock().unwrap();
        votes
            .iter()
            .max_by_key(|(_, voters)| voters.len())
            .map(|((map, mode), _)| (map.clone(), *mode))
    }

    /// Next map from the rotation, or the current map if no rotation is configured.
    pub(crate) fn next_rotation(&self, options: &BasePluginOptions) -> (CubeString, GameMode) {
        if options.maprotations.is_empty() {
            return (self.map_name(), self.mode());
        }
        let idx = *self.rotation_index.lock().unwrap();
        let entry = &options.maprotations[idx % options.maprotations.len()];
        (entry.map_name.clone(), entry.mode)
    }
}
