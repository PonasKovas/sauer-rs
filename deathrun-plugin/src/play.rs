use crate::map_state::{DeathrunState, MapState, Trap, get_server_state};
use crate::trap::{StoredTrap, TRAP_DOMAIN, TRAP_STORE_VERSION};
use crate::ui;
use cube_string::IntoCubeStringLossy;
use ogz_rs::data::{MapConfig, Variable};
use rand::seq::SliceRandom;
use sauer_proto::{
    Client, channel0, channel1,
    data::{
        ClientState, EditVarValue, EntityType, GameMode, MasterMode, Orient, SelInfo,
        SetTeamReason, Vec3,
    },
};
use sauer_server::{
    Server,
    plugin::{Context, Plugin},
};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::collections::HashSet;
use std::sync::{Arc, RwLock};
use tokio::time::{Duration, Instant, sleep};
use tracing::{info, warn};

// ─── Config ───────────────────────────────────────────────────────────────────

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PlayConfig {
    /// Seconds without movement before a player is auto-spectated for AFK.
    #[serde(default = "default_afk_timeout")]
    pub afk_timeout_secs: u64,
}

fn default_afk_timeout() -> u64 {
    10
}

impl Default for PlayConfig {
    fn default() -> Self {
        Self {
            afk_timeout_secs: default_afk_timeout(),
        }
    }
}

#[derive(PartialEq, Clone, Copy)]
pub enum RoundState {
    NotStarted,
    MapLoading,
    Playing,
    Ended,
}

pub struct LoadedMap {
    pub state: MapState,
    pub traps: Vec<Trap>,
}

pub struct PlayMaps {
    pub maps: HashMap<String, LoadedMap>,
    pub rotation: Vec<String>,
    pub rotation_idx: usize,
    pub current_map: Option<String>,
    pub evil_cn: Option<u32>,
    pub round_state: RoundState,
    pub spawned_this_round: HashSet<u32>,
    /// Last recorded position per connected player.
    afk_last_pos: HashMap<u32, [f32; 3]>,
    /// Time of the last significant movement per player.
    afk_last_move: HashMap<u32, Instant>,
}

impl Default for PlayMaps {
    fn default() -> Self {
        Self {
            maps: HashMap::new(),
            rotation: Vec::new(),
            rotation_idx: 0,
            current_map: None,
            evil_cn: None,
            round_state: RoundState::NotStarted,
            spawned_this_round: HashSet::new(),
            afk_last_pos: HashMap::new(),
            afk_last_move: HashMap::new(),
        }
    }
}

impl PlayMaps {
    pub fn add_map(&mut self, name: String, data: &[u8]) -> Result<(), String> {
        let mut state = MapState::from_bytes(data).map_err(|e| e.to_string())?;
        let mut traps = Vec::new();
        match crate::entity_store::read::<Vec<StoredTrap>>(
            &state.world,
            TRAP_STORE_VERSION,
            crate::trap::TRAP_DOMAIN,
        ) {
            Ok(Some(stored)) => {
                for st in &stored {
                    st.apply_reset_to_world(&mut state.world);
                }
                traps = stored.into_iter().filter_map(|st| st.into_trap()).collect();
                info!("Loaded {} traps from map '{name}'", traps.len());
            }
            Ok(None) => {}
            Err(e) => warn!("Could not load traps from map '{name}': {e}"),
        }

        if !self.rotation.contains(&name) {
            self.rotation.push(name.clone());
        }

        if self.current_map.is_none() && !self.rotation.is_empty() {
            self.current_map = Some(self.rotation[0].clone());
        }

        self.maps.insert(name, LoadedMap { state, traps });
        Ok(())
    }

    pub fn remove_map(&mut self, name: &str) {
        self.maps.remove(name);
        if let Some(pos) = self.rotation.iter().position(|x| x == name) {
            self.rotation.remove(pos);
        }
        if self.rotation_idx >= self.rotation.len() {
            self.rotation_idx = 0;
        }
        if self.current_map.as_deref() == Some(name) {
            self.current_map = self.rotation.first().cloned();
        }
    }

    fn make_active_state(&self) -> (Option<String>, DeathrunState) {
        let mut ds = DeathrunState::default();
        if let Some(name) = &self.current_map {
            if let Some(loaded) = self.maps.get(name) {
                ds.map = Some(loaded.state.clone());
                ds.traps = loaded.traps.clone();
                return (Some(name.clone()), ds);
            }
        }
        (None, ds)
    }

    pub fn update_afk_pos(&mut self, cn: u32, pos: [f32; 3]) {
        let moved = self.afk_last_pos.get(&cn).map_or(true, |last| {
            let d = [pos[0] - last[0], pos[1] - last[1], pos[2] - last[2]];
            d[0] * d[0] + d[1] * d[1] + d[2] * d[2] > 0.25 // 0.5 unit threshold
        });
        self.afk_last_pos.insert(cn, pos);
        if moved {
            self.afk_last_move.insert(cn, Instant::now());
        }
    }

    pub fn remove_player_afk(&mut self, cn: u32) {
        self.afk_last_pos.remove(&cn);
        self.afk_last_move.remove(&cn);
    }
}

pub struct DeathrunPlay {
    maps: Arc<RwLock<PlayMaps>>,
    config: PlayConfig,
}

impl DeathrunPlay {
    pub fn new(config: PlayConfig) -> Self {
        Self {
            maps: Arc::new(RwLock::new(PlayMaps::default())),
            config,
        }
    }

    pub fn maps_handle(&self) -> Arc<RwLock<PlayMaps>> {
        Arc::clone(&self.maps)
    }

    pub fn add_map(&self, name: String, data: &[u8]) -> Result<(), String> {
        self.maps.write().unwrap().add_map(name, data)
    }

    pub fn remove_map(&self, name: &str) {
        self.maps.write().unwrap().remove_map(name);
    }
}

fn broadcast_map_vars(server: &Server, author_cn: i32, variables: &[Variable]) {
    for var in variables {
        let (name, val) = match var {
            Variable::Int { name, value } => (name.clone(), EditVarValue::Int(*value)),
            Variable::Float { name, value } => (name.clone(), EditVarValue::Float(*value)),
            Variable::String { name, value } => (name.clone(), EditVarValue::String(value.clone())),
        };
        server.broadcast_chan1(&Client {
            client_num: author_cn,
            packet: channel1::EditVar { name, val },
        });
    }
}

fn send_map_vars(server: &Server, target_cn: u32, author_cn: i32, variables: &[Variable]) {
    for var in variables {
        let (name, val) = match var {
            Variable::Int { name, value } => (name.clone(), EditVarValue::Int(*value)),
            Variable::Float { name, value } => (name.clone(), EditVarValue::Float(*value)),
            Variable::String { name, value } => (name.clone(), EditVarValue::String(value.clone())),
        };
        server.send_chan1(
            target_cn,
            &Client {
                client_num: author_cn,
                packet: channel1::EditVar { name, val },
            },
        );
    }
}

fn start_round_sequence(server: Arc<Server>, maps: Arc<RwLock<PlayMaps>>) {
    tokio::spawn(async move {
        let (world_size, root, entities, vslots, variables, store_ids) = {
            let mut lock = maps.write().unwrap();
            if lock.rotation.is_empty() {
                return;
            }
            lock.round_state = RoundState::MapLoading;
            lock.spawned_this_round.clear();

            if lock.rotation_idx >= lock.rotation.len() {
                lock.rotation.shuffle(&mut rand::thread_rng());
                lock.rotation_idx = 0;
            }
            let next_map = lock.rotation[lock.rotation_idx].clone();
            lock.rotation_idx += 1;
            lock.current_map = Some(next_map);

            let (_, new_state) = lock.make_active_state();
            *get_server_state(&server).write().unwrap() = new_state;

            let state_lock = get_server_state(&server);
            let state_read = state_lock.read().unwrap();
            let map = state_read.map.as_ref().unwrap();
            let store_ids = crate::entity_store::get_store_entity_ids(&map.world, TRAP_DOMAIN)
                .into_iter()
                .collect::<HashSet<_>>();
            let variables = match &map.meta.config {
                MapConfig::Modern { variables } => variables.clone(),
                _ => vec![],
            };
            (
                map.world.size,
                map.world.root.clone(),
                map.world.entities.clone(),
                map.meta.vslots.clone(),
                variables,
                store_ids,
            )
        };

        server.set_master_mode(MasterMode::Locked);
        for cn in server.client_cns() {
            server.set_spectator(cn, true);
        }

        server.broadcast_chan1(&channel1::NewMap {
            size: world_size.trailing_zeros() as i32,
        });
        let author_cn = server.client_cns().first().copied().unwrap_or(0) as i32;
        server.broadcast_chan1(&Client {
            client_num: author_cn,
            packet: channel1::DelCube {
                sel: SelInfo {
                    o: Vec3 { x: 0, y: 0, z: 0 },
                    s: Vec3 { x: 2, y: 2, z: 2 },
                    grid: world_size / 2,
                    orient: Orient::Left,
                    cx: 0,
                    cxs: 2,
                    cy: 0,
                    cys: 2,
                    corner: 0,
                },
            },
        });
        broadcast_map_vars(&server, author_cn, &variables);

        for i in 0..entities.len() {
            server.broadcast_chan1(&Client {
                client_num: author_cn,
                packet: channel1::EditEnt {
                    ent_id: i as i32,
                    pos: Vec3 {
                        x: 0.0,
                        y: 0.0,
                        z: 0.0,
                    },
                    type_: EntityType::Empty,
                    attrs: [0; 5],
                },
            });
        }

        sleep(Duration::from_millis(500)).await;

        let mut chunks = Vec::new();
        crate::recreate::collect_chunks(&root, [0, 0, 0], world_size, world_size / 4, &mut chunks);
        //chunks.shuffle(&mut rand::thread_rng());
        crate::geo_sync::broadcast_chunks(&server, author_cn, chunks, &vslots, 20).await;

        sleep(Duration::from_millis(500)).await;

        for (i, ent) in entities.into_iter().enumerate() {
            if store_ids.contains(&i) {
                continue;
            }
            server.broadcast_chan1(&Client {
                client_num: author_cn,
                packet: channel1::EditEnt {
                    ent_id: i as i32,
                    pos: Vec3 {
                        x: ent.position[0],
                        y: ent.position[1],
                        z: ent.position[2],
                    },
                    type_: ent.ent_type,
                    attrs: ent.attrs.map(|x| x as i32),
                },
            });
        }

        let cns = server.client_cns();
        if cns.is_empty() {
            let mut lock = maps.write().unwrap();
            lock.round_state = RoundState::NotStarted;
            server.set_master_mode(MasterMode::Veto);
            return;
        }

        let evil_cn = if cns.len() > 1 {
            Some(*cns.choose(&mut rand::thread_rng()).unwrap())
        } else {
            None
        };

        {
            let mut lock = maps.write().unwrap();
            lock.evil_cn = evil_cn;
            lock.round_state = RoundState::Playing;
        }

        for &cn in &cns {
            let team = if Some(cn) == evil_cn { "evil" } else { "good" };
            server.set_team(cn, team.lossy_cubestring(), SetTeamReason::Forced);
            server.set_spectator(cn, false); // Moves them into Dead state safely
            server.send_chan1(
                cn,
                &channel1::ForceDeath {
                    client_num: cn as i32,
                },
            ); // Resets screen
        }

        let evil_name = evil_cn
            .and_then(|cn| server.players.try_get(cn))
            .map(|p| p.name.strip_colors().to_utf8());
        ui::broadcast_round_start(&server, evil_name.as_deref());
    });
}

fn check_win_conditions(server: &Arc<Server>, maps: &Arc<RwLock<PlayMaps>>) {
    let mut lock = maps.write().unwrap();
    if lock.round_state != RoundState::Playing {
        return;
    }

    let cns = server.client_cns();
    if cns.is_empty() {
        lock.round_state = RoundState::NotStarted;
        server.set_master_mode(MasterMode::Veto);
        return;
    }

    let evil_cn = match lock.evil_cn {
        Some(cn) => cn,
        None => return,
    };

    if !cns.contains(&evil_cn) {
        declare_win(
            server,
            maps,
            &mut lock,
            "\x0C1Good\x0C7 (Evil disconnected)",
        );
        return;
    }

    let evil_state = server.players.try_get(evil_cn).map(|p| p.client_state);
    if evil_state == Some(ClientState::Dead) || evil_state == Some(ClientState::Spectator) {
        declare_win(server, maps, &mut lock, "\x0C1Good\x0C7 (Evil died)");
        return;
    }

    let mut good_spawned_count = 0;
    let mut good_alive_count = 0;

    for &cn in &cns {
        if cn == evil_cn {
            continue;
        }
        if lock.spawned_this_round.contains(&cn) {
            good_spawned_count += 1;
            if let Some(p) = server.players.try_get(cn) {
                if p.client_state == ClientState::Alive {
                    good_alive_count += 1;
                }
            }
        }
    }

    if good_spawned_count > 0 && good_alive_count == 0 {
        declare_win(
            server,
            maps,
            &mut lock,
            "\x0C3Evil\x0C7 (All Good players died)",
        );
    }
}

fn declare_win(
    server: &Arc<Server>,
    maps: &Arc<RwLock<PlayMaps>>,
    lock: &mut std::sync::RwLockWriteGuard<PlayMaps>,
    winner_text: &str,
) {
    lock.round_state = RoundState::Ended;
    ui::broadcast_round_over(server, winner_text);
    start_round_sequence(Arc::clone(server), Arc::clone(maps));
}

fn send_map_to_client(server: Arc<Server>, target_cn: u32, maps: &Arc<RwLock<PlayMaps>>) -> bool {
    let (world_size, root, entities, vslots, variables, store_ids) = {
        let lock = maps.read().unwrap();
        let name = match &lock.current_map {
            Some(n) => n,
            None => return false,
        };
        let loaded = match lock.maps.get(name) {
            Some(m) => m,
            None => return false,
        };
        let store_ids = crate::entity_store::get_store_entity_ids(&loaded.state.world, TRAP_DOMAIN)
            .into_iter()
            .collect::<HashSet<_>>();
        let variables = match &loaded.state.meta.config {
            MapConfig::Modern { variables } => variables.clone(),
            _ => vec![],
        };
        (
            loaded.state.world.size,
            loaded.state.world.root.clone(),
            loaded.state.world.entities.clone(),
            loaded.state.meta.vslots.clone(),
            variables,
            store_ids,
        )
    };

    tokio::spawn(async move {
        server.send_chan1(
            target_cn,
            &Client {
                client_num: target_cn as i32,
                packet: channel1::NewMap {
                    size: world_size.trailing_zeros() as i32,
                },
            },
        );

        server.send_chan1(
            target_cn,
            &Client {
                client_num: target_cn as i32,
                packet: channel1::DelCube {
                    sel: SelInfo {
                        o: Vec3 { x: 0, y: 0, z: 0 },
                        s: Vec3 { x: 2, y: 2, z: 2 },
                        grid: world_size / 2,
                        orient: Orient::Left,
                        cx: 0,
                        cxs: 2,
                        cy: 0,
                        cys: 2,
                        corner: 0,
                    },
                },
            },
        );
        let author_cn = server.client_cns().first().copied().unwrap_or(target_cn) as i32;
        send_map_vars(&server, target_cn, author_cn, &variables);

        for i in 0..entities.len() {
            server.send_chan1(
                target_cn,
                &Client {
                    client_num: target_cn as i32,
                    packet: channel1::EditEnt {
                        ent_id: i as i32,
                        pos: Vec3 {
                            x: 0.0,
                            y: 0.0,
                            z: 0.0,
                        },
                        type_: EntityType::Empty,
                        attrs: [0; 5],
                    },
                },
            );
        }

        sleep(Duration::from_millis(500)).await;

        let mut chunks = Vec::new();
        crate::recreate::collect_chunks(&root, [0, 0, 0], world_size, world_size / 4, &mut chunks);
        //chunks.shuffle(&mut rand::thread_rng());

        let author_cn = server.client_cns().first().copied().unwrap_or(0) as i32;
        crate::geo_sync::send_chunks_to_client(&server, target_cn, author_cn, chunks, &vslots, 20)
            .await;

        sleep(Duration::from_millis(500)).await;

        for (i, ent) in entities.into_iter().enumerate() {
            if store_ids.contains(&i) {
                continue;
            }
            server.send_chan1(
                target_cn,
                &Client {
                    client_num: target_cn as i32,
                    packet: channel1::EditEnt {
                        ent_id: i as i32,
                        pos: Vec3 {
                            x: ent.position[0],
                            y: ent.position[1],
                            z: ent.position[2],
                        },
                        type_: ent.ent_type,
                        attrs: ent.attrs.map(|x| x as i32),
                    },
                },
            );
        }
    });

    true
}

fn check_afk(server: &Arc<Server>, maps: &Arc<RwLock<PlayMaps>>, timeout: Duration) {
    let now = Instant::now();

    let to_spectate: Vec<u32> = {
        let lock = maps.read().unwrap();
        if lock.round_state != RoundState::Playing {
            return;
        }
        let cns = server.client_cns();
        if cns.len() <= 1 {
            return;
        }
        cns.into_iter()
            .filter(|&cn| {
                server
                    .players
                    .try_get(cn)
                    .map_or(false, |p| p.client_state == ClientState::Alive)
                    && lock
                        .afk_last_move
                        .get(&cn)
                        .map_or(false, |&t| now.duration_since(t) >= timeout)
            })
            .collect()
    };

    for cn in &to_spectate {
        let name = server
            .players
            .try_get(*cn)
            .map(|p| p.name.strip_colors().to_utf8())
            .unwrap_or_default();
        ui::broadcast(
            server,
            ui::warn_msg(&format!("{name} was spectated for being AFK.")),
        );
        server.set_spectator(*cn, true);
    }

    if !to_spectate.is_empty() {
        check_win_conditions(server, maps);
    }
}

impl Plugin for DeathrunPlay {
    async fn init(&self, server: &mut Server) {
        *server.map_name.write().unwrap() = "".lossy_cubestring();
        *server.game_mode.write().unwrap() = GameMode::TeamPlay;
        *server.master_mode.write().unwrap() = MasterMode::Veto;

        let (_, state) = self.maps.read().unwrap().make_active_state();
        server.extra.insert(RwLock::new(state));
        server.extra.insert(Arc::clone(&self.maps));
    }

    async fn start(&self, server: &Arc<Server>) {
        let maps = Arc::clone(server.extra.get::<Arc<RwLock<PlayMaps>>>().unwrap());
        let server = Arc::clone(server);
        let timeout = Duration::from_secs(self.config.afk_timeout_secs);
        tokio::spawn(async move {
            loop {
                sleep(Duration::from_secs(1)).await;
                check_afk(&server, &maps, timeout);
            }
        });
    }

    async fn on_client_welcomed(&self, ctx: &mut Context) {
        let cn = ctx.cn();
        let server = ctx.server();
        let maps = Arc::clone(server.extra.get::<Arc<RwLock<PlayMaps>>>().unwrap());

        let (is_first, is_solo_round, map_name) = {
            let lock = maps.read().unwrap();
            let is_first = server.client_count() == 1;
            let is_solo_round = lock.round_state == RoundState::Playing && lock.evil_cn.is_none();
            let map_name = lock.current_map.clone();
            (is_first, is_solo_round, map_name)
        };

        ui::send_welcome_play(server, cn, map_name.as_deref(), is_solo_round || is_first);

        if is_first || is_solo_round {
            // First player, or a second player joined a solo round: restart so evil gets assigned.
            start_round_sequence(Arc::clone(server), Arc::clone(&maps));
        } else {
            let is_playing = maps.read().unwrap().round_state != RoundState::NotStarted;
            if is_playing {
                send_map_to_client(Arc::clone(server), cn, &maps);
                server.set_spectator(cn, true);
                server.set_team(cn, "good".lossy_cubestring(), SetTeamReason::Forced);
            }
        }
    }

    async fn pos(&self, ctx: &mut Context, packet: &channel0::Position) {
        let cn = ctx.cn();
        let server = ctx.server();
        let maps = server.extra.get::<Arc<RwLock<PlayMaps>>>().unwrap();
        maps.write()
            .unwrap()
            .update_afk_pos(cn, [packet.pos.x, packet.pos.y, packet.pos.z]);
    }

    async fn disconnected(&self, ctx: &mut Context) {
        let cn = ctx.cn();
        let server = ctx.server();
        let maps = Arc::clone(server.extra.get::<Arc<RwLock<PlayMaps>>>().unwrap());
        maps.write().unwrap().remove_player_afk(cn);
        check_win_conditions(server, &maps);
    }

    async fn on_player_died(&self, ctx: &mut Context, _victim_cn: u32, _actor_cn: u32) {
        let server = ctx.server();
        let maps = Arc::clone(server.extra.get::<Arc<RwLock<PlayMaps>>>().unwrap());
        check_win_conditions(server, &maps);
    }

    async fn on_damage(&self, ctx: &mut Context, target_cn: u32, actor_cn: u32, _damage: &mut i32) {
        if target_cn == actor_cn {
            return; // Self-damage is allowed
        }
        let server = ctx.server();
        let target_team = server.players.try_get(target_cn).map(|p| p.team.clone());
        let actor_team = server.players.try_get(actor_cn).map(|p| p.team.clone());

        if target_team.is_some() && target_team == actor_team {
            ctx.cancel();
        }
    }

    async fn try_spawn(&self, ctx: &mut Context, _packet: &channel1::TrySpawn) {
        let cn = ctx.cn();
        let server = ctx.server();
        let maps = server.extra.get::<Arc<RwLock<PlayMaps>>>().unwrap();

        let mut cancel = false;
        {
            let mut lock = maps.write().unwrap();
            match lock.round_state {
                RoundState::Playing => {
                    let solo = lock.evil_cn.is_none();
                    if !solo && lock.spawned_this_round.contains(&cn) {
                        cancel = true; // One life only in multiplayer!
                    } else {
                        lock.spawned_this_round.insert(cn);
                    }
                }
                RoundState::MapLoading | RoundState::Ended => {
                    cancel = true; // Wait for round to actually start
                }
                RoundState::NotStarted => {}
            }
        }
        if cancel {
            ctx.cancel();
        }
    }

    async fn shoot(&self, ctx: &mut Context, packet: &channel1::Shoot) {
        let cn = ctx.cn();
        let server = ctx.server();

        let is_evil = {
            let maps = server.extra.get::<Arc<RwLock<PlayMaps>>>().unwrap();
            maps.read().unwrap().evil_cn == Some(cn)
        };
        if !is_evil {
            return;
        }

        let hit_pt = [packet.to.x, packet.to.y, packet.to.z];
        let trap_idx = {
            let state = crate::map_state::get_server_state(server).read().unwrap();
            state.traps.iter().position(|trap| {
                let b_min = trap.button_pos;
                let b_max = [
                    b_min[0] + trap.button_size,
                    b_min[1] + trap.button_size,
                    b_min[2] + trap.button_size,
                ];

                (0..3).all(|ax| {
                    hit_pt[ax] >= b_min[ax] as f32 - 0.5 && hit_pt[ax] <= b_max[ax] as f32 + 0.5
                })
            })
        };

        if let Some(idx) = trap_idx {
            crate::trap::trigger_trap(server, idx, cn);
        }
    }

    async fn switch_team(&self, ctx: &mut Context, _packet: &channel1::SwitchTeam) {
        ctx.cancel();
    }

    async fn map_vote(&self, ctx: &mut Context, _packet: &channel1::MapVote) {
        ctx.cancel(); // Map rotation is strictly managed by PlayMaps
    }

    async fn spectator(&self, ctx: &mut Context, packet: &channel1::SpectatorClient) {
        let server = ctx.server();
        let maps = server.extra.get::<Arc<RwLock<PlayMaps>>>().unwrap();
        let round_state = maps.read().unwrap().round_state;

        // Prevent self-unspectating during the game (even for masters to prevent cheating the 1-life limit).
        if round_state == RoundState::Playing || round_state == RoundState::MapLoading {
            if !packet.val {
                ctx.cancel();
            }
        }
    }

    async fn get_map(&self, ctx: &mut Context, _packet: &channel1::GetMap) {
        let cn = ctx.cn();
        let server = ctx.server();
        let maps = Arc::clone(server.extra.get::<Arc<RwLock<PlayMaps>>>().unwrap());
        if send_map_to_client(Arc::clone(server), cn, &maps) {
            info!("Sending map rebuild to cn {cn} on N_GETMAP");
            ctx.cancel();
        }
    }
}
