use crate::{options::BasePluginOptions, registry::ClientRegistry};
use cube_string::CubeString;
use sauer_proto::{
    channel1::{self, ForceIntermission, GameSpeedClient, ItemList, MapVote, NewMap, PauseGameClient},
    data::{ArmourType, ClientState, GameMode, Gun, ItemSpawn, PlayerAmmo, PlayerState, Privilege},
};
use sauer_server::{
    Server,
    plugin::{Control, PASS},
};
use std::{
    collections::HashMap,
    sync::{
        Arc, Mutex,
        atomic::{AtomicBool, AtomicI32, Ordering},
    },
};
use tokio::time::{Duration, Instant, interval};
use tracing::info;

const GAMELIMIT_MS: u64 = 10 * 60 * 1000;
const OVERTIME_MS: u64 = 2 * 60 * 1000;
const INTERMISSION_MS: u64 = 10_000;
const TIMEUP_BROADCAST_MS: u64 = 1_000;
const TICK_MS: u64 = 100;

// ── GameState ─────────────────────────────────────────────────────────────────

pub struct GameState {
    map_name: Mutex<CubeString>,
    mode: Mutex<GameMode>,
    /// Accumulated game time in milliseconds (not counting paused intervals).
    gamemillis: Mutex<u64>,
    /// Match time limit in milliseconds.
    pub gamelimit: u64,
    /// When the game is paused, no gamemillis advance occurs.
    paused: AtomicBool,
    /// Game speed percentage (default 100).
    gamespeed: AtomicI32,
    /// True once intermission has begun; cleared on map change.
    interm: AtomicBool,
    /// Wall-clock time when intermission started; used to schedule the map change.
    interm_start: Mutex<Option<Instant>>,
    /// Server has not yet received the item layout from the first client.
    notgotitems: AtomicBool,
    /// Items received from the first client after a map change.
    items: Mutex<Vec<ItemSpawn>>,
    /// Votes: (map_name, mode) → set of voter client numbers.
    votes: Mutex<HashMap<(CubeString, GameMode), Vec<u32>>>,
    /// Index into the configured map rotation for the next auto-cycle.
    rotation_index: Mutex<usize>,
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

    /// Remaining seconds to broadcast via N_TIMEUP (negative once over limit).
    pub fn remaining_seconds(&self) -> i32 {
        let millis = *self.gamemillis.lock().unwrap();
        let limit = self.gamelimit;
        if millis >= limit {
            0
        } else {
            ((limit - millis + 999) / 1000) as i32
        }
    }

    /// Begin intermission: set flag, record wall time, broadcast N_TIMEUP 0.
    pub fn start_intermission(&self, server: &Server) {
        if self.interm.swap(true, Ordering::AcqRel) {
            return; // already in intermission
        }
        *self.interm_start.lock().unwrap() = Some(Instant::now());
        server.broadcast_chan1(&channel1::TimeUp { seconds_remaining: 0 });
        info!("intermission started — map change in {}s", INTERMISSION_MS / 1000);
    }

    /// Apply a map change: wipe state, update map/mode, broadcast N_MAPCHANGE,
    /// reset player states, and send N_SPAWNSTATE to non-spectators.
    pub async fn change_map(
        &self,
        map_name: CubeString,
        mode: GameMode,
        server: &Arc<Server>,
        registry: &ClientRegistry,
        options: &BasePluginOptions,
    ) {
        info!("map change → {} ({:?})", map_name, mode);

        // Wipe game state.
        *self.map_name.lock().unwrap() = map_name.clone();
        *self.mode.lock().unwrap() = mode;
        *self.gamemillis.lock().unwrap() = 0;
        *self.interm_start.lock().unwrap() = None;
        self.interm.store(false, Ordering::Release);
        self.paused.store(false, Ordering::Release);
        self.notgotitems.store(true, Ordering::Release);
        self.items.lock().unwrap().clear();
        self.votes.lock().unwrap().clear();

        // Advance rotation index so the next auto-cycle picks the entry after this one.
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
            has_items: has_items(mode),
        });

        // Reset and respawn all non-spectator clients.
        for cn in registry.all_cns() {
            let Some(client) = registry.get(cn) else { continue };
            if client.state() == ClientState::Spectator {
                continue;
            }
            drop(client);

            if is_multiplayer(mode) {
                let state = default_spawn_state(mode);
                server.send_chan1(cn, &channel1::SpawnState { client_num: cn as i32, state });
            }
        }
    }

    /// Record a vote. Returns the current vote count for that (map, mode) pair.
    fn record_vote(&self, cn: u32, map_name: CubeString, mode: GameMode) -> usize {
        let mut votes = self.votes.lock().unwrap();
        let entry = votes.entry((map_name, mode)).or_default();
        if !entry.contains(&cn) {
            entry.push(cn);
        }
        entry.len()
    }

    /// Best voted (map, mode) pair, or None if no votes.
    fn best_voted(&self) -> Option<(CubeString, GameMode)> {
        let votes = self.votes.lock().unwrap();
        votes
            .iter()
            .max_by_key(|(_, voters)| voters.len())
            .map(|((map, mode), _)| (map.clone(), *mode))
    }
}

// ── Tick loop ─────────────────────────────────────────────────────────────────

pub(crate) async fn run_tick_loop(server: Arc<Server>, options: Arc<BasePluginOptions>) {
    let mut ticker = interval(Duration::from_millis(TICK_MS));
    let mut last_tick = Instant::now();
    let mut last_timeup = Instant::now();

    loop {
        ticker.tick().await;

        let Some(game) = server.extra.get::<GameState>() else { return };
        let Some(registry) = server.extra.get::<ClientRegistry>() else { return };

        let now = Instant::now();
        let elapsed_ms = now.duration_since(last_tick).as_millis() as u64;
        last_tick = now;

        // ── Intermission countdown ────────────────────────────────────────────
        if game.is_intermission() {
            let interm_elapsed = game
                .interm_start
                .lock()
                .unwrap()
                .map(|t| t.elapsed().as_millis() as u64)
                .unwrap_or(0);

            if interm_elapsed >= INTERMISSION_MS {
                let (map_name, mode) = game
                    .best_voted()
                    .unwrap_or_else(|| next_rotation(&game, &options));
                game.change_map(map_name, mode, &server, &registry, &options).await;
            }
            continue;
        }

        // ── Advance gamemillis ────────────────────────────────────────────────
        if !game.is_paused() {
            let speed = game.gamespeed.load(Ordering::Acquire).max(0) as u64;
            let delta = elapsed_ms * speed / 100;
            *game.gamemillis.lock().unwrap() += delta;
        }

        let millis = game.gamemillis();

        // ── Time limit check ──────────────────────────────────────────────────
        if millis >= game.gamelimit {
            // TODO: tie detection — if score is tied, extend by overtime instead.
            // For now, always enter intermission.
            let _ = OVERTIME_MS; // suppress unused warning until tie logic is added
            game.start_intermission(&server);
            continue;
        }

        // ── Periodic N_TIMEUP broadcast ───────────────────────────────────────
        if now.duration_since(last_timeup) >= Duration::from_millis(TIMEUP_BROADCAST_MS) {
            last_timeup = now;
            server.broadcast_chan1(&channel1::TimeUp {
                seconds_remaining: game.remaining_seconds(),
            });
        }
    }
}

// ── Mode helpers ──────────────────────────────────────────────────────────────

/// True for all networked multiplayer modes (excludes SP / demo / coop-edit).
pub fn is_multiplayer(mode: GameMode) -> bool {
    !matches!(mode, GameMode::Sp | GameMode::Dmsp | GameMode::Demo | GameMode::CoopEdit)
}

/// True for modes that use the item pickup system (excludes insta/efficiency/tactics variants).
pub fn has_items(mode: GameMode) -> bool {
    matches!(
        mode,
        GameMode::Ffa
            | GameMode::TeamPlay
            | GameMode::Capture
            | GameMode::RegenCapture
            | GameMode::Ctf
            | GameMode::Protect
            | GameMode::Hold
            | GameMode::Collect
    )
}

// ── Default spawn state ───────────────────────────────────────────────────────

/// Generate a default spawn PlayerState for a client entering a new map.
pub fn default_spawn_state(mode: GameMode) -> PlayerState {
    let instagib = matches!(
        mode,
        GameMode::Instagib
            | GameMode::InstaTeam
            | GameMode::InstaCTF
            | GameMode::InstaProtect
            | GameMode::InstaHold
            | GameMode::InstaCollect
    );
    let efficiency = matches!(
        mode,
        GameMode::Efficiency
            | GameMode::EfficTeam
            | GameMode::EfficCTF
            | GameMode::EfficProtect
            | GameMode::EfficHold
            | GameMode::EfficCollect
    );

    if instagib {
        return PlayerState {
            life_sequence: 1,
            health: 1,
            max_health: 1,
            armour: 0,
            armour_type: ArmourType::Blue,
            gun_select: Gun::Rifle,
            ammo: PlayerAmmo {
                shotgun: 0,
                chaingun: 0,
                rocket_launcher: 0,
                rifle: -1, // infinite
                grenade_launcher: 0,
                pistol: 0,
            },
        };
    }

    if efficiency {
        return PlayerState {
            life_sequence: 1,
            health: 100,
            max_health: 100,
            armour: 0,
            armour_type: ArmourType::Blue,
            gun_select: Gun::Fist,
            ammo: PlayerAmmo {
                shotgun: 10,
                chaingun: 20,
                rocket_launcher: 5,
                rifle: 5,
                grenade_launcher: 5,
                pistol: 10,
            },
        };
    }

    // All other multiplayer modes: start with 50 pistol.
    PlayerState {
        life_sequence: 1,
        health: 100,
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
            pistol: 50,
        },
    }
}

// ── Rotation helper ───────────────────────────────────────────────────────────

fn next_rotation(game: &GameState, options: &BasePluginOptions) -> (CubeString, GameMode) {
    if options.maprotations.is_empty() {
        return (game.map_name(), game.mode());
    }
    let idx = *game.rotation_index.lock().unwrap();
    let entry = &options.maprotations[idx % options.maprotations.len()];
    (entry.map_name.clone(), entry.mode)
}

// ── Handlers ──────────────────────────────────────────────────────────────────

pub async fn on_map_vote(
    server: &Arc<Server>,
    cn: u32,
    packet: &MapVote,
    options: &BasePluginOptions,
) -> Control {
    let registry = server.extra.get::<ClientRegistry>().unwrap();
    let game = server.extra.get::<GameState>().unwrap();

    let privilege = registry
        .get(cn)
        .map(|c| c.privilege())
        .unwrap_or(Privilege::None);

    if privilege >= Privilege::Master {
        info!("cn {} forced map {} ({:?})", cn, packet.map_name, packet.mode);
        game.change_map(packet.map_name.clone(), packet.mode, server, &registry, options).await;
        return PASS;
    }

    let vote_count = game.record_vote(cn, packet.map_name.clone(), packet.mode);
    let eligible = registry.client_count();
    info!(
        "cn {} voted for {} ({:?}): {}/{}",
        cn, packet.map_name, packet.mode, vote_count, eligible
    );

    if vote_count * 2 > eligible {
        info!("vote passed for {} ({:?})", packet.map_name, packet.mode);
        game.change_map(packet.map_name.clone(), packet.mode, server, &registry, options).await;
    }

    PASS
}

pub async fn on_new_map(server: &Arc<Server>, cn: u32, packet: &NewMap) -> Control {
    let game = server.extra.get::<GameState>().unwrap();

    if game.mode() != GameMode::CoopEdit {
        return PASS;
    }

    info!("cn {} resized/cleared map to size {}", cn, packet.size);

    game.notgotitems.store(false, Ordering::Release);
    game.items.lock().unwrap().clear();

    let map_name = game.map_name();
    server.broadcast_chan1(&channel1::MapChange {
        map_name,
        mode: GameMode::CoopEdit,
        has_items: false,
    });

    PASS
}

pub async fn on_item_list(server: &Arc<Server>, cn: u32, packet: &ItemList) -> Control {
    let game = server.extra.get::<GameState>().unwrap();

    if !game.notgotitems() {
        return PASS;
    }

    game.notgotitems.store(false, Ordering::Release);
    *game.items.lock().unwrap() = packet.items.clone();

    info!("cn {} provided item list: {} items", cn, packet.items.len());

    PASS
}

pub async fn on_force_intermission(server: &Arc<Server>, cn: u32, _packet: &ForceIntermission) -> Control {
    let registry = server.extra.get::<ClientRegistry>().unwrap();
    let privilege = registry
        .get(cn)
        .map(|c| c.privilege())
        .unwrap_or(Privilege::None);

    if privilege < Privilege::Master {
        return PASS;
    }

    let game = server.extra.get::<GameState>().unwrap();
    info!("cn {} forced intermission", cn);
    game.start_intermission(server);

    PASS
}

pub async fn on_pause_game(server: &Arc<Server>, cn: u32, packet: &PauseGameClient) -> Control {
    let registry = server.extra.get::<ClientRegistry>().unwrap();
    let privilege = registry
        .get(cn)
        .map(|c| c.privilege())
        .unwrap_or(Privilege::None);

    if privilege < Privilege::Master {
        return PASS;
    }

    let game = server.extra.get::<GameState>().unwrap();
    game.paused.store(packet.val, Ordering::Release);

    info!("cn {} {} the game", cn, if packet.val { "paused" } else { "unpaused" });

    server.broadcast_chan1(&channel1::PauseGameServer {
        is_paused: packet.val,
        client_num: cn as i32,
    });

    PASS
}

pub async fn on_game_speed(server: &Arc<Server>, cn: u32, packet: &GameSpeedClient) -> Control {
    let registry = server.extra.get::<ClientRegistry>().unwrap();
    let privilege = registry
        .get(cn)
        .map(|c| c.privilege())
        .unwrap_or(Privilege::None);

    if privilege < Privilege::Admin {
        return PASS;
    }

    let speed = packet.speed.clamp(10, 1000);
    let game = server.extra.get::<GameState>().unwrap();
    game.gamespeed.store(speed, Ordering::Release);

    info!("cn {} set game speed to {}", cn, speed);

    server.broadcast_chan1(&channel1::GameSpeedServer {
        speed,
        client_num: cn as i32,
    });

    PASS
}

/// Send the current map state to a newly connected client.
pub(crate) fn send_map_info(server: &Server, cn: u32) {
    let Some(game) = server.extra.get::<GameState>() else { return };
    let mode = game.mode();
    server.send_chan1(
        cn,
        &channel1::MapChange {
            map_name: game.map_name(),
            mode,
            has_items: has_items(mode),
        },
    );
    if has_items(mode) && !game.notgotitems() {
        let items = game.items.lock().unwrap().clone();
        if !items.is_empty() {
            server.send_chan1(cn, &channel1::ItemList { items });
        }
    }
}
