use crate::map_state::{
    DeathrunState, MapState, get_server_state, is_waiting_for_map, with_map_state_mut,
};
use crate::trap::{
    check_and_rebuild_traps, cmd_deltrap, cmd_newtrap, intersects_edit_face, intersects_sel,
};
use crate::ui;
use bytes::Bytes;
use cube_string::{CubeString, IntoCubeStringLossy};
use dashmap::DashMap;
use sauer_proto::{
    Client, channel0,
    channel1::{self, EditMode},
    data::{ClientState, Direction, EntityType, GameMode, PlayerState, Privilege, Vec3},
};
use sauer_server::{
    Server,
    plugin::{Context, Plugin},
};
use std::sync::RwLock;
use tracing::{info, warn};

pub const TRAP_EDIT_PARTICLE_ATTRS: [i16; 5] = [34, 100, 100, 0, 0];
pub const TRAP_EDIT_PARTICLE_YAW_OFFSET: f32 = 30.0;
pub const TRAP_EDIT_PARTICLE_PITCH_OFFSET: f32 = 0.0;
pub const TRAP_EDIT_PARTICLE_MAX_DIST: f32 = 1000.0;

// ─── Packet Memory Tracker ────────────────────────────────────────────────────

fn intercept_packet(ctx: &Context, packet: crate::trap::RecordedPacket) {
    let server = ctx.server();
    let mut state = crate::map_state::get_server_state(server).write().unwrap();
    if let Some(edit) = &mut state.trap_edit {
        if let crate::trap::RecordedPacket::EditEnt(p) = &packet {
            edit.affected_entities.insert(p.ent_id as usize);
        }
    }
}

pub struct DeathrunEdit {
    player_dirs: DashMap<u64, Direction>,
    last_particle_update: DashMap<u64, tokio::time::Instant>,
}

impl DeathrunEdit {
    pub fn new() -> Self {
        Self {
            player_dirs: DashMap::new(),
            last_particle_update: DashMap::new(),
        }
    }
}

impl Plugin for DeathrunEdit {
    async fn init(&self, server: &mut Server) {
        *server.map_name.write().unwrap() = "".lossy_cubestring();
        *server.game_mode.write().unwrap() = GameMode::CoopEdit;
        server.reserve_cn(crate::SERVER_BOT_CN);
        let mut state = DeathrunState::default();
        state.map = Some(MapState::default_empty());
        server.extra.insert(RwLock::new(state));

        server.register_command("#newtrap", "create a trap at the aimed surface", "");
        server.register_command("#deltrap", "remove the trap at the aimed surface", "");
        server.register_command("#settrap", "reset and enter edit mode for a trap", "");
        server.register_command(
            "#edittrap",
            "enter edit mode for a trap without resetting",
            "",
        );
        server.register_command("#endtrap", "finish editing the trap", "");
        server.register_command("#trapinfo", "print info about the trap (size, overhead)", "");
        server.register_command("#traptime", "extend duration (default 2 000ms)", "<ms>");
        server.register_command(
            "#traptimeout",
            "set trap timeout (default 20 000ms)",
            "<ms>",
        );
    }

    async fn pos(&self, ctx: &mut Context, packet: &channel0::Position) {
        let id = ctx.id();
        self.player_dirs.insert(id, packet.dir);

        let server = ctx.server();
        let state = get_server_state(server).read().unwrap();

        if state.trap_edit.is_none() {
            return;
        }

        let now = tokio::time::Instant::now();
        if let Some(mut last) = self.last_particle_update.get_mut(&id) {
            if now.duration_since(*last).as_millis() < 200 {
                return;
            }
            *last = now;
        } else {
            self.last_particle_update.insert(id, now);
        }

        let yaw = (packet.dir.yaw as f32) + TRAP_EDIT_PARTICLE_YAW_OFFSET;
        let yaw_rad = yaw.to_radians();
        let pitch =
            ((packet.dir.pitch as f32) + TRAP_EDIT_PARTICLE_PITCH_OFFSET).clamp(-90.0, 90.0);
        let pitch_rad = pitch.to_radians();

        let forward = [
            -yaw_rad.sin() * pitch_rad.cos(),
            yaw_rad.cos() * pitch_rad.cos(),
            pitch_rad.sin(),
        ];

        let eye_z = packet.pos.z + 14.0;
        let origin = [packet.pos.x, packet.pos.y, eye_z];

        let mut dist = TRAP_EDIT_PARTICLE_MAX_DIST;
        if let Some(map) = state.map.as_ref() {
            if let Some(hit) = sauer_map::raycast::raycast_with_filter(
                &map.world,
                origin,
                forward,
                TRAP_EDIT_PARTICLE_MAX_DIST,
                |c| !c.is_empty() || c.material != sauer_core::Material::default(),
            ) {
                dist = (hit.dist - 1.0).max(1.0); // Pull back slightly from the wall so it doesn't clip
            }
        }
        drop(state); // Drop the lock before sending to network

        let particle_pos = Vec3 {
            x: origin[0] + forward[0] * dist,
            y: origin[1] + forward[1] * dist,
            z: origin[2] + forward[2] * dist,
        };

        let ent_id = {
            let mut state = get_server_state(server).write().unwrap();

            if let Some(id) = state.trap_edit_particle_ent {
                id
            } else {
                let map = match state.map.as_mut() {
                    Some(m) => m,
                    None => return,
                };

                let id = map
                    .world
                    .entities
                    .iter()
                    .position(|e| e.ent_type == ogz_rs::data::EntityType::Empty)
                    .unwrap_or_else(|| {
                        let i = map.world.entities.len();
                        map.world.entities.push(ogz_rs::data::Entity {
                            position: [0.0; 3],
                            attrs: [0; 5],
                            ent_type: ogz_rs::data::EntityType::Empty,
                            extra_data: vec![],
                        });
                        i
                    });

                if let Some(ent) = map.world.entities.get_mut(id) {
                    ent.ent_type = ogz_rs::data::EntityType::Particles;
                    ent.attrs = TRAP_EDIT_PARTICLE_ATTRS;
                }

                state.trap_edit_particle_ent = Some(id);
                id
            }
        };

        let cn = crate::SERVER_BOT_CN as i32;

        let ent = Client {
            client_num: cn,
            packet: channel1::EditEnt {
                ent_id: ent_id as i32,
                pos: particle_pos,
                type_: EntityType::Particles,
                attrs: TRAP_EDIT_PARTICLE_ATTRS.map(|x| x as i32),
            },
        };

        server.send_chan1(id, &ent);
    }

    async fn on_command(&self, ctx: &mut Context, args: &[CubeString]) {
        let cmd = args.get(0).and_then(|s| s.as_str()).unwrap_or("");

        if cmd != "#newtrap"
            && cmd != "#deltrap"
            && cmd != "#settrap"
            && cmd != "#edittrap"
            && cmd != "#endtrap"
            && cmd != "#trapinfo"
            && cmd != "#traptime"
            && cmd != "#traptimeout"
        {
            return;
        }

        let id = ctx.id();
        let server = ctx.server().clone();

        if cmd == "#endtrap" {
            crate::trap::cmd_endtrap(ctx);
            return;
        }

        if cmd == "#newtrap" || cmd == "#deltrap" {
            if get_server_state(&server)
                .read()
                .unwrap()
                .trap_edit
                .is_some()
            {
                ui::send_warn(
                    &server,
                    id,
                    "Cannot add or remove traps while editing one. Use #endtrap first.",
                );
                return;
            }
        }

        let pos = match server.players.try_get(id) {
            Some(p) => {
                if p.client_state == ClientState::Spectator {
                    ui::send_warn(&server, id, "Cannot use trap commands while spectating.");
                    return;
                }
                p.position
            }
            None => return,
        };

        let dir = match self.player_dirs.get(&id) {
            Some(d) => *d,
            None => return,
        };

        let yaw_rad = (dir.yaw as f32).to_radians();
        let pitch_rad = (dir.pitch as f32).to_radians();
        let ray = [
            -yaw_rad.sin() * pitch_rad.cos(),
            yaw_rad.cos() * pitch_rad.cos(),
            pitch_rad.sin(),
        ];
        let origin = [pos.x, pos.y, pos.z + 14.0];

        let hit_orient;
        let hit_pt;
        {
            let state = get_server_state(&server).read().unwrap();
            let map = match state.map.as_ref() {
                None => return,
                Some(m) => m,
            };

            let hit = match sauer_map::raycast::raycast_with_filter(
                &map.world,
                origin,
                ray,
                2048.0,
                |c| !c.is_empty() || c.material.volume == sauer_core::Volume::Glass,
            ) {
                None => return,
                Some(h) => h,
            };
            hit_orient = hit.orient;
            hit_pt = [
                origin[0] + ray[0] * hit.dist,
                origin[1] + ray[1] * hit.dist,
                origin[2] + ray[2] * hit.dist,
            ];
        }

        if cmd == "#deltrap" {
            cmd_deltrap(ctx, hit_pt);
        } else if cmd == "#newtrap" {
            cmd_newtrap(ctx, hit_pt, hit_orient);
        } else if cmd == "#settrap" {
            crate::trap::cmd_settrap(ctx, hit_pt);
        } else if cmd == "#edittrap" {
            crate::trap::cmd_edittrap(ctx, hit_pt);
        } else if cmd == "#trapinfo" {
            crate::trap::cmd_trapinfo(ctx, hit_pt);
        } else if cmd == "#traptime" {
            let time = args
                .get(1)
                .and_then(|s| s.as_str())
                .and_then(|s| s.parse::<u32>().ok());
            match time {
                Some(t) => crate::trap::cmd_traptime(ctx, hit_pt, t),
                None => crate::trap::cmd_traptime_print(ctx, hit_pt),
            }
        } else if cmd == "#traptimeout" {
            let timeout = args
                .get(1)
                .and_then(|s| s.as_str())
                .and_then(|s| s.parse::<u32>().ok());
            match timeout {
                Some(t) => crate::trap::cmd_traptimeout(ctx, hit_pt, t),
                None => crate::trap::cmd_traptimeout_print(ctx, hit_pt),
            }
        }
    }

    async fn shoot(&self, ctx: &mut Context, packet: &channel1::Shoot) {
        let id = ctx.id();
        let server = ctx.server().clone();

        let hit_pt = [packet.to.x, packet.to.y, packet.to.z];

        let trap_to_trigger = {
            let state = crate::map_state::get_server_state(&server).read().unwrap();

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

        if let Some(trap_idx) = trap_to_trigger {
            crate::trap::trigger_trap(&server, trap_idx, id);
        }
    }

    async fn edit_mode(&self, ctx: &mut Context, _packet: &channel1::EditMode) {
        let id = ctx.id();
        let cn = ctx.server().get_cn(id).unwrap_or(0) as i32;
        if is_waiting_for_map(ctx.server()) {
            ctx.server().send_chan1(
                id,
                &Client {
                    client_num: cn,
                    packet: EditMode { is_editing: false },
                },
            );
            ui::send_warn(
                ctx.server(),
                id,
                "Waiting for map upload. Use /sendmap to proceed.",
            );
            ctx.cancel();
        }
    }

    async fn on_spawn(&self, ctx: &mut Context, _state: &mut PlayerState) {
        if is_waiting_for_map(ctx.server()) {
            ctx.cancel();
        }
    }

    async fn map_changed(&self, ctx: &mut Context, map_name: &CubeString, _mode: GameMode) {
        let id = ctx.id();
        let server = ctx.server();
        {
            let mut state = get_server_state(server).write().unwrap();
            state.map = None;
            state.waiting_for_map = true;
        state.traps.clear();
        state.trap_edit = None;
        state.active_traps.clear();
        state.client_vslot_bloat.clear();
    }
    for client_id in server.client_ids() {
            server.set_spectator(client_id, true);
        }

        let player_name = server
            .players
            .try_get(id)
            .map(|p| p.name.strip_colors().to_utf8())
            .unwrap_or_else(|| "Someone".to_string());
        let map_str = map_name.strip_colors().to_utf8();
        ui::broadcast_map_changed(server, &player_name, &map_str);
    }

    async fn map(&self, ctx: &mut Context, data: &Bytes) {
        let id = ctx.id();

        if get_server_state(ctx.server())
            .read()
            .unwrap()
            .trap_edit
            .is_some()
        {
            ui::send_warn(ctx.server(), id, "Cannot upload map while editing a trap.");
            ctx.cancel();
            return;
        }
        ctx.cancel();
        let server = ctx.server();

        let is_spectator = server
            .players
            .try_get(id)
            .map_or(false, |p| p.client_state == ClientState::Spectator);
        let is_master = server.get_privilege(id) >= Privilege::Master;

        if is_spectator && !is_master {
            ui::send_warn(server, id, "You must be unspectated to upload a map.");
            return;
        }

        let size_kb = data.len() / 1024;
        let player_name = server
            .players
            .try_get(id)
            .map(|p| p.name.strip_colors().to_utf8())
            .unwrap_or_else(|| "Someone".to_string());

        server.broadcast_chan1(&sauer_proto::channel1::ServMsg {
            text: format!("\x0C1[MAP] \x0C7Receiving map from \x0C5{}\x0C7 ({} KB). Processing...", player_name, size_kb).lossy_cubestring(),
        });

        let data_clone = data.clone();
        let server_clone = std::sync::Arc::clone(server);
        let total_bytes = data.len();
        
        tokio::spawn(async move {
            let _start_time = std::time::Instant::now();
            let parse_result = tokio::task::spawn_blocking(move || -> Result<(MapState, Vec<crate::map_state::Trap>, Vec<u8>, i32, sauer_map::vslot_db::StoreStats), String> {
                let t_start = std::time::Instant::now();
                
                let mut state = MapState::from_bytes(&data_clone).map_err(|e| e.to_string())?;
                let t_parsed = t_start.elapsed();
                
                let world_size = state.world.size;

                for ent in &mut state.world.entities {
                    if ent.ent_type == ogz_rs::data::EntityType::Particles
                        && ent.attrs[3] == 1337
                        && ent.attrs[4] == 7331
                    {
                        ent.ent_type = ogz_rs::data::EntityType::Empty;
                        ent.attrs = [0; 5];
                    }
                }

                let mut loaded_traps: Vec<crate::map_state::Trap> = Vec::new();
                let mut store_stats = sauer_map::vslot_db::StoreStats::default();

                let t_traps_start = std::time::Instant::now();
                if let Ok((entries, stats)) =
                    sauer_map::vslot_db::read_all::<crate::trap::StoredTrap>(
                        &state.world,
                        &state.meta.vslots,
                        *b"DR",
                        crate::trap::TRAP_STORE_VERSION,
                        crate::trap::TRAP_DOMAIN,
                    )
                {
                    store_stats = stats;
                    let mut stored_traps = Vec::new();
                    for entry in entries {
                        let st = entry.data;
                        let bpos = st.button_pos;
                        let valid = entry.instances.iter().any(|inst| {
                            inst.pos[0] >= bpos[0] && inst.pos[0] <= bpos[0] + 8 &&
                            inst.pos[1] >= bpos[1] && inst.pos[1] <= bpos[1] + 8 &&
                            inst.pos[2] >= bpos[2] && inst.pos[2] <= bpos[2] + 8
                        });
                        if valid {
                            stored_traps.push(st);
                        } else {
                            tracing::warn!("Rejected copied trap payload outside expected bounds at {:?}", entry.instances.first().map(|i| i.pos));
                        }
                    }
                    for st in &stored_traps {
                        st.apply_reset_to_world(&mut state.world);
                    }
                    loaded_traps = stored_traps
                        .into_iter()
                        .filter_map(|st| st.into_trap())
                        .collect();
                }
                let t_traps = t_traps_start.elapsed();

                let t_rebuild_start = std::time::Instant::now();
                crate::trap::placement::rebuild_all_trap_buttons(&mut state, &loaded_traps);
                for trap in &loaded_traps {
                    let packets = crate::trap::stored::generate_trap_vslot_packets(
                        &crate::trap::StoredTrap::from_trap(trap),
                    );
                    for p in &packets {
                        state.handle_edit_v_slot(p);
                    }
                }
                let t_rebuild = t_rebuild_start.elapsed();

                let t_serialize_start = std::time::Instant::now();
                // Compress the fully prepared map back to Bytes here in the background
                let out_bytes = state.to_ogz_bytes().map_err(|e| e.to_string())?;
                let t_serialize = t_serialize_start.elapsed();

                tracing::info!(
                    "Map compilation metrics: [Parse: {:?}] [Read Traps: {:?}] [Rebuild Geometries: {:?}] [Serialize: {:?}] [Total: {:?}]",
                    t_parsed, t_traps, t_rebuild, t_serialize, t_start.elapsed()
                );

                Ok((state, loaded_traps, out_bytes, world_size, store_stats))
            }).await.expect("Map parsing panicked");

        match parse_result {
            Ok((state, loaded_traps, out_bytes, world_size, stats)) => {
                state.update_server_items(&server_clone);

                let comp_kb = stats.payload_compressed as f64 / 1024.0;
                let uncomp_kb = stats.payload_uncompressed as f64 / 1024.0;
                let percent = if total_bytes > 0 {
                    (stats.payload_compressed as f64 / total_bytes as f64) * 100.0
                } else {
                    0.0
                };

                let loaded_traps_len = loaded_traps.len();

                {
                    let mut s = get_server_state(&server_clone).write().unwrap();
                    s.map = Some(state);
                    s.traps = loaded_traps;
                    s.waiting_for_map = false;
                    s.trap_edit = None;
                    s.active_traps.clear();
                }
                info!("id {id} uploaded a map (world_size={world_size}); state loaded");

                server_clone.broadcast_chan1(&sauer_proto::channel1::ServMsg {
                    text: format!(
                        "\x0C1[MAP] \x0C7Loaded \x0C5{}\x0C7 traps. Size: \x0C2{:.1} KB\x0C7 comp / \x0C2{:.1} KB\x0C7 uncomp (\x0C2{:.1}%\x0C7 of OGZ).",
                        loaded_traps_len,
                        comp_kb,
                        uncomp_kb,
                        percent
                    ).lossy_cubestring()
                });

            let pkt = sauer_proto::channel2::SendMap { data: out_bytes };
            for client_id in server_clone.client_ids() {
                server_clone.send_chan2(client_id, &pkt);
            }

            server_clone.set_spectator(id, false);
            ui::send(
                &server_clone,
                id,
                ui::body("Map received. You can now spawn and edit freely."),
            );
        }
        Err(e) => warn!("id {id} sent an invalid map: {e}"),
        }
    });
    }

    async fn on_client_welcomed(&self, ctx: &mut Context) {
        let id = ctx.id();
        let server = ctx.server();
        crate::map_state::get_server_state(server)
            .read()
            .unwrap()
            .client_vslot_bloat
            .insert(id, 0);

        if server.client_count() == 1 {
            server.set_privilege(id, Privilege::Master);
            server.set_spectator(id, false);
        }

        server.send_chan1(
            id,
            &sauer_proto::channel1::InitClient {
                client_num: crate::SERVER_BOT_CN as i32,
                name: "Server".lossy_cubestring(),
                player_model: sauer_proto::data::PlayerModel::MrFixit,
                team: "".lossy_cubestring(),
            },
        );
        server.send_chan1(
            id,
            &sauer_proto::channel1::SpectatorServer {
                client_num: crate::SERVER_BOT_CN as i32,
                is_spectator: true,
            },
        );

        crate::map_state::send_map_to_async(ctx.server(), id);
        info!("sent map state to id {id} after welcome");
        ui::send_welcome_edit(server, id);
    }

    async fn get_map(&self, ctx: &mut Context, _packet: &channel1::GetMap) {
        let id = ctx.id();
        crate::map_state::send_map_to_async(ctx.server(), id);
        info!("sent map state to id {id} on N_GETMAP");
        ctx.cancel();
    }

    async fn spectator(&self, ctx: &mut Context, packet: &channel1::SpectatorClient) {
        let id = ctx.id();
        if !packet.val && is_waiting_for_map(ctx.server()) {
            ui::send_warn(
                ctx.server(),
                id,
                "Waiting for map upload. Use /sendmap to proceed.",
            );
            ctx.cancel();
        }
    }

    // ── Coop-edit map tracking + Interceptors + Trap immunities ──────────────────────

    async fn edit_tex(&self, ctx: &mut Context, packet: &channel1::EditTex) {
        intercept_packet(ctx, crate::trap::RecordedPacket::EditTex(packet.clone()));
        with_map_state_mut(ctx, |state| state.handle_edit_tex(packet));
        check_and_rebuild_traps(ctx.server(), ctx.id(), |trap| {
            intersects_sel(&packet.sel, trap)
        });
    }

    async fn edit_mat(&self, ctx: &mut Context, packet: &channel1::EditMat) {
        intercept_packet(ctx, crate::trap::RecordedPacket::EditMat(packet.clone()));
        with_map_state_mut(ctx, |state| state.handle_edit_mat(packet));
        check_and_rebuild_traps(ctx.server(), ctx.id(), |trap| {
            intersects_sel(&packet.sel, trap)
        });
    }

    async fn del_cube(&self, ctx: &mut Context, packet: &channel1::DelCube) {
        intercept_packet(ctx, crate::trap::RecordedPacket::DelCube(packet.clone()));
        with_map_state_mut(ctx, |state| state.handle_del_cube(packet));
        check_and_rebuild_traps(ctx.server(), ctx.id(), |trap| {
            intersects_sel(&packet.sel, trap)
        });
    }

    async fn replace(&self, ctx: &mut Context, packet: &channel1::Replace) {
        intercept_packet(ctx, crate::trap::RecordedPacket::Replace(packet.clone()));
        with_map_state_mut(ctx, |state| state.handle_replace(packet));
        check_and_rebuild_traps(ctx.server(), ctx.id(), |trap| {
            if packet.in_sel == 0 {
                true
            } else {
                intersects_sel(&packet.sel, trap)
            }
        });
    }

    async fn edit_face(&self, ctx: &mut Context, packet: &channel1::EditFace) {
        intercept_packet(ctx, crate::trap::RecordedPacket::EditFace(packet.clone()));
        with_map_state_mut(ctx, |state| state.handle_edit_face(packet));
        check_and_rebuild_traps(ctx.server(), ctx.id(), |trap| {
            intersects_edit_face(&packet.sel, packet.dir, trap)
        });
    }

    async fn edit_ent(&self, ctx: &mut Context, packet: &channel1::EditEnt) {
        intercept_packet(ctx, crate::trap::RecordedPacket::EditEnt(packet.clone()));
        with_map_state_mut(ctx, |state| state.handle_edit_ent(packet));
    }

    async fn edit_var(&self, ctx: &mut Context, packet: &channel1::EditVar) {
        with_map_state_mut(ctx, |state| state.handle_edit_var(packet));
    }

    async fn edit_v_slot(&self, ctx: &mut Context, packet: &channel1::EditVSlot) {
        let delta = &packet.extra_data;
        let mut size = 1;
        for p in &delta.params { size += 1 + p.name.len() + 1 + 16; }
        if delta.scale.is_some() { size += 5; }
        if delta.rotation.is_some() { size += 5; }
        if delta.offset.is_some() { size += 9; }
        if delta.scroll.is_some() { size += 9; }
        if delta.layer.is_some() { size += 5; }
        if delta.alpha.is_some() { size += 9; }
        if delta.color.is_some() { size += 13; }
        crate::map_state::track_vslot_bloat(ctx.server(), size);
        intercept_packet(ctx, crate::trap::RecordedPacket::EditVSlot(packet.clone()));
        with_map_state_mut(ctx, |state| state.handle_edit_v_slot(packet));
        check_and_rebuild_traps(ctx.server(), ctx.id(), |trap| {
            intersects_sel(&packet.sel, trap)
        });
    }

    async fn flip(&self, ctx: &mut Context, packet: &channel1::Flip) {
        intercept_packet(ctx, crate::trap::RecordedPacket::Flip(packet.clone()));
        check_and_rebuild_traps(ctx.server(), ctx.id(), |trap| {
            intersects_sel(&packet.sel, trap)
        });
    }

    async fn rotate(&self, ctx: &mut Context, packet: &channel1::Rotate) {
        intercept_packet(ctx, crate::trap::RecordedPacket::Rotate(packet.clone()));
        check_and_rebuild_traps(ctx.server(), ctx.id(), |trap| {
            intersects_sel(&packet.sel, trap)
        });
    }

    async fn copy(&self, ctx: &mut Context, packet: &channel1::CopyClient) {
        let id = ctx.id();
        let server = ctx.server().clone();

        if *server.game_mode.read().unwrap() != GameMode::CoopEdit {
            return;
        }

        if let Some(mut p) = server.players.try_get_mut(id) {
            p.clipboard = None;
        }

        if let Some(state) = crate::map_state::get_server_state(&server)
            .write()
            .unwrap()
            .map
            .as_mut()
        {
            let sel = &packet.sel;

            let d = sel.orient as usize >> 1;
            let r_dim = [1usize, 2, 0][d];
            let c_dim = [2usize, 0, 1][d];
            let d_dim = [0usize, 1, 2][d];

            let s_r = sel.s[r_dim];
            let s_c = sel.s[c_dim];
            let s_d = sel.s[d_dim];
            let dc = sel.orient as usize & 1;

            let total_nodes = (s_r * s_c * s_d) as usize;
            let mut extracted_nodes = vec![
                sauer_map::Node::Leaf(sauer_map::Cube {
                    edges: [0; 12],
                    textures: [0; 6],
                    material: Default::default()
                });
                total_nodes
            ];

            state.world.edit_sel(&sel, |node, pos| {
                let dx = (pos[r_dim] - sel.o[r_dim]) / sel.grid;
                let dy = (pos[c_dim] - sel.o[c_dim]) / sel.grid;
                let pos_d = (pos[d_dim] - sel.o[d_dim]) / sel.grid;
                let dz = if dc != 0 { s_d - 1 - pos_d } else { pos_d };

                if dx >= 0 && dx < s_r && dy >= 0 && dy < s_c && dz >= 0 && dz < s_d {
                    let idx = (dz * s_c * s_r + dy * s_r + dx) as usize;
                    if idx < extracted_nodes.len() {
                        extracted_nodes[idx] = node.clone();
                    }
                }
            });

            let vslots = sauer_map::clipboard::extract_vslots(&extracted_nodes, &state.meta.vslots);

            if let Some(mut p) = server.players.try_get_mut(id) {
                p.clipboard = Some(sauer_server::ClipboardData {
                    payload: sauer_proto::data::ClipboardPayload {
                        unpack_len: 0,
                        sel: packet.sel.clone(),
                        nodes: extracted_nodes,
                        vslots,
                    }
                });
            }
        }

        ctx.cancel();
        let cn = server.get_cn(id).unwrap_or(0) as i32;
        server.broadcast_except_chan1(
            id,
            &sauer_proto::Client {
                client_num: cn,
                packet: sauer_proto::channel1::CopyServer {
                    sel: packet.sel.clone(),
                },
            },
        );
    }

    async fn paste(&self, ctx: &mut Context, packet: &channel1::PasteClient) {
        let id = ctx.id();
        let server = ctx.server().clone();

        if let Some(p) = server.players.try_get(id) {
            if let Some(cb) = &p.clipboard {
                let payload = cb.payload.clone();
                with_map_state_mut(ctx, |state| state.handle_paste(packet, &payload));

                let cb_sel = &payload.sel;
                let min = [packet.sel.o.x, packet.sel.o.y, packet.sel.o.z];
                let max = [
                    min[0] + cb_sel.s.x * packet.sel.grid,
                    min[1] + cb_sel.s.y * packet.sel.grid,
                    min[2] + cb_sel.s.z * packet.sel.grid,
                ];
                crate::trap::check_and_rebuild_traps(ctx.server(), ctx.id(), |trap| {
                    crate::trap::intersects_bounds(min, max, trap)
                });
                return;
            }
        }
        crate::trap::check_and_rebuild_traps(ctx.server(), ctx.id(), |_| true);
    }

    async fn undo(&self, ctx: &mut Context, packet: &channel1::UndoClient) {
        with_map_state_mut(ctx, |state| state.handle_undo_redo(&packet.payload));

        let sel = &packet.payload.sel;
        let max = [
            sel.o.x + sel.s.x * sel.grid,
            sel.o.y + sel.s.y * sel.grid,
            sel.o.z + sel.s.z * sel.grid,
        ];
        crate::trap::check_and_rebuild_traps(ctx.server(), ctx.id(), |trap| {
            crate::trap::intersects_bounds([sel.o.x, sel.o.y, sel.o.z], max, trap)
        });
    }

    async fn new_map(&self, ctx: &mut Context, packet: &channel1::NewMap) {
        let id = ctx.id();
        let server = ctx.server();

        if get_server_state(server).read().unwrap().trap_edit.is_some() {
            ui::send_warn(server, id, "Cannot use newmap while editing a trap.");
            crate::map_state::send_map_to_async(ctx.server(), id);
            ctx.cancel();
            return;
        }
        let server = ctx.server();
        let mut state = get_server_state(server).write().unwrap();
        if packet.size >= 0 {
            let world_size = 1_i32 << packet.size.clamp(10, 16);
            state.map = Some(MapState::new_empty(world_size));
            state.traps.clear();
        state.active_traps.clear();
        state.trap_edit = None;
        state.trap_edit_particle_ent = None;
        state.client_vslot_bloat.clear();
    } else if let Some(map) = state.map.as_mut() {
            map.enlarge();
        }
    }

    async fn map_vote(&self, ctx: &mut Context, packet: &channel1::MapVote) {
        let id = ctx.id();
        let server = ctx.server();

        if get_server_state(server).read().unwrap().trap_edit.is_some() {
            ui::send_warn(server, id, "Cannot vote map while editing a trap.");
            ctx.cancel();
            return;
        }
        if packet.mode != GameMode::CoopEdit {
            ctx.cancel();
        }
    }

    async fn redo(&self, ctx: &mut Context, packet: &channel1::RedoClient) {
        with_map_state_mut(ctx, |state| state.handle_undo_redo(&packet.payload));

        let sel = &packet.payload.sel;
        let max = [
            sel.o.x + sel.s.x * sel.grid,
            sel.o.y + sel.s.y * sel.grid,
            sel.o.z + sel.s.z * sel.grid,
        ];
        crate::trap::check_and_rebuild_traps(ctx.server(), ctx.id(), |trap| {
            crate::trap::intersects_bounds([sel.o.x, sel.o.y, sel.o.z], max, trap)
        });
    }
}
