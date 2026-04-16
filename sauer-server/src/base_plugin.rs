use std::{
    collections::HashMap,
    sync::{Arc, RwLock},
};

use cube_string::{CubeString, IntoCubeStringLossy};
use rand::random;
use sauer_proto::{
    channel1::{MapChange, ServInfo, SpawnState, Welcome},
    data::{ArmourType, GameMode, Gun, PlayerAmmo, PlayerModel, PlayerState},
};
use tracing::info;

use crate::plugin::{Context, Control, PASS, Plugin};

/// A plugin implementing the bare minimum functionality which you would expect in a sauerbraten server
pub struct BasePlugin {
    data: Arc<RwLock<BasePluginData>>,
}

struct BasePluginData {
    server_desc: CubeString,
    players: HashMap<u32, Player>,
}

struct Player {
    name: CubeString,
    model: PlayerModel,
}

impl BasePlugin {
    pub fn new() -> Self {
        Self {
            data: Arc::new(RwLock::new(BasePluginData::new())),
        }
    }
    pub fn set_desc(&self, new_desc: CubeString) {
        self.data.write().unwrap().server_desc = new_desc;
    }
}

impl BasePluginData {
    fn new() -> Self {
        Self {
            server_desc: CubeString::builder()
                .color(cube_string::Color::Magenta)
                .push_str("sauer")
                .color(cube_string::Color::White)
                .push_str("-")
                .color(cube_string::Color::Yellow)
                .push_str("rs")
                .build(),
            players: HashMap::new(),
        }
    }
}

impl Plugin for BasePlugin {
    async fn connected(&self, ctx: Context<'_>) -> Control {
        let server_desc = self.data.read().unwrap().server_desc.clone();

        ctx.server
            .get_client(ctx.cn)
            .send_chan1(&ServInfo {
                client_num: ctx.cn as i32,
                protocol: sauer_proto::PROTOCOL_VERSION,
                session_id: random(), // used for crypto challenges to mitigate replay attacks?
                has_password: false,
                server_desc,
                server_auth: CubeString::new(),
            })
            .await;

        PASS
    }
    async fn connect(&self, ctx: Context<'_>, packet: &sauer_proto::channel1::Connect) -> Control {
        let name = packet.name.clone();
        let model = packet.player_model;

        info!("{name} connected");

        self.data
            .write()
            .unwrap()
            .players
            .insert(ctx.cn, Player { name, model });

        ctx.server.get_client(ctx.cn).send_chan1(&Welcome {}).await;
        ctx.server
            .get_client(ctx.cn)
            .send_chan1(&MapChange {
                map_name: "complex".lossy_cubestring(),
                mode: GameMode::CoopEdit,
                has_items: true,
            })
            .await;

        ctx.server
            .get_client(ctx.cn)
            .send_chan1(&SpawnState {
                client_num: ctx.cn as i32,
                state: PlayerState {
                    life_sequence: 0,
                    health: 100,
                    max_health: 100,
                    armour: 0,
                    armour_type: ArmourType::Yellow,
                    gun_select: Gun::Rifle,
                    ammo: PlayerAmmo {
                        rifle: 9999,
                        shotgun: 0,
                        chaingun: 0,
                        rocket_launcher: 0,
                        grenade_launcher: 0,
                        pistol: 0,
                    },
                },
            })
            .await;

        PASS
    }
}
