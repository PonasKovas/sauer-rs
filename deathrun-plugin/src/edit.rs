use crate::map_state::{
    DeathrunState, get_server_state, is_waiting_for_map, with_map_state_mut, update_server_items,
};
use sauer_map::Map;
use crate::trap::{cmd_deltrap, cmd_newtrap};
use crate::ui;
use bytes::Bytes;
use cube_string::{CubeString, IntoCubeStringLossy};
use dashmap::DashMap;
use sauer_proto::{
    Client, channel0,
    channel1::{self, EditMode},
    data::{ClientState, Direction, EntityType, GameMode, PlayerState, Privilege},
};
use sauer_server::{
    Server,
    plugin::{Context, Plugin},
};
use std::sync::RwLock;
use tracing::{info, warn};

pub const TRAP_EDIT_PARTICLE_ATTRS: [i32; 5] = [34, 100, 100, 0, 0];
pub const TRAP_EDIT_PARTICLE_YAW_OFFSET: f32 = 30.0;
pub const TRAP_EDIT_PARTICLE_PITCH_OFFSET: f32 = 0.0;
pub const TRAP_EDIT_PARTICLE_MAX_DIST: f32 = 1000.0;

// ─── Packet Memory Tracker ────────────────────────────────────────────────────

fn handle_edit_reject(ctx: &Context, min: [i32; 3], max: [i32; 3]) {
    crate::ui::send_warn(ctx.server(), ctx.id(), "That region is protected by a trap.");
    let server = ctx.server().clone();
    let cn = crate::SERVER_BOT_CN as i32;
    tokio::spawn(async move {
        let payloads = {
            let state = crate::map_state::get_server_state(&server).read().unwrap();
            if let Some(map) = state.map.as_ref() {
                map.generate_restore_packets(min, max)
            } else {
                vec![]
            }
        };
        crate::geo_sync::broadcast_payloads(&server, cn, payloads, 0).await;
    });
}



pub struct TrapHighlightState {
    pub trap_idx: Option<usize>,
    pub look_start: tokio::time::Instant,
    pub last_sent: tokio::time::Instant,
    pub active: bool,
}

pub struct DeathrunEdit {
    player_dirs: DashMap<u64, Direction>,
    last_particle_update: DashMap<u64, tokio::time::Instant>,
    trap_highlights: DashMap<u64, TrapHighlightState>,
}

impl DeathrunEdit {
    pub fn new() -> Self {
        Self {
            player_dirs: DashMap::new(),
            last_particle_update: DashMap::new(),
            trap_highlights: DashMap::new(),
        }
    }
}

impl Plugin for DeathrunEdit {
    async fn disconnected(&self, ctx: &mut Context) {
        let id = ctx.id();
        self.player_dirs.remove(&id);
        self.last_particle_update.remove(&id);
        self.trap_highlights.remove(&id);
        
        let mut state_w = get_server_state(ctx.server()).write().unwrap();
        if let Some(map) = state_w.map.as_mut() {
            let up = map.clear_all_virt_ents_for_player(id);
            let cn = crate::SERVER_BOT_CN as i32;
            if !up.global_broadcast.is_empty() {
                ctx.server().broadcast_chan1(up.global_broadcast.into_iter().map(|p| Client { client_num: cn, packet: p }).collect::<Vec<_>>());
            }
        }
    }

    async fn init(&self, server: &mut Server) {
        *server.map_name.write().unwrap() = "".lossy_cubestring();
        *server.game_mode.write().unwrap() = GameMode::CoopEdit;
        server.reserve_cn(crate::SERVER_BOT_CN);
        let mut state = DeathrunState::default();
        state.map = Some(Map::default_empty());
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
        let is_spectator = server.players.try_get(id).map_or(true, |p| p.client_state == ClientState::Spectator);

        // --- READ PHASE 1: Target Trap & AABB ---
        let target_trap;
        let mut edges_to_draw = None;
        let is_trap_edit_active;

        let origin = [packet.pos.x, packet.pos.y, packet.pos.z + 14.0];

        {
            let state = get_server_state(server).read().unwrap();
            
            let yaw_rad = (packet.dir.yaw as f32).to_radians();
            let pitch_rad = (packet.dir.pitch as f32).to_radians();
            let forward = [
                -yaw_rad.sin() * pitch_rad.cos(),
                yaw_rad.cos() * pitch_rad.cos(),
                pitch_rad.sin(),
            ];
            
            let mut dist = 2048.0;
            if let Some(map) = state.map.as_ref() {
                if let Some(hit) = sauer_map::raycast::raycast_with_filter(
                    map, origin, forward, 2048.0,
                    |c| !c.is_empty() || c.material.volume == sauer_core::Volume::Glass,
                ) {
                    dist = hit.dist;
                }
            }
            let hit_pt = [
                origin[0] + forward[0] * dist,
                origin[1] + forward[1] * dist,
                origin[2] + forward[2] * dist,
            ];

            let hit_trap = if is_spectator {
                None
            } else {
                state.traps.iter().position(|trap| {
                    let b_min = trap.button_pos;
                    let b_max = [
                        b_min[0] + trap.button_size,
                        b_min[1] + trap.button_size,
                        b_min[2] + trap.button_size,
                    ];
                    (0..3).all(|ax| hit_pt[ax] >= b_min[ax] as f32 - 0.5 && hit_pt[ax] <= b_max[ax] as f32 + 0.5)
                })
            };

            let edited_trap = state.trap_edit.as_ref().map(|e| e.trap_idx);
            target_trap = edited_trap.or(hit_trap);
            is_trap_edit_active = state.trap_edit.is_some();

            if let Some(idx) = target_trap {
                let mut show_box = false;
                let mut t_min = [0; 3];
                let mut t_max = [0; 3];

                if let Some(edit_state) = state.trap_edit.as_ref().filter(|e| e.trap_idx == idx) {
                    let current_map = state.map.as_ref().unwrap();
                    let world_size = current_map.size;
                    let mut tight_min = [i32::MAX; 3];
                    let mut tight_max = [i32::MIN; 3];
                    
                    sauer_map::octree::diff_bounds(&edit_state.original_map.root, &current_map.root, [0, 0, 0], world_size, &mut tight_min, &mut tight_max);
                    
                    let empty_ent = ogz_rs::data::Entity { position: [0.0; 3], attrs: [0; 5], ent_type: ogz_rs::data::EntityType::Empty, extra_data: vec![] };
                    for i in 0..current_map.entities.len().max(edit_state.original_map.entities.len()) {
                        if current_map.virt_state.global_pool.contains(&i) { continue; }
                        let c = current_map.entities.get(i).unwrap_or(&empty_ent);
                        let o = edit_state.original_map.entities.get(i).unwrap_or(&empty_ent);
                        if c != o {
                            if c.ent_type != ogz_rs::data::EntityType::Empty {
                                for j in 0..3 {
                                    tight_min[j] = tight_min[j].min(c.position[j].floor() as i32);
                                    tight_max[j] = tight_max[j].max(c.position[j].ceil() as i32 + 1);
                                }
                            }
                            if o.ent_type != ogz_rs::data::EntityType::Empty {
                                for j in 0..3 {
                                    tight_min[j] = tight_min[j].min(o.position[j].floor() as i32);
                                    tight_max[j] = tight_max[j].max(o.position[j].ceil() as i32 + 1);
                                }
                            }
                        }
                    }

                    if edit_state.is_edit {
                        let trap = &state.traps[idx];
                        if trap.clip_s != [0, 0, 0] {
                            let o_min = trap.clip_pos;
                            let o_max = [
                                o_min[0] + trap.clip_s[0] * trap.clip_grid,
                                o_min[1] + trap.clip_s[1] * trap.clip_grid,
                                o_min[2] + trap.clip_s[2] * trap.clip_grid,
                            ];
                            for i in 0..3 {
                                tight_min[i] = tight_min[i].min(o_min[i]);
                                tight_max[i] = tight_max[i].max(o_max[i]);
                            }
                        }
                    }

                    if tight_min[0] != i32::MAX {
                        t_min = std::array::from_fn(|i| tight_min[i].max(0));
                        t_max = std::array::from_fn(|i| tight_max[i].min(world_size));
                        show_box = true;
                    }
                }
                
                if !show_box {
                    let trap = &state.traps[idx];
                    if trap.clip_s != [0, 0, 0] {
                        t_min = trap.clip_pos;
                        t_max = [
                            t_min[0] + trap.clip_s[0] * trap.clip_grid,
                            t_min[1] + trap.clip_s[1] * trap.clip_grid,
                            t_min[2] + trap.clip_s[2] * trap.clip_grid,
                        ];
                        show_box = true;
                    }
                }

                if show_box {
                    let len_x = t_max[0] - t_min[0];
                    let len_y = t_max[1] - t_min[1];
                    let len_z = t_max[2] - t_min[2];

                    edges_to_draw = Some([
                        (t_min[0], t_min[1], t_min[2], 1, len_x),
                        (t_min[0], t_max[1], t_min[2], 1, len_x),
                        (t_min[0], t_min[1], t_max[2], 1, len_x),
                        (t_min[0], t_max[1], t_max[2], 1, len_x),
                        (t_min[0], t_min[1], t_min[2], 2, len_y),
                        (t_max[0], t_min[1], t_min[2], 2, len_y),
                        (t_min[0], t_min[1], t_max[2], 2, len_y),
                        (t_max[0], t_min[1], t_max[2], 2, len_y),
                        (t_min[0], t_min[1], t_min[2], 0, len_z),
                        (t_max[0], t_min[1], t_min[2], 0, len_z),
                        (t_min[0], t_max[1], t_min[2], 0, len_z),
                        (t_max[0], t_max[1], t_min[2], 0, len_z),
                    ]);
                }
            }
        } // Read Lock 1 Dropped

        // --- WRITE PHASE 1: Update DashMap and Highlight Virt Ents ---
        let mut clear_highlight = false;
        let mut update_highlight = false;

        let mut hl_state_clone = {
            let mut hl_state = self.trap_highlights.entry(id).or_insert_with(|| TrapHighlightState {
                trap_idx: None,
                look_start: tokio::time::Instant::now(),
                last_sent: tokio::time::Instant::now(),
                active: false,
            });

            if hl_state.trap_idx != target_trap {
                if hl_state.active {
                    clear_highlight = true;
                }
                hl_state.trap_idx = target_trap;
                hl_state.look_start = tokio::time::Instant::now();
                hl_state.active = false;
            }

            if target_trap.is_some() {
                if hl_state.last_sent.elapsed().as_millis() > 100 {
                    update_highlight = true;
                    hl_state.last_sent = tokio::time::Instant::now();
                }
            }
            
            TrapHighlightState {
                trap_idx: hl_state.trap_idx,
                look_start: hl_state.look_start,
                last_sent: hl_state.last_sent,
                active: hl_state.active,
            }
        };

        if clear_highlight {
            let mut state_w = get_server_state(server).write().unwrap();
            if let Some(map) = state_w.map.as_mut() {
                let up = map.clear_virt_ents(id, "highlight");
                let cn = crate::SERVER_BOT_CN as i32;
                if !up.global_broadcast.is_empty() {
                    server.broadcast_chan1(up.global_broadcast.into_iter().map(|p| Client { client_num: cn, packet: p }).collect::<Vec<_>>());
                }
                if !up.personal.is_empty() {
                    server.send_chan1(id, up.personal.into_iter().map(|p| Client { client_num: cn, packet: p }).collect::<Vec<_>>());
                }
            }
            hl_state_clone.active = false;
        }

        if update_highlight {
            if let Some(edges) = edges_to_draw {
                let time_ms = std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).unwrap().as_millis();
                let hue = (time_ms % 2000) as f32 / 2000.0;
                let h = hue * 6.0;
                let x = 1.0 - ((h % 2.0) - 1.0).abs();
                let (r, g, b) = match h as i32 {
                    0 => (1.0, x, 0.0),
                    1 => (x, 1.0, 0.0),
                    2 => (0.0, 1.0, x),
                    3 => (0.0, x, 1.0),
                    4 => (x, 0.0, 1.0),
                    _ => (1.0, 0.0, x),
                };
                let color_int = ((r * 255.0) as i32) << 16 | ((g * 255.0) as i32) << 8 | ((b * 255.0) as i32);

                let mut fake_ents = Vec::new();
                for edge in &edges {
                    fake_ents.push(sauer_map::virt_ents::FakeEnt {
                        ent_type: EntityType::Particles,
                        position: [edge.0 as f32, edge.1 as f32, edge.2 as f32],
                        attrs: [7, edge.3, edge.4, color_int, 0],
                    });
                }
                let mut state_w = get_server_state(server).write().unwrap();
                if let Some(map) = state_w.map.as_mut() {
                    let up = map.update_virt_ents(id, "highlight", fake_ents);
                    let cn = crate::SERVER_BOT_CN as i32;
                    if !up.global_broadcast.is_empty() {
                        server.broadcast_chan1(up.global_broadcast.into_iter().map(|p| Client { client_num: cn, packet: p }).collect::<Vec<_>>());
                    }
                    if !up.personal.is_empty() {
                        server.send_chan1(id, up.personal.into_iter().map(|p| Client { client_num: cn, packet: p }).collect::<Vec<_>>());
                    }
                }
                hl_state_clone.active = true;
            } else if hl_state_clone.active {
                let mut state_w = get_server_state(server).write().unwrap();
                if let Some(map) = state_w.map.as_mut() {
                    let up = map.clear_virt_ents(id, "highlight");
                    let cn = crate::SERVER_BOT_CN as i32;
                    if !up.global_broadcast.is_empty() {
                        server.broadcast_chan1(up.global_broadcast.into_iter().map(|p| Client { client_num: cn, packet: p }).collect::<Vec<_>>());
                    }
                    if !up.personal.is_empty() {
                        server.send_chan1(id, up.personal.into_iter().map(|p| Client { client_num: cn, packet: p }).collect::<Vec<_>>());
                    }
                }
                hl_state_clone.active = false;
            }
        }

        if let Some(mut entry) = self.trap_highlights.get_mut(&id) {
            entry.active = hl_state_clone.active;
        }

        // --- READ PHASE 2: Check particle timer ---
        let mut should_update_pos = false;
        let mut clear_particle = false;

        if is_trap_edit_active {
            let now = tokio::time::Instant::now();
            if let Some(mut last) = self.last_particle_update.get_mut(&id) {
                if now.duration_since(*last).as_millis() >= 100 {
                    *last = now;
                    should_update_pos = true;
                }
            } else {
                self.last_particle_update.insert(id, now);
                should_update_pos = true;
            }
        } else if self.last_particle_update.remove(&id).is_some() {
            clear_particle = true;
        }

        // --- WRITE PHASE 2: Update Particle Virt Ents ---
        if clear_particle {
            let mut state_w = get_server_state(server).write().unwrap();
            if let Some(map) = state_w.map.as_mut() {
                let up = map.clear_virt_ents(id, "edit_particle");
                let cn = crate::SERVER_BOT_CN as i32;
                if !up.global_broadcast.is_empty() {
                    server.broadcast_chan1(up.global_broadcast.into_iter().map(|p| Client { client_num: cn, packet: p }).collect::<Vec<_>>());
                }
                if !up.personal.is_empty() {
                    server.send_chan1(id, up.personal.into_iter().map(|p| Client { client_num: cn, packet: p }).collect::<Vec<_>>());
                }
            }
        }

        if should_update_pos {
            let yaw_rad = (packet.dir.yaw as f32).to_radians();
            let pitch_rad = (packet.dir.pitch as f32).to_radians();

            let yaw_off_rad = TRAP_EDIT_PARTICLE_YAW_OFFSET.to_radians();
            let pitch_off_rad = TRAP_EDIT_PARTICLE_PITCH_OFFSET.to_radians();

            // Calculate the orthonormal basis vectors for the camera
            let f = [
                -yaw_rad.sin() * pitch_rad.cos(),
                yaw_rad.cos() * pitch_rad.cos(),
                pitch_rad.sin(),
            ];

            let r = [
                yaw_rad.cos(),
                yaw_rad.sin(),
                0.0,
            ];

            let u = [
                yaw_rad.sin() * pitch_rad.sin(),
                -yaw_rad.cos() * pitch_rad.sin(),
                pitch_rad.cos(),
            ];

            // Calculate the offset vector in local camera space
            let local_x = -yaw_off_rad.sin() * pitch_off_rad.cos();
            let local_y = yaw_off_rad.cos() * pitch_off_rad.cos();
            let local_z = pitch_off_rad.sin();

            // Transform back to world space
            let forward_edit = [
                r[0] * local_x + f[0] * local_y + u[0] * local_z,
                r[1] * local_x + f[1] * local_y + u[1] * local_z,
                r[2] * local_x + f[2] * local_y + u[2] * local_z,
            ];

            let mut dist_edit = TRAP_EDIT_PARTICLE_MAX_DIST;
            
            {
                let state_r = get_server_state(server).read().unwrap();
                if let Some(map) = state_r.map.as_ref() {
                    if let Some(hit) = sauer_map::raycast::raycast_with_filter(
                        map, origin, forward_edit, TRAP_EDIT_PARTICLE_MAX_DIST,
                        |c| !c.is_empty() || c.material != sauer_core::Material::default(),
                    ) {
                        dist_edit = (hit.dist - 1.0).max(1.0);
                    }
                }
            } // Read Lock 2 Dropped

            let fake_ent = sauer_map::virt_ents::FakeEnt {
                ent_type: EntityType::Particles,
                position: [
                    origin[0] + forward_edit[0] * dist_edit,
                    origin[1] + forward_edit[1] * dist_edit,
                    origin[2] + forward_edit[2] * dist_edit,
                ],
                attrs: TRAP_EDIT_PARTICLE_ATTRS,
            };

            let mut state_w = get_server_state(server).write().unwrap();
            if let Some(map) = state_w.map.as_mut() {
                let up = map.update_virt_ents(id, "edit_particle", vec![fake_ent]);
                let cn = crate::SERVER_BOT_CN as i32;
                if !up.global_broadcast.is_empty() {
                    server.broadcast_chan1(up.global_broadcast.into_iter().map(|p| Client { client_num: cn, packet: p }).collect::<Vec<_>>());
                }
                if !up.personal.is_empty() {
                    server.send_chan1(id, up.personal.into_iter().map(|p| Client { client_num: cn, packet: p }).collect::<Vec<_>>());
                }
            }
        }
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

        if cmd == "#newtrap" || cmd == "#deltrap" || cmd == "#settrap" || cmd == "#edittrap" {
            let state = get_server_state(&server).read().unwrap();
            if state.trap_edit.is_some() {
                ui::send_warn(&server, id, "Cannot edit traps while #edittrap is active. Use #endtrap first.");
                return;
            }
            if !state.active_trap_stack.is_empty() {
                ui::send_warn(&server, id, "Cannot edit traps while a trap is currently activated. Wait for it to reset.");
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
                map,
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
            if state.trap_edit.is_some() {
                return;
            }

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
        state.base_map = None;
        state.active_trap_stack.clear();
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
            let parse_result = tokio::task::spawn_blocking(move || -> Result<(Map, Vec<crate::map_state::Trap>, Vec<u8>, i32, sauer_map::db::StoreStats), String> {
                let t_start = std::time::Instant::now();
                
                let mut state = Map::from_bytes(&data_clone).map_err(|e: ogz_rs::OgzError| e.to_string())?;
                let t_parsed = t_start.elapsed();
                
                let world_size = state.size;

                for ent in &mut state.entities {
                    if ent.ent_type == ogz_rs::data::EntityType::Particles
                        && ent.attrs[3] == 1337
                        && ent.attrs[4] == 7331
                    {
                        ent.ent_type = ogz_rs::data::EntityType::Empty;
                        ent.attrs = [0; 5];
                    }
                }

                let mut loaded_traps: Vec<crate::map_state::Trap> = Vec::new();
                let mut store_stats = sauer_map::db::StoreStats::default();

                let t_traps_start = std::time::Instant::now();
                if let Ok((entries, stats)) = state.db_scan::<crate::trap::StoredTrap>() {
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
                    for mut st in stored_traps {
                        st.remap_vslots(&mut state);
                        if let Some(trap) = st.into_trap() {
                            loaded_traps.push(trap);
                        }
                    }
                }
                let t_traps = t_traps_start.elapsed();

                let t_rebuild_start = std::time::Instant::now();
                crate::trap::placement::rebuild_all_trap_buttons(&mut state, &loaded_traps);
                for trap in &loaded_traps {
                    let packets = crate::trap::stored::generate_trap_vslot_packets(
                        &crate::trap::StoredTrap::from_trap(trap),
                    );
                    for p in &packets {
                        state.apply_vslots(&p.sel, p.all_faces, p.delta != 0, &p.extra_data);
                    }
                }
                let t_rebuild = t_rebuild_start.elapsed();

                let t_serialize_start = std::time::Instant::now();
                // Compress the fully prepared map back to Bytes here in the background
                let out_bytes = state.to_ogz_bytes().map_err(|e: ogz_rs::OgzError| e.to_string())?;
                let t_serialize = t_serialize_start.elapsed();

                tracing::info!(
                    "Map compilation metrics: [Parse: {:?}] [Read Traps: {:?}] [Rebuild Geometries: {:?}] [Serialize: {:?}] [Total: {:?}]",
                    t_parsed, t_traps, t_rebuild, t_serialize, t_start.elapsed()
                );

                Ok((state, loaded_traps, out_bytes, world_size, store_stats))
            }).await.expect("Map parsing panicked");

            match parse_result {
            Ok((mut state, loaded_traps, out_bytes, world_size, stats)) => {
                for trap in &loaded_traps {
                    let b_max = [trap.button_pos[0] + trap.button_size, trap.button_pos[1] + trap.button_size, trap.button_pos[2] + trap.button_size];
                    state.protect_region(trap.button_pos, b_max);
                }

                update_server_items(&state, &server_clone);

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
                    s.base_map = None;
                    s.active_trap_stack.clear();
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
        let mut ok = true;
        with_map_state_mut(ctx, |state| ok = state.apply_edit_tex(packet));
        if !ok {
            let (min, max) = packet.sel.bounds();
            handle_edit_reject(ctx, min, max);
        }
    }

    async fn edit_mat(&self, ctx: &mut Context, packet: &channel1::EditMat) {
        let mut ok = true;
        with_map_state_mut(ctx, |state| ok = state.apply_edit_mat(packet));
        if !ok {
            let (min, max) = packet.sel.bounds();
            handle_edit_reject(ctx, min, max);
        }
    }

    async fn del_cube(&self, ctx: &mut Context, packet: &channel1::DelCube) {
        let mut ok = true;
        with_map_state_mut(ctx, |state| ok = state.apply_del_cube(packet));
        if !ok {
            let (min, max) = packet.sel.bounds();
            handle_edit_reject(ctx, min, max);
        }
    }

    async fn replace(&self, ctx: &mut Context, packet: &channel1::Replace) {
        let mut ok = true;
        with_map_state_mut(ctx, |state| ok = state.apply_replace(packet));
        if !ok {
            let (min, max) = if packet.in_sel != 0 {
                packet.sel.bounds()
            } else {
                let s = crate::map_state::get_server_state(ctx.server()).read().unwrap().map.as_ref().map(|m| m.size).unwrap_or(0);
                ([0, 0, 0], [s, s, s])
            };
            handle_edit_reject(ctx, min, max);
        }
    }

    async fn edit_face(&self, ctx: &mut Context, packet: &channel1::EditFace) {
        let mut ok = true;
        with_map_state_mut(ctx, |state| ok = state.apply_edit_face(packet));
        if !ok {
            let mut min = [packet.sel.o.x, packet.sel.o.y, packet.sel.o.z];
            let mut max = [
                min[0] + packet.sel.s.x * packet.sel.grid,
                min[1] + packet.sel.s.y * packet.sel.grid,
                min[2] + packet.sel.s.z * packet.sel.grid,
            ];
            
            let d = (packet.sel.orient as usize) >> 1;
            let dc = (packet.sel.orient as usize) & 1;
            let dir = packet.dir;
            let grid = packet.sel.grid;
            
            let mut actual_mode = packet.mode as i32;
            if actual_mode == 1 && (packet.sel.cx > 0 || packet.sel.cy > 0 || (packet.sel.cxs & 1) != 0 || (packet.sel.cys & 1) != 0) {
                actual_mode = 0;
            }

            if actual_mode == 1 {
                if dc == 1 {
                    let old_face = max[d];
                    let new_face = max[d] - dir * grid;
                    min[d] = old_face.min(new_face);
                    max[d] = old_face.max(new_face);
                } else {
                    let old_face = min[d];
                    let new_face = min[d] + dir * grid;
                    min[d] = old_face.min(new_face);
                    max[d] = old_face.max(new_face);
                }
            } else {
                if dc == 1 {
                    min[d] = max[d] - grid;
                } else {
                    max[d] = min[d] + grid;
                }
            }
            handle_edit_reject(ctx, min, max);
        }
    }

    async fn edit_ent(&self, ctx: &mut Context, packet: &channel1::EditEnt) {
        let mut ok = true;
        with_map_state_mut(ctx, |state| ok = state.apply_edit_ent(packet));
        if !ok {
            let min = [packet.pos.x as i32, packet.pos.y as i32, packet.pos.z as i32];
            let max = [min[0] + 1, min[1] + 1, min[2] + 1];
            handle_edit_reject(ctx, min, max);
        }
    }

    async fn edit_var(&self, ctx: &mut Context, packet: &channel1::EditVar) {
        with_map_state_mut(ctx, |state| state.apply_edit_var(packet));
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
        let mut ok = true;
        with_map_state_mut(ctx, |state| ok = state.apply_edit_v_slot(packet));
        if !ok {
            let (min, max) = packet.sel.bounds();
            handle_edit_reject(ctx, min, max);
        }
    }

    async fn flip(&self, ctx: &mut Context, packet: &channel1::Flip) {
        let mut ok = true;
        let (min, max) = packet.sel.bounds();
        with_map_state_mut(ctx, |state| if state.is_protected(min, max) { ok = false; });
        if !ok {
            handle_edit_reject(ctx, min, max);
        }
    }

    async fn rotate(&self, ctx: &mut Context, packet: &channel1::Rotate) {
        let mut ok = true;
        let (min, max) = packet.sel.bounds();
        with_map_state_mut(ctx, |state| if state.is_protected(min, max) { ok = false; });
        if !ok {
            handle_edit_reject(ctx, min, max);
        }
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
            let payload = state.copy_selection(&packet.sel);

            if let Some(mut p) = server.players.try_get_mut(id) {
                p.clipboard = Some(sauer_server::ClipboardData {
                    payload
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
                let mut ok = true;
                with_map_state_mut(ctx, |state| ok = state.apply_paste(packet, &payload));
                
                if !ok {
                    let cb_sel = &payload.sel;
                    let min = [packet.sel.o.x, packet.sel.o.y, packet.sel.o.z];
                    let max = [
                        min[0] + cb_sel.s.x * packet.sel.grid,
                        min[1] + cb_sel.s.y * packet.sel.grid,
                        min[2] + cb_sel.s.z * packet.sel.grid,
                    ];
                    handle_edit_reject(ctx, min, max);
                }
            }
        }
    }

    async fn undo(&self, ctx: &mut Context, packet: &channel1::UndoClient) {
        let mut ok = true;
        with_map_state_mut(ctx, |state| ok = state.apply_undo_redo(&packet.payload));
        if !ok {
            match &packet.payload.data {
                sauer_proto::data::UndoData::Cubes { sel, .. } => {
                    let max = [
                        sel.o.x + sel.s.x * sel.grid,
                        sel.o.y + sel.s.y * sel.grid,
                        sel.o.z + sel.s.z * sel.grid,
                    ];
                    handle_edit_reject(ctx, [sel.o.x, sel.o.y, sel.o.z], max);
                }
                sauer_proto::data::UndoData::Entities(ents) => {
                    if let Some(ue) = ents.first() {
                        let pos = ue.ent.position;
                        let min = [pos[0] as i32, pos[1] as i32, pos[2] as i32];
                        let max = [min[0] + 1, min[1] + 1, min[2] + 1];
                        handle_edit_reject(ctx, min, max);
                    }
                }
            }
        }
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
            state.map = Some(Map::new_empty(world_size));
            state.traps.clear();
        state.active_trap_stack.clear();
        state.trap_edit = None;
        state.base_map = None;
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
        let mut ok = true;
        with_map_state_mut(ctx, |state| ok = state.apply_undo_redo(&packet.payload));
        if !ok {
            match &packet.payload.data {
                sauer_proto::data::UndoData::Cubes { sel, .. } => {
                    let max = [
                        sel.o.x + sel.s.x * sel.grid,
                        sel.o.y + sel.s.y * sel.grid,
                        sel.o.z + sel.s.z * sel.grid,
                    ];
                    handle_edit_reject(ctx, [sel.o.x, sel.o.y, sel.o.z], max);
                }
                sauer_proto::data::UndoData::Entities(ents) => {
                    if let Some(ue) = ents.first() {
                        let pos = ue.ent.position;
                        let min = [pos[0] as i32, pos[1] as i32, pos[2] as i32];
                        let max = [min[0] + 1, min[1] + 1, min[2] + 1];
                        handle_edit_reject(ctx, min, max);
                    }
                }
            }
        }
    }
}
