use std::{sync::Arc, time::Duration};

use cube_string::CubeString;
use rand::random;
use sauer_proto::{
    Client,
    channel0::{JumpPad, Position, Teleport},
    channel1::{
        ClientDisconnected, ClientPing, Connect, EditModeClient, EditModeServer, Explode, ExplodeFx,
        GunSelect, InitClient, ItemAck, ItemList, ItemPickup, ItemSpawn, MapChange, Ping, Pong,
        Resume, ServInfo, ShotFx, Shoot, SpawnClient, Suicide, SwitchModel, SwitchName, Text,
        TrySpawn, Welcome,
    },
    data::{ClientResumeState, ClientState, PlayerState},
};
use sauer_server::plugin::{Context, Control, PASS, Plugin};
use tracing::info;

use crate::{
    BasePlugin,
    combat::{gun_damage, gun_damrad},
    items::{apply_item, item_respawn_ms, mode_has_items},
    spawn::{apply_spawn, pick_team},
    state::{ItemState, fresh_player},
};

impl Plugin for BasePlugin {
    // ── Connection lifecycle ──────────────────────────────────────────────────

    async fn connected(&self, ctx: Context<'_>) -> Control {
        ctx.server.send_chan1(
            ctx.cn,
            &ServInfo {
                client_num: ctx.cn as i32,
                protocol: sauer_proto::PROTOCOL_VERSION,
                session_id: random(),
                has_password: false,
                server_desc: self.server_desc.clone(),
                server_auth: CubeString::new(),
            },
        );
        PASS
    }

    async fn disconnected(&self, ctx: Context<'_>) -> Control {
        self.state.lock().unwrap().players.remove(&ctx.cn);
        self.broadcast1(ctx.server, None, &ClientDisconnected { client_num: ctx.cn as i32 });
        PASS
    }

    async fn connect(&self, ctx: Context<'_>, packet: &Connect) -> Control {
        let name = packet.name.clone();
        let model = packet.player_model;
        info!("{name} connected (cn={})", ctx.cn);

        let (resume_states, others, team) = {
            let mut state = self.state.lock().unwrap();

            let team = pick_team(self.mode, &state);

            // Snapshot existing players before inserting the newcomer.
            let resume_states: Vec<ClientResumeState> = state
                .players
                .iter()
                .map(|(&cn, p)| ClientResumeState {
                    client_num: cn as i32,
                    state: p.state,
                    frags: p.frags,
                    flags: p.flags,
                    deaths: p.deaths,
                    quad_millis: 0,
                    player_state: PlayerState {
                        life_sequence: p.life_sequence,
                        health: p.health,
                        max_health: p.max_health,
                        armour: p.armour,
                        armour_type: p.armour_type,
                        gun_select: p.gun,
                        ammo: p.ammo.clone(),
                    },
                })
                .collect();

            let others: Vec<_> = state.players.iter().map(|(&cn, p)| (cn, p.clone())).collect();

            let mut player = fresh_player(name.clone(), model);
            player.team = team.clone();
            state.players.insert(ctx.cn, player);

            (resume_states, others, team)
        };

        // Send welcome sequence to the new player.
        ctx.server.send_chan1(ctx.cn, &Welcome {});
        ctx.server.send_chan1(
            ctx.cn,
            &MapChange { map_name: self.map_name.clone(), mode: self.mode, has_items: true },
        );

        // Tell the new player about everyone else's current state.
        if !resume_states.is_empty() {
            ctx.server.send_chan1(ctx.cn, &Resume { clients: resume_states });
        }

        // Tell existing players about the newcomer.
        self.broadcast1(
            ctx.server,
            Some(ctx.cn),
            &InitClient {
                client_num: ctx.cn as i32,
                name: name.clone(),
                player_model: model,
                team: CubeString::from_str_lossy(&team),
            },
        );

        // Tell the newcomer about each existing player.
        for (other_cn, other) in others {
            ctx.server.send_chan1(
                ctx.cn,
                &InitClient {
                    client_num: other_cn as i32,
                    name: other.name,
                    player_model: other.model,
                    team: CubeString::from_str_lossy(&other.team),
                },
            );
        }

        PASS
    }

    // ── Movement (channel 0) ──────────────────────────────────────────────────

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

    // ── Misc channel-1 ────────────────────────────────────────────────────────

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

    async fn text(&self, ctx: Context<'_>, packet: &Text) -> Control {
        self.broadcast1(
            ctx.server,
            Some(ctx.cn),
            &Client { client_num: ctx.cn as i32, packet: packet.clone() },
        );
        PASS
    }

    async fn switch_name(&self, ctx: Context<'_>, packet: &SwitchName) -> Control {
        if let Some(p) = self.state.lock().unwrap().players.get_mut(&ctx.cn) {
            p.name = packet.name.clone();
        }
        self.broadcast1(
            ctx.server,
            Some(ctx.cn),
            &Client { client_num: ctx.cn as i32, packet: packet.clone() },
        );
        PASS
    }

    async fn switch_model(&self, ctx: Context<'_>, packet: &SwitchModel) -> Control {
        if let Some(p) = self.state.lock().unwrap().players.get_mut(&ctx.cn) {
            p.model = packet.model;
        }
        self.broadcast1(
            ctx.server,
            Some(ctx.cn),
            &Client { client_num: ctx.cn as i32, packet: packet.clone() },
        );
        PASS
    }

    async fn gun_select(&self, ctx: Context<'_>, packet: &GunSelect) -> Control {
        if let Some(p) = self.state.lock().unwrap().players.get_mut(&ctx.cn) {
            p.gun = packet.gun;
        }
        self.broadcast1(
            ctx.server,
            Some(ctx.cn),
            &Client { client_num: ctx.cn as i32, packet: packet.clone() },
        );
        PASS
    }

    async fn edit_mode(&self, ctx: Context<'_>, packet: &EditModeClient) -> Control {
        if let Some(p) = self.state.lock().unwrap().players.get_mut(&ctx.cn) {
            p.state = if packet.val { ClientState::Editing } else { ClientState::Alive };
        }
        self.broadcast1(
            ctx.server,
            Some(ctx.cn),
            &Client {
                client_num: ctx.cn as i32,
                packet: EditModeServer { is_editing: packet.val },
            },
        );
        PASS
    }

    // ── Spawning ──────────────────────────────────────────────────────────────

    async fn try_spawn(&self, ctx: Context<'_>, _: &TrySpawn) -> Control {
        {
            let mut state = self.state.lock().unwrap();
            if let Some(p) = state.players.get_mut(&ctx.cn) {
                apply_spawn(self.mode, p);
            }
        }
        // Broadcast the new spawn state so all clients update their HUDs.
        self.broadcast_spawn_state(ctx.server, ctx.cn);
        PASS
    }

    async fn spawn_client(&self, ctx: Context<'_>, packet: &SpawnClient) -> Control {
        // Client confirms spawn.  Update their selected gun (they may have switched
        // between receiving SpawnState and sending SpawnClient).
        if let Some(p) = self.state.lock().unwrap().players.get_mut(&ctx.cn) {
            p.gun = packet.gun_select;
        }
        PASS
    }

    // ── Combat ────────────────────────────────────────────────────────────────

    async fn suicide(&self, ctx: Context<'_>, _: &Suicide) -> Control {
        self.kill(ctx.server, ctx.cn, ctx.cn);
        PASS
    }

    async fn shoot(&self, ctx: Context<'_>, packet: &Shoot) -> Control {
        // Broadcast the shot visual to everyone except the shooter.
        self.broadcast1(
            ctx.server,
            Some(ctx.cn),
            &ShotFx {
                client_num: ctx.cn as i32,
                gun: packet.gun,
                id: packet.id,
                from: packet.from,
                to: packet.to,
            },
        );

        // Apply damage for each reported hitscan hit.
        // hit.rays encodes how many pellets connected (1 for most guns, up to 10 for SG).
        let base = gun_damage(packet.gun);
        for hit in &packet.hits {
            self.do_damage(ctx.server, hit.target_cn as u32, ctx.cn, base * hit.rays.max(1));
        }
        PASS
    }

    async fn explode(&self, ctx: Context<'_>, packet: &Explode) -> Control {
        // Broadcast the explosion visual to everyone except the shooter.
        self.broadcast1(
            ctx.server,
            Some(ctx.cn),
            &ExplodeFx { client_num: ctx.cn as i32, gun: packet.gun, id: packet.id },
        );

        // For splash weapons, apply distance-attenuated damage.
        // hit.dist is the distance from the blast centre; hit.rays is not used for splash.
        let base = gun_damage(packet.gun);
        let damrad = gun_damrad(packet.gun);
        for hit in &packet.hits {
            let scale = (1.0 - (hit.dist / damrad).clamp(0.0, 1.0)) as f32;
            let damage = ((base as f32) * scale).max(1.0) as i32;
            self.do_damage(ctx.server, hit.target_cn as u32, ctx.cn, damage);
        }
        PASS
    }

    // ── Items ─────────────────────────────────────────────────────────────────

    async fn item_list(&self, ctx: Context<'_>, packet: &ItemList) -> Control {
        if !mode_has_items(self.mode) {
            return PASS;
        }

        let mut state = self.state.lock().unwrap();

        if state.items.is_empty() {
            // First client after a map load: register items from their list.
            for item in &packet.items {
                let idx = item.item_index as usize;
                if state.items.len() <= idx {
                    state.items.resize_with(idx + 1, || ItemState {
                        item_type: sauer_proto::data::ItemType::Health,
                        taken: false,
                    });
                }
                state.items[idx] = ItemState { item_type: item.item_type, taken: false };
            }
        } else {
            // Subsequent client: inform them which items are already taken.
            for (idx, item) in state.items.iter().enumerate() {
                if item.taken {
                    ctx.server.send_chan1(
                        ctx.cn,
                        &ItemAck { item_index: idx as i32, client_num: -1 },
                    );
                }
            }
        }
        PASS
    }

    async fn item_pickup(&self, ctx: Context<'_>, packet: &ItemPickup) -> Control {
        if !mode_has_items(self.mode) {
            return PASS;
        }

        let idx = packet.item_index as usize;
        let respawn_ms = {
            let mut state = self.state.lock().unwrap();
            let item = match state.items.get_mut(idx) {
                Some(i) if !i.taken => i,
                _ => return PASS, // invalid index or already taken
            };
            item.taken = true;
            let item_type = item.item_type;
            // Drop the `item` borrow before borrowing players.
            let respawn_ms = item_respawn_ms(item_type);
            if let Some(p) = state.players.get_mut(&ctx.cn) {
                apply_item(p, item_type);
            }
            respawn_ms
        };

        // Acknowledge the pickup to all clients (triggers the pickup animation).
        self.broadcast1(
            ctx.server,
            None,
            &ItemAck { item_index: idx as i32, client_num: ctx.cn as i32 },
        );

        // Schedule the item to reappear after its respawn delay.
        let state_ref = Arc::clone(&self.state);
        let server_ref = Arc::clone(ctx.server);
        tokio::spawn(async move {
            tokio::time::sleep(Duration::from_millis(respawn_ms)).await;

            state_ref.lock().unwrap().items[idx].taken = false;

            let cns: Vec<u32> = state_ref.lock().unwrap().players.keys().copied().collect();
            let pkt = ItemSpawn { item_index: idx as i32 };
            for cn in cns {
                server_ref.send_chan1(cn, &pkt);
            }
        });

        PASS
    }
}
