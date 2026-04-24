use crate::{
    Server,
    handle_client::handle_client,
    items::spawntime,
    peer::Peer,
    players::{clear_player, init_player},
    plugin::{Context, Plugin},
};
use cube_string::{CubeString, IntoCubeStringLossy};
use enet_rs::{Host, HostConfig};
use sauer_proto::{
    channel1::{ItemSpawn, ServInfo},
    data::{GameMode, MasterMode},
};
use std::{
    net::{Ipv4Addr, SocketAddr, SocketAddrV4},
    sync::Arc,
};
use tracing::error;

pub struct ServerBuilder<P = ()> {
    bind_addr: SocketAddr,
    max_peers: usize,
    server_desc: CubeString,
    server_auth: CubeString,
    has_password: bool,
    map_name: CubeString,
    game_mode: GameMode,
    master_mode: MasterMode,
    plugin: P,
}

impl ServerBuilder {
    pub fn new() -> Self {
        ServerBuilder {
            bind_addr: SocketAddrV4::new(Ipv4Addr::UNSPECIFIED, 28785).into(),
            max_peers: 128,
            server_desc: "sauer-rs".lossy_cubestring(),
            server_auth: CubeString::new(),
            has_password: false,
            map_name: "complex".lossy_cubestring(),
            game_mode: GameMode::Ffa,
            master_mode: MasterMode::Open,
            plugin: (),
        }
    }
}

impl<P: Plugin> ServerBuilder<P> {
    pub fn bind_addr(mut self, addr: impl Into<SocketAddr>) -> Self {
        self.bind_addr = addr.into();
        self
    }

    pub fn max_peers(mut self, peers: usize) -> Self {
        self.max_peers = peers;
        self
    }

    pub fn server_desc(mut self, desc: impl Into<CubeString>) -> Self {
        self.server_desc = desc.into();
        self
    }

    pub fn server_auth(mut self, auth: impl Into<CubeString>) -> Self {
        self.server_auth = auth.into();
        self
    }

    pub fn has_password(mut self, val: bool) -> Self {
        self.has_password = val;
        self
    }

    pub fn map_name(mut self, name: impl Into<CubeString>) -> Self {
        self.map_name = name.into();
        self
    }

    pub fn game_mode(mut self, mode: GameMode) -> Self {
        self.game_mode = mode;
        self
    }

    pub fn master_mode(mut self, mode: MasterMode) -> Self {
        self.master_mode = mode;
        self
    }

    pub fn plugin<T: Plugin>(self, plugin: T) -> ServerBuilder<(P, T)> {
        ServerBuilder {
            bind_addr: self.bind_addr,
            max_peers: self.max_peers,
            server_desc: self.server_desc,
            server_auth: self.server_auth,
            has_password: self.has_password,
            map_name: self.map_name,
            game_mode: self.game_mode,
            master_mode: self.master_mode,
            plugin: (self.plugin, plugin),
        }
    }

    pub async fn run(self) -> enet_rs::Result<()> {
        let mut host = Host::bind(
            self.bind_addr,
            HostConfig {
                max_peers: self.max_peers,
                channel_count: sauer_proto::CHANNEL_COUNT as u8,
                use_checksum: false,
                incoming_bandwidth: 0,
                outgoing_bandwidth: 0,
            },
        )
        .await?;

        let plugin = Arc::new(self.plugin);
        let mut server = Server::new(
            self.server_desc,
            self.server_auth,
            self.has_password,
            self.map_name,
            self.game_mode,
            self.master_mode,
        );
        plugin.init(&mut server).await;

        let server = Arc::new(server);
        plugin.start(&server).await;

        // Start the background tick task: decrements quad timers and
        // advances item respawn countdowns, broadcasting N_ITEMSPAWN when an item fires.
        Server::start_tick_task(Arc::clone(&server));

        let mut next_cn = 0u32;

        while let Some(peer) = host.accept().await {
            let (tx, rx) = peer.split();

            let cn = next_cn;
            next_cn = (next_cn + 1) % i32::MAX as u32;

            server.clients.insert(cn, Peer::new(tx));

            let plugin = Arc::clone(&plugin);
            let server = Arc::clone(&server);
            tokio::spawn(async move {
                let mut ctx = Context { server: Arc::clone(&server), cn, cancelled: false };

                plugin.connected(&mut ctx).await;

                let client_connected = !ctx.cancelled;
                if client_connected {
                    server.on_client_connected(cn);

                    if let Err(e) = handle_client(&server, &plugin, rx, cn).await {
                        error!("handling peer: {e}");
                    }
                }

                ctx.cancelled = false;
                plugin.disconnected(&mut ctx).await;

                if client_connected {
                    server.on_client_disconnected(cn);
                }

                server.clients.remove(&cn);
            });
        }

        Ok(())
    }
}

impl Default for ServerBuilder<()> {
    fn default() -> Self {
        Self::new()
    }
}

impl Server {
    /// Called when a new ENet peer connects (before the packet loop starts).
    pub(crate) fn on_client_connected(&self, cn: u32) {
        let session_id = init_player(&self.players, cn);

        let has_password = *self.has_password.read().unwrap();
        self.send_chan1(
            cn,
            &ServInfo {
                client_num: cn as i32,
                protocol: sauer_proto::PROTOCOL_VERSION,
                session_id,
                has_password,
                server_desc: self.server_desc.read().unwrap().clone(),
                server_auth: self.server_auth.read().unwrap().clone(),
            },
        );
    }

    /// Called after the packet loop ends (peer disconnected).
    pub(crate) fn on_client_disconnected(&self, cn: u32) {
        use sauer_proto::{channel1::ClientDisconnected, data::Privilege};

        let had_privilege = self
            .players
            .try_get(cn)
            .map_or(false, |p| p.privilege != Privilege::None);

        clear_player(&self.players, cn);

        self.broadcast_chan1(&ClientDisconnected { client_num: cn as i32 });

        if had_privilege {
            self.broadcast_current_master();
        }
    }

    /// Spawns the background task that runs every 100ms:
    /// - decrements quad timers for all players
    /// - advances item respawn countdowns; broadcasts N_ITEMSPAWN when items respawn
    pub(crate) fn start_tick_task(server: Arc<Server>) {
        tokio::spawn(async move {
            let mut interval =
                tokio::time::interval(std::time::Duration::from_millis(100));
            loop {
                interval.tick().await;

                for mut entry in server.players.map.iter_mut() {
                    if entry.quad_millis > 0 {
                        entry.quad_millis = (entry.quad_millis - 100).max(0);
                    }
                }

                let spawning: Vec<usize> = {
                    let mut items = server.items.write().unwrap();
                    if items.notgotitems {
                        continue;
                    }
                    items
                        .list
                        .iter_mut()
                        .enumerate()
                        .filter(|(_, item)| item.spawn_time_ms > 0)
                        .filter_map(|(i, item)| {
                            item.spawn_time_ms -= 100;
                            if item.spawn_time_ms <= 0 {
                                item.spawn_time_ms = 0;
                                item.spawned = true;
                                Some(i)
                            } else {
                                None
                            }
                        })
                        .collect()
                };
                for i in spawning {
                    server.broadcast_chan1(&ItemSpawn { item_index: i as i32 });
                }
            }
        });
    }
}

// ── Base packet handlers (called from handle_client after plugin dispatch) ────

use sauer_proto::{
    Client,
    channel0,
    channel1::{
        self, ClipboardServer, ExplodeFx, ItemAck, RedoServer, ShotFx, SpawnServer, UndoServer,
    },
    channel2,
    data::{ClientState, MasterMode as MM, Privilege, SetTeamReason},
};
use crate::items::{apply_pickup, canpickup, entity_to_item_type, ServerItem};

impl Server {
    pub(crate) fn handle_connect(&self, cn: u32, packet: &channel1::Connect) {
        use tracing::info;
        info!("{} (cn {cn}) connected.", packet.name);

        self.init_name(cn, packet.name.clone());
        self.init_model(cn, packet.player_model);

        let master_mode = self.master_mode();
        match master_mode {
            MM::Private => {
                // A plugin must perform auth and call welcome() manually.
            }
            MM::Locked => {
                self.welcome(cn);
                if self.get_privilege(cn) < Privilege::Master {
                    if let Some(mut p) = self.players.try_get_mut(cn) {
                        p.client_state = ClientState::Spectator;
                    }
                    self.broadcast_chan1(&channel1::SpectatorServer {
                        client_num: cn as i32,
                        is_spectator: true,
                    });
                }
            }
            _ => self.welcome(cn),
        }
    }

    pub(crate) fn handle_try_spawn(&self, cn: u32) {
        self.prepare_spawn(cn);
    }

    pub(crate) fn handle_spawn_client(&self, cn: u32, packet: &channel1::SpawnClient) {
        let spawned_state = {
            let Some(mut p) = self.players.try_get_mut(cn) else { return };
            if p.client_state != ClientState::Dead {
                return;
            }
            let Some(state) = p.state.as_mut() else { return };
            // Sync server life_seq to what the client is using.
            state.life_sequence = packet.life_sequence;
            state.gun_select = packet.gun_select;
            p.client_state = ClientState::Alive;
            p.state.clone()
        };

        if let Some(state) = spawned_state {
            // Broadcast to all OTHER clients; the spawning client already set itself CS_ALIVE.
            self.broadcast_except_chan1(
                cn,
                &Client { client_num: cn as i32, packet: SpawnServer { state } },
            );
        }
    }

    pub(crate) fn handle_spectator(&self, cn: u32, packet: &channel1::SpectatorClient) {
        let target_cn = packet.spectator_cn as u32;
        let has_privilege = self.get_privilege(cn) >= Privilege::Master;
        let locked = self.master_mode() == MM::Locked;

        if !has_privilege && (target_cn != cn || locked) {
            return;
        }

        if let Some(mut p) = self.players.try_get_mut(target_cn) {
            if packet.val {
                p.client_state = ClientState::Spectator;
            } else if p.client_state == ClientState::Spectator {
                p.client_state = ClientState::Dead;
            }
        }

        self.broadcast_chan1(&channel1::SpectatorServer {
            client_num: packet.spectator_cn,
            is_spectator: packet.val,
        });
    }

    pub(crate) fn handle_suicide(&self, cn: u32) {
        self.kill_player(cn);
    }

    pub(crate) fn handle_shoot(&self, cn: u32, packet: &channel1::Shoot) {
        use crate::players::combat::gun_damage_per_ray;

        {
            let Some(mut p) = self.players.try_get_mut(cn) else { return };
            if p.client_state != ClientState::Alive {
                return;
            }
            if let Some(state) = p.state.as_mut() {
                match packet.gun {
                    sauer_proto::data::Gun::Fist => {}
                    sauer_proto::data::Gun::Sg => state.ammo.shotgun -= 1,
                    sauer_proto::data::Gun::Cg => state.ammo.chaingun -= 1,
                    sauer_proto::data::Gun::Rl => state.ammo.rocket_launcher -= 1,
                    sauer_proto::data::Gun::Rifle => state.ammo.rifle -= 1,
                    sauer_proto::data::Gun::Gl => state.ammo.grenade_launcher -= 1,
                    sauer_proto::data::Gun::Pistol => state.ammo.pistol -= 1,
                }
            }
        }

        self.broadcast_except_chan1(
            cn,
            &ShotFx {
                client_num: cn as i32,
                gun: packet.gun,
                id: packet.id,
                from: packet.from,
                to: packet.to,
            },
        );

        for hit in &packet.hits {
            let target_cn = hit.target_cn as u32;
            if !self.check_hit(target_cn, hit.life_sequence) {
                continue;
            }
            let damage = hit.rays * gun_damage_per_ray(packet.gun);
            self.apply_damage(target_cn, cn, packet.gun, damage, hit.dir);
        }
    }

    pub(crate) fn handle_explode(&self, cn: u32, packet: &channel1::Explode) {
        use crate::players::combat::explosion_damage;

        if self.players.get(cn).client_state != ClientState::Alive {
            return;
        }

        self.broadcast_chan1(&ExplodeFx {
            client_num: cn as i32,
            gun: packet.gun,
            id: packet.id,
        });

        for hit in &packet.hits {
            let target_cn = hit.target_cn as u32;
            if !self.check_hit(target_cn, hit.life_sequence) {
                continue;
            }
            let damage = explosion_damage(packet.gun, hit.dist, target_cn == cn);
            self.apply_damage(target_cn, cn, packet.gun, damage, hit.dir);
        }
    }

    pub(crate) fn handle_pos(&self, cn: u32, packet: &channel0::Position) {
        self.set_position(cn, packet.pos);
        self.broadcast_except_chan0(cn, packet);
    }

    pub(crate) fn handle_teleport(&self, cn: u32, packet: &channel0::Teleport) {
        self.broadcast_except_chan0(cn, packet);
    }

    pub(crate) fn handle_jumppad(&self, cn: u32, packet: &channel0::JumpPad) {
        self.broadcast_except_chan0(cn, packet);
    }

    pub(crate) fn handle_text(&self, cn: u32, packet: &channel1::Text) {
        use tracing::info;
        let name = self.players.get(cn).name.clone();
        info!("[CHAT] {name}: {}", packet.text);
        self.broadcast_except_chan1(
            cn,
            &Client { client_num: cn as i32, packet: packet.clone() },
        );
    }

    pub(crate) fn handle_switch_name(&self, cn: u32, packet: &channel1::SwitchName) {
        self.set_name(cn, packet.name.clone());
    }

    pub(crate) fn handle_switch_model(&self, cn: u32, packet: &channel1::SwitchModel) {
        self.set_model(cn, packet.model);
    }

    pub(crate) fn handle_switch_team(&self, cn: u32, packet: &channel1::SwitchTeam) {
        self.set_team(cn, packet.team.clone(), SetTeamReason::Switched);
    }

    pub(crate) fn handle_ping(&self, cn: u32, packet: &channel1::Ping) {
        self.send_chan1(cn, &channel1::Pong { ping: packet.time });
    }

    pub(crate) fn handle_client_ping(&self, cn: u32, packet: &channel1::ClientPing) {
        self.broadcast_except_chan1(
            cn,
            &Client { client_num: cn as i32, packet: packet.clone() },
        );
    }

    pub(crate) fn handle_taunt(&self, cn: u32, packet: &channel1::Taunt) {
        self.broadcast_except_chan1(
            cn,
            &Client { client_num: cn as i32, packet: packet.clone() },
        );
    }

    pub(crate) fn handle_sound(&self, cn: u32, packet: &channel1::Sound) {
        self.broadcast_except_chan1(
            cn,
            &Client { client_num: cn as i32, packet: packet.clone() },
        );
    }

    pub(crate) fn handle_map_upload(&self, cn: u32, data: &bytes::Bytes) {
        use tracing::info;
        *self.map_data.write().unwrap() = Some(data.clone());
        let name = self.players.get(cn).name.clone();
        info!("[MAP] {name} (cn {cn}) uploaded a new map.");
        self.broadcast_chan1(&channel1::ServMsg {
            text: format!("[{name} uploaded the map, use \"/getmap\" to receive it]")
                .lossy_cubestring(),
        });
    }

    pub(crate) fn handle_get_map(&self, cn: u32) {
        let map_bytes = self.map_data.read().unwrap().clone();
        let Some(data) = map_bytes else { return };

        self.send_chan2(cn, &channel2::SendMap { data: data.to_vec() });

        // Resend all players' clipboards so the client has current state after loading the map.
        let clipboard_packets: Vec<(i32, i32, Vec<u8>)> = self
            .players
            .map
            .iter()
            .filter(|e| *e.key() != cn)
            .filter_map(|e| {
                e.value()
                    .clipboard
                    .as_ref()
                    .map(|cb| (*e.key() as i32, cb.unpack_len, cb.data.clone()))
            })
            .collect();
        for (owner_cn, unpack_len, data) in clipboard_packets {
            self.send_chan1(cn, &ClipboardServer { client_num: owner_cn, unpack_len, data });
        }
    }

    /// Relay a coop-edit packet from `cn` to all other clients, but only in CoopEdit mode.
    pub(crate) fn relay_coop<T>(&self, cn: u32, pkt: &T)
    where
        Client<T>: sauer_proto::PacketSequence<sauer_proto::route::S2C<sauer_proto::route::Chan1>>,
        T: Clone,
    {
        if *self.game_mode.read().unwrap() != sauer_proto::data::GameMode::CoopEdit {
            return;
        }
        self.broadcast_except_chan1(cn, &Client { client_num: cn as i32, packet: pkt.clone() });
    }

    pub(crate) fn handle_edit_ent(&self, cn: u32, packet: &channel1::EditEnt) {
        if *self.game_mode.read().unwrap() != sauer_proto::data::GameMode::CoopEdit {
            return;
        }

        let idx = packet.ent_id as usize;
        let new_item = {
            let mut items = self.items.write().unwrap();
            match entity_to_item_type(packet.type_) {
                Some(item_type) => {
                    if items.list.len() <= idx {
                        items.list.resize_with(idx + 1, || ServerItem {
                            item_type,
                            spawned: false,
                            spawn_time_ms: 0,
                        });
                        true
                    } else {
                        let item = &mut items.list[idx];
                        if item.item_type != item_type
                            || (!item.spawned && item.spawn_time_ms == 0)
                        {
                            item.item_type = item_type;
                            item.spawned = false;
                            item.spawn_time_ms = 0;
                            true
                        } else {
                            false
                        }
                    }
                }
                None => {
                    // Entity changed to a non-item type — deactivate the slot.
                    if let Some(item) = items.list.get_mut(idx) {
                        item.spawned = false;
                        item.spawn_time_ms = 0;
                    }
                    false
                }
            }
        };

        // Relay the geometry change to all other coop clients.
        self.broadcast_except_chan1(
            cn,
            &Client { client_num: cn as i32, packet: packet.clone() },
        );

        // Tell all clients (including the editor) that the new item slot is spawned.
        if new_item {
            self.items.write().unwrap().list[idx].spawned = true;
            self.broadcast_chan1(&ItemSpawn { item_index: idx as i32 });
        }
    }

    pub(crate) fn handle_new_map(&self, cn: u32, packet: &channel1::NewMap) {
        if packet.size >= 0 {
            let mut items = self.items.write().unwrap();
            items.reset();
            items.notgotitems = false;
        }
        if *self.game_mode.read().unwrap() != sauer_proto::data::GameMode::CoopEdit {
            return;
        }
        self.broadcast_except_chan1(
            cn,
            &Client { client_num: cn as i32, packet: packet.clone() },
        );
    }

    pub(crate) fn handle_item_list(&self, cn: u32, packet: &channel1::ItemList) {
        let is_spectator = self
            .players
            .try_get(cn)
            .map_or(false, |p| p.client_state == ClientState::Spectator);

        let mut items = self.items.write().unwrap();
        if !items.notgotitems || is_spectator {
            return;
        }
        let num_players = self.clients.len();
        for item in &packet.items {
            let idx = item.item_index as usize;
            while items.list.len() <= idx {
                items.list.push(ServerItem {
                    item_type: item.item_type,
                    spawned: false,
                    spawn_time_ms: 0,
                });
            }
            items.list[idx].item_type = item.item_type;
            if crate::items::delay_spawn(item.item_type) {
                items.list[idx].spawn_time_ms = spawntime(item.item_type, num_players);
                items.list[idx].spawned = false;
            } else {
                items.list[idx].spawned = true;
                items.list[idx].spawn_time_ms = 0;
            }
        }
        items.notgotitems = false;
    }

    pub(crate) fn handle_item_pickup(&self, cn: u32, packet: &channel1::ItemPickup) {
        let idx = packet.item_index as usize;

        let can = {
            let Some(p) = self.players.try_get(cn) else { return };
            if p.client_state != ClientState::Alive {
                return;
            }
            let quad_millis = p.quad_millis;
            p.state.as_ref().map_or(false, |s| {
                self.items
                    .read()
                    .unwrap()
                    .list
                    .get(idx)
                    .map_or(false, |item| item.spawned && canpickup(s, quad_millis, item.item_type))
            })
        };

        if !can {
            self.send_chan1(
                cn,
                &ItemAck { item_index: packet.item_index, client_num: -1 },
            );
            return;
        }

        let num_players = self.clients.len();
        let item_type = {
            let mut items = self.items.write().unwrap();
            let item = &mut items.list[idx];
            item.spawned = false;
            item.spawn_time_ms = spawntime(item.item_type, num_players);
            item.item_type
        };

        if let Some(mut p) = self.players.try_get_mut(cn) {
            let mut quad = p.quad_millis;
            if let Some(state) = p.state.as_mut() {
                apply_pickup(state, &mut quad, item_type);
            }
            p.quad_millis = quad;
        }

        self.broadcast_chan1(&ItemAck {
            item_index: packet.item_index,
            client_num: cn as i32,
        });
    }

    pub(crate) fn handle_undo(&self, cn: u32, packet: &channel1::UndoClient) {
        if *self.game_mode.read().unwrap() != sauer_proto::data::GameMode::CoopEdit {
            return;
        }
        self.broadcast_except_chan1(
            cn,
            &UndoServer {
                client_num: cn as i32,
                unpack_len: packet.unpack_len,
                data: packet.data.clone(),
            },
        );
    }

    pub(crate) fn handle_redo(&self, cn: u32, packet: &channel1::RedoClient) {
        if *self.game_mode.read().unwrap() != sauer_proto::data::GameMode::CoopEdit {
            return;
        }
        self.broadcast_except_chan1(
            cn,
            &RedoServer {
                client_num: cn as i32,
                unpack_len: packet.unpack_len,
                data: packet.data.clone(),
            },
        );
    }

    pub(crate) fn handle_copy(&self, cn: u32, packet: &channel1::CopyClient) {
        if *self.game_mode.read().unwrap() != sauer_proto::data::GameMode::CoopEdit {
            return;
        }
        if let Some(mut p) = self.players.try_get_mut(cn) {
            p.clipboard = None;
        }
        self.broadcast_except_chan1(
            cn,
            &Client {
                client_num: cn as i32,
                packet: channel1::CopyServer { sel: packet.sel.clone() },
            },
        );
    }

    pub(crate) fn handle_paste(&self, cn: u32, packet: &channel1::PasteClient) {
        if *self.game_mode.read().unwrap() != sauer_proto::data::GameMode::CoopEdit {
            return;
        }
        self.broadcast_except_chan1(
            cn,
            &Client {
                client_num: cn as i32,
                packet: channel1::PasteServer { sel: packet.sel.clone() },
            },
        );
    }

    pub(crate) fn handle_clipboard(&self, cn: u32, packet: &channel1::ClipboardClient) {
        use crate::players::ClipboardData;

        let is_spectator = self
            .players
            .try_get(cn)
            .map_or(false, |p| p.client_state == ClientState::Spectator);
        if is_spectator {
            return;
        }

        let pack_len = packet.data.len();
        if pack_len == 0 || pack_len > (1 << 16) || packet.unpack_len <= 0 {
            if let Some(mut p) = self.players.try_get_mut(cn) {
                p.clipboard = None;
            }
            return;
        }

        if let Some(mut p) = self.players.try_get_mut(cn) {
            p.clipboard = Some(ClipboardData {
                unpack_len: packet.unpack_len,
                data: packet.data.clone(),
            });
        }

        self.broadcast_except_chan1(
            cn,
            &ClipboardServer {
                client_num: cn as i32,
                unpack_len: packet.unpack_len,
                data: packet.data.clone(),
            },
        );
    }

    pub(crate) fn handle_master_mode(&self, cn: u32, packet: &channel1::MasterModeClient) {
        if self.get_privilege(cn) >= Privilege::Master {
            self.set_master_mode(packet.mode);
        }
    }

    /// Returns true if `target_cn` is alive with a matching `life_sequence`.
    /// Guards against hitting a player who has already respawned.
    fn check_hit(&self, target_cn: u32, life_sequence: i32) -> bool {
        self.players.try_get(target_cn).map_or(false, |p| {
            p.client_state == ClientState::Alive
                && matches!(p.state.as_ref(), Some(s) if s.life_sequence == life_sequence)
        })
    }
}
