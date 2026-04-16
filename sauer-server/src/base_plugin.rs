use std::{
    collections::HashMap,
    sync::{Arc, RwLock},
};

use cube_string::{CubeString, IntoCubeStringLossy};
use rand::random;
use sauer_proto::{
    Client, Encode,
    channel0::{JumpPad, Position, Teleport},
    channel1::{ClientDisconnected, InitClient, MapChange, ServInfo, SpawnState, Text, Welcome},
    data::{ArmourType, GameMode, Gun, PlayerAmmo, PlayerModel, PlayerState},
    route::{Chan0, Chan1, S2C},
};
use tracing::info;

use crate::{
    Server,
    plugin::{Context, Control, PASS, Plugin},
};

/// A plugin implementing the bare minimum functionality which you would expect in a sauerbraten server
#[derive(Clone, Debug)]
pub struct BasePlugin {
    data: Arc<RwLock<BasePluginData>>,
}

#[derive(Clone, Debug, PartialEq)]
struct BasePluginData {
    server_desc: CubeString,
    players: HashMap<u32, Player>,
}

#[derive(Clone, Debug, PartialEq)]
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

    async fn broadcast1<T: Encode<S2C<Chan1>>>(
        &self,
        server: &Server,
        except: Option<u32>,
        packet: &T,
    ) {
        let cns: Vec<_> = self
            .data
            .read()
            .unwrap()
            .players
            .keys()
            .copied()
            .filter(|cn| Some(*cn) != except)
            .collect();

        for cn in cns {
            server.get_client(cn).send_chan1(packet).await;
        }
    }
    async fn broadcast0<T: Encode<S2C<Chan0>>>(
        &self,
        server: &Server,
        except: Option<u32>,
        packet: &T,
    ) {
        let cns: Vec<_> = self
            .data
            .read()
            .unwrap()
            .players
            .keys()
            .copied()
            .filter(|cn| Some(*cn) != except)
            .collect();

        for cn in cns {
            server.get_client(cn).send_chan0(packet).await;
        }
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
    async fn disconnected(&self, ctx: Context<'_>) -> Control {
        self.data.write().unwrap().players.remove(&ctx.cn);

        // notify all other connected players
        self.broadcast1(
            ctx.server,
            None,
            &ClientDisconnected {
                client_num: ctx.cn as i32,
            },
        )
        .await;

        PASS
    }
    async fn connect(&self, ctx: Context<'_>, packet: &sauer_proto::channel1::Connect) -> Control {
        let name = packet.name.clone();
        let model = packet.player_model;

        info!("{name} connected");

        self.data.write().unwrap().players.insert(
            ctx.cn,
            Player {
                name: name.clone(),
                model,
            },
        );

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

        // notify existing players of the new player
        self.broadcast1(
            ctx.server,
            Some(ctx.cn),
            &InitClient {
                client_num: ctx.cn as i32,
                name: name.clone(),
                player_model: model,
                team: CubeString::new(),
            },
        )
        .await;

        // notify the new player of existing players
        let others: Vec<_> = self
            .data
            .read()
            .unwrap()
            .players
            .iter()
            .filter(|(cn, _)| **cn != ctx.cn)
            .map(|(cn, p)| (*cn, p.clone()))
            .collect();
        for (other_cn, other) in others {
            ctx.server
                .get_client(ctx.cn)
                .send_chan1(&InitClient {
                    client_num: other_cn as i32,
                    name: other.name,
                    player_model: other.model,
                    team: CubeString::new(),
                })
                .await;
        }

        PASS
    }
    async fn pos(&self, ctx: Context<'_>, packet: &Position) -> Control {
        self.broadcast0(ctx.server, Some(ctx.cn), packet).await;

        PASS
    }
    async fn teleport(&self, ctx: Context<'_>, packet: &Teleport) -> Control {
        self.broadcast0(ctx.server, Some(ctx.cn), packet).await;

        PASS
    }
    async fn jumppad(&self, ctx: Context<'_>, packet: &JumpPad) -> Control {
        self.broadcast0(ctx.server, Some(ctx.cn), packet).await;

        PASS
    }

    async fn text(&self, ctx: Context<'_>, packet: &Text) -> Control {
        let packet = Client {
            client_num: ctx.cn as i32,
            packet: packet.clone(),
        };
        self.broadcast1(ctx.server, Some(ctx.cn), &packet).await;

        PASS
    }
}
