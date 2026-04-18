use std::{
    collections::HashMap,
    sync::{Arc, RwLock},
};

use cube_string::{CubeString, IntoCubeStringLossy};
use rand::random;
use sauer_proto::{
    Client,
    channel0::{JumpPad, Position, Teleport},
    channel1::{
        ClientDisconnected, ClientPing, Damage, Died, InitClient, MapChange, Ping, Pong, ServInfo,
        Shoot, ShotFx, SpawnState, Suicide, SwitchModel, SwitchName, Text, TrySpawn, Welcome,
    },
    data::{ArmourType, ClientState, GameMode, Gun, PlayerAmmo, PlayerModel, PlayerState},
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
    state: ClientState,
    health: i32,
    frags: i32,
    life_sequence: i32,
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

    fn broadcast1<T: sauer_proto::Encode<sauer_proto::route::S2C<sauer_proto::route::Chan1>>>(
        &self,
        server: &Server,
        except: Option<u32>,
        packet: &T,
    ) {
        let data = self.data.read().unwrap();
        for &cn in data.players.keys() {
            if Some(cn) != except {
                server.send_chan1(cn, packet);
            }
        }
    }

    fn broadcast0<T: sauer_proto::Encode<sauer_proto::route::S2C<sauer_proto::route::Chan0>>>(
        &self,
        server: &Server,
        except: Option<u32>,
        packet: &T,
    ) {
        let data = self.data.read().unwrap();
        for &cn in data.players.keys() {
            if Some(cn) != except {
                server.send_chan0(cn, packet);
            }
        }
    }

    fn kill(&self, server: &Server, victim_cn: u32, actor_cn: u32) {
        let actor_frags = {
            let mut data = self.data.write().unwrap();

            data.players.get_mut(&victim_cn).unwrap().state = ClientState::Dead;
            data.players.get_mut(&victim_cn).unwrap().life_sequence += 1;

            if victim_cn != actor_cn {
                data.players.get_mut(&actor_cn).unwrap().frags += 1;
            }
            data.players[&actor_cn].frags
        };

        self.broadcast1(
            server,
            None,
            &Died {
                victim_cn: victim_cn as i32,
                actor_cn: actor_cn as i32,
                actor_frags,
                team_frags: 0,
            },
        );
    }

    fn do_damage(&self, server: &Server, victim_cn: u32, actor_cn: u32, damage: i32) {
        let new_health = {
            let mut data = self.data.write().unwrap();
            let victim = match data.players.get_mut(&victim_cn) {
                Some(p) => p,
                None => return,
            };
            victim.health -= damage;
            victim.health
        };

        self.broadcast1(
            server,
            None,
            &Damage {
                target_cn: victim_cn as i32,
                actor_cn: actor_cn as i32,
                damage,
                armour: 0,
                health: new_health,
            },
        );

        if new_health <= 0 {
            self.kill(server, victim_cn, actor_cn);
        }
    }

    fn send_spawn_state(&self, server: &Server, cn: u32) {
        let data = self.data.read().unwrap();
        let player = data.players.get(&cn).unwrap();

        self.broadcast1(
            server,
            None,
            &SpawnState {
                client_num: cn as i32,
                state: PlayerState {
                    life_sequence: player.life_sequence,
                    health: player.health,
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
            },
        );
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

        ctx.server.send_chan1(
            ctx.cn,
            &ServInfo {
                client_num: ctx.cn as i32,
                protocol: sauer_proto::PROTOCOL_VERSION,
                session_id: random(),
                has_password: false,
                server_desc,
                server_auth: CubeString::new(),
            },
        );

        PASS
    }

    async fn disconnected(&self, ctx: Context<'_>) -> Control {
        self.data.write().unwrap().players.remove(&ctx.cn);

        self.broadcast1(
            ctx.server,
            None,
            &ClientDisconnected {
                client_num: ctx.cn as i32,
            },
        );

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
                state: ClientState::Alive,
                health: 100,
                frags: 0,
                life_sequence: 0,
            },
        );

        ctx.server.send_chan1(ctx.cn, &Welcome {});
        ctx.server.send_chan1(
            ctx.cn,
            &MapChange {
                map_name: "complex".lossy_cubestring(),
                mode: GameMode::CoopEdit,
                has_items: true,
            },
        );

        self.send_spawn_state(ctx.server, ctx.cn);

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
        );

        // notify the new player of existing players
        let others: Vec<_> = self
            .data
            .read()
            .unwrap()
            .players
            .iter()
            .filter(|&(&cn, _)| cn != ctx.cn)
            .map(|(&cn, p)| (cn, p.clone()))
            .collect();

        for (other_cn, other) in others {
            ctx.server.send_chan1(
                ctx.cn,
                &InitClient {
                    client_num: other_cn as i32,
                    name: other.name,
                    player_model: other.model,
                    team: CubeString::new(),
                },
            );
        }

        PASS
    }

    async fn pos(&self, ctx: Context<'_>, packet: &Position) -> Control {
        self.broadcast0(ctx.server, Some(ctx.cn), packet);
        PASS
    }

    async fn teleport(&self, ctx: Context<'_>, packet: &Teleport) -> Control {
        self.broadcast0(ctx.server, Some(ctx.cn), packet);
        PASS
    }

    async fn jumppad(&self, ctx: Context<'_>, packet: &JumpPad) -> Control {
        self.broadcast0(ctx.server, Some(ctx.cn), packet);
        PASS
    }

    async fn text(&self, ctx: Context<'_>, packet: &Text) -> Control {
        self.broadcast1(
            ctx.server,
            Some(ctx.cn),
            &Client { client_num: ctx.cn as i32, packet: packet.clone() },
        );
        PASS
    }

    async fn switch_model(&self, ctx: Context<'_>, packet: &SwitchModel) -> Control {
        self.data.write().unwrap().players.get_mut(&ctx.cn).unwrap().model = packet.model;
        self.broadcast1(
            ctx.server,
            Some(ctx.cn),
            &Client { client_num: ctx.cn as i32, packet: packet.clone() },
        );
        PASS
    }

    async fn switch_name(&self, ctx: Context<'_>, packet: &SwitchName) -> Control {
        self.data.write().unwrap().players.get_mut(&ctx.cn).unwrap().name = packet.name.clone();
        self.broadcast1(
            ctx.server,
            Some(ctx.cn),
            &Client { client_num: ctx.cn as i32, packet: packet.clone() },
        );
        PASS
    }

    async fn ping(&self, ctx: Context<'_>, packet: &Ping) -> Control {
        ctx.server.send_chan1(ctx.cn, &Pong { ping: packet.time });
        PASS
    }

    async fn client_ping(&self, ctx: Context<'_>, packet: &ClientPing) -> Control {
        self.broadcast1(
            ctx.server,
            Some(ctx.cn),
            &Client { client_num: ctx.cn as i32, packet: packet.clone() },
        );
        PASS
    }

    async fn shoot(&self, ctx: Context<'_>, packet: &Shoot) -> Control {
        self.broadcast1(
            ctx.server,
            Some(ctx.cn),
            &ShotFx {
                client_num: ctx.cn as i32,
                gun: packet.gun,
                id: packet.id,
                to: packet.to,
                from: packet.from,
            },
        );

        for hit in &packet.hits {
            self.do_damage(ctx.server, hit.target_cn as u32, ctx.cn, 1000);
        }

        PASS
    }

    async fn suicide(&self, ctx: Context<'_>, _: &Suicide) -> Control {
        self.kill(ctx.server, ctx.cn, ctx.cn);
        PASS
    }

    async fn try_spawn(&self, ctx: Context<'_>, _: &TrySpawn) -> Control {
        {
            let mut data = self.data.write().unwrap();
            let player = data.players.get_mut(&ctx.cn).unwrap();
            player.state = ClientState::Alive;
            player.health = 100;
        }
        self.send_spawn_state(ctx.server, ctx.cn);
        PASS
    }
}
