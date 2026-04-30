use crate::map_state::{DeathrunState, MapState, Trap, get_server_state};
use crate::trap::{StoredTrap, TRAP_STORE_VERSION, trigger_trap};
use cube_string::IntoCubeStringLossy;
use rand::seq::SliceRandom;
use sauer_proto::{
    Client, channel1,
    data::{EntityType, GameMode, Orient, SelInfo, SetTeamReason, Vec3},
};
use sauer_server::{
    Server,
    plugin::{Context, Plugin},
};
use std::collections::HashMap;
use std::sync::{Arc, RwLock};
use tokio::time::{Duration, sleep};
use tracing::{info, warn};

pub struct LoadedMap {
    pub state: MapState,
    pub traps: Vec<Trap>,
}

pub struct PlayMaps {
    pub maps: HashMap<String, LoadedMap>,
    pub current_map: Option<String>,
    pub evil_cn: Option<u32>,
}

impl Default for PlayMaps {
    fn default() -> Self {
        Self {
            maps: HashMap::new(),
            current_map: None,
            evil_cn: None,
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
        if self.current_map.is_none() {
            self.current_map = Some(name.clone());
        }
        self.maps.insert(name, LoadedMap { state, traps });
        Ok(())
    }

    pub fn remove_map(&mut self, name: &str) {
        self.maps.remove(name);
        if self.current_map.as_deref() == Some(name) {
            self.current_map = self.maps.keys().next().cloned();
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
}

pub struct DeathrunPlay {
    maps: Arc<RwLock<PlayMaps>>,
}

impl DeathrunPlay {
    pub fn new() -> Self {
        Self {
            maps: Arc::new(RwLock::new(PlayMaps::default())),
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

/// Sends the current map to a single client using the chunked clipboard-paste visual rebuild.
/// Spawns an async task so the caller returns immediately.
fn send_map_to_client(server: Arc<Server>, cn: u32, maps: &Arc<RwLock<PlayMaps>>) -> bool {
    let (world_size, root, entities, vslots) = {
        let lock = maps.read().unwrap();
        let name = match &lock.current_map {
            Some(n) => n,
            None => return false,
        };
        let loaded = match lock.maps.get(name) {
            Some(m) => m,
            None => return false,
        };
        (
            loaded.state.world.size,
            loaded.state.world.root.clone(),
            loaded.state.world.entities.clone(),
            loaded.state.meta.vslots.clone(),
        )
    };

    tokio::spawn(async move {
        // 1. Set correct world size on the client.
        server.send_chan1(
            cn,
            &Client {
                client_num: cn as i32,
                packet: channel1::NewMap { size: world_size },
            },
        );

        // 2. Delete the entire world.
        server.send_chan1(
            cn,
            &Client {
                client_num: cn as i32,
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

        // 3. Clear entities.
        for i in 0..entities.len() {
            server.send_chan1(
                cn,
                &Client {
                    client_num: cn as i32,
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

        // 4. Collect chunks and paste them one by one with a short delay for the visual effect.
        let mut chunks = Vec::new();
        crate::recreate::collect_chunks(&root, [0, 0, 0], world_size, world_size / 8, &mut chunks);
        chunks.shuffle(&mut rand::thread_rng());

        for (pos, size, node) in chunks {
            let (unpack_len, data) = sauer_map::clipboard::generate_clipboard(&node, size, &vslots);

            server.send_chan1(
                cn,
                &channel1::ClipboardServer {
                    client_num: cn as i32,
                    unpack_len,
                    data,
                },
            );
            server.send_chan1(
                cn,
                &Client {
                    client_num: cn as i32,
                    packet: channel1::PasteServer {
                        sel: SelInfo {
                            o: Vec3 {
                                x: pos[0],
                                y: pos[1],
                                z: pos[2],
                            },
                            s: Vec3 { x: 1, y: 1, z: 1 },
                            grid: size,
                            orient: Orient::Left,
                            cx: 0,
                            cxs: 1,
                            cy: 0,
                            cys: 1,
                            corner: 0,
                        },
                    },
                },
            );

            sleep(Duration::from_millis(5)).await;
        }

        sleep(Duration::from_millis(500)).await;

        // 5. Restore entities.
        for (i, ent) in entities.into_iter().enumerate() {
            let ent_type = ent.ent_type;
            server.send_chan1(
                cn,
                &Client {
                    client_num: cn as i32,
                    packet: channel1::EditEnt {
                        ent_id: i as i32,
                        pos: Vec3 {
                            x: ent.position[0],
                            y: ent.position[1],
                            z: ent.position[2],
                        },
                        type_: ent_type,
                        attrs: ent.attrs.map(|x| x as i32),
                    },
                },
            );
        }
    });

    true
}

fn assign_teams(server: &Server, evil_cn: u32) {
    let good = "good".lossy_cubestring();
    let evil = "evil".lossy_cubestring();
    for cn in server.client_cns() {
        if cn == evil_cn {
            server.set_team(cn, evil.clone(), SetTeamReason::Forced);
        } else {
            server.set_team(cn, good.clone(), SetTeamReason::Forced);
        }
    }
}

fn start_round(server: &Server, maps: &Arc<RwLock<PlayMaps>>) {
    let cns = server.client_cns();
    if cns.is_empty() {
        return;
    }
    let &evil_cn = cns.choose(&mut rand::thread_rng()).unwrap();

    let (_, new_state) = {
        let mut lock = maps.write().unwrap();
        lock.evil_cn = Some(evil_cn);
        lock.make_active_state()
    };
    *get_server_state(server).write().unwrap() = new_state;

    assign_teams(server, evil_cn);
    info!("Round started, evil player is cn {evil_cn}");
}

impl Plugin for DeathrunPlay {
    async fn init(&self, server: &mut Server) {
        *server.map_name.write().unwrap() = "".lossy_cubestring();
        *server.game_mode.write().unwrap() = GameMode::TeamPlay;

        let (_, state) = self.maps.read().unwrap().make_active_state();
        server.extra.insert(RwLock::new(state));
        server.extra.insert(Arc::clone(&self.maps));
    }

    async fn on_client_welcomed(&self, ctx: &mut Context) {
        let cn = ctx.cn();
        let server = ctx.server();

        let maps = Arc::clone(server.extra.get::<Arc<RwLock<PlayMaps>>>().unwrap());
        send_map_to_client(Arc::clone(server), cn, &maps);

        let evil_cn = maps.read().unwrap().evil_cn;
        if evil_cn.is_some() {
            server.set_team(cn, "good".lossy_cubestring(), SetTeamReason::Forced);
        } else {
            start_round(server, &maps);
        }
    }

    async fn disconnected(&self, ctx: &mut Context) {
        let cn = ctx.cn();
        let server = ctx.server();
        let maps = Arc::clone(server.extra.get::<Arc<RwLock<PlayMaps>>>().unwrap());

        let was_evil = {
            let mut lock = maps.write().unwrap();
            if lock.evil_cn == Some(cn) {
                lock.evil_cn = None;
                true
            } else {
                false
            }
        };

        if was_evil {
            let remaining: Vec<u32> = server
                .client_cns()
                .into_iter()
                .filter(|&c| c != cn)
                .collect();
            if let Some(&new_evil) = remaining.choose(&mut rand::thread_rng()) {
                maps.write().unwrap().evil_cn = Some(new_evil);
                assign_teams(server, new_evil);
                info!("Evil cn {cn} left, new evil is cn {new_evil}");
            } else {
                info!("Evil cn {cn} left, no players remaining");
            }
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
            let state = get_server_state(server).read().unwrap();
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
            trigger_trap(server, idx, cn);
        }
    }

    async fn switch_team(&self, ctx: &mut Context, _packet: &channel1::SwitchTeam) {
        ctx.cancel();
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
