use crate::map_state::{
    DeathrunState, get_server_state, is_waiting_for_map, update_server_items, with_map_state_mut,
};
use crate::trap::{cmd_deltrap, cmd_newtrap, guard};
use crate::ui;
use bytes::Bytes;
use cube_string::{CubeString, IntoCubeStringLossy};
use dashmap::DashMap;
use sauer_map::Map;
use sauer_proto::{
    Client, channel0,
    channel1::{self, EditMode},
    data::{ClientState, Direction, EntityType, GameMode, MasterMode, PlayerState, Privilege},
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

fn handle_edit_reject(ctx: &Context, min: [i32; 3], max: [i32; 3]) {
    guard::reject(
        ctx,
        (min, max),
        "That area is protected (a trap button or the trap vault).",
    );
}

pub struct TrapHighlightState {
    pub trap_idx: Option<usize>,
    pub look_start: tokio::time::Instant,
    pub last_sent: tokio::time::Instant,
    pub active: bool,
}

/// Where a joining player is in the map load sequence. The info wall is held
/// back until the map is loaded so the client's own map output doesn't bury it.
enum WelcomeStep {
    /// Waiting for the client to load the map from N_MAPCHANGE.
    InitialLoad,
    /// The map was sent with N_SENDMAP, waiting for the client to load it.
    SentMap,
}

pub struct DeathrunEdit {
    player_dirs: DashMap<u64, Direction>,
    last_particle_update: DashMap<u64, tokio::time::Instant>,
    trap_highlights: DashMap<u64, TrapHighlightState>,
    pending_welcome: DashMap<u64, WelcomeStep>,
    /// The trap button each player currently sees through.
    seethrough: DashMap<u64, crate::trap::placement::ButtonKey>,
}

impl DeathrunEdit {
    pub fn new() -> Self {
        Self {
            player_dirs: DashMap::new(),
            last_particle_update: DashMap::new(),
            trap_highlights: DashMap::new(),
            pending_welcome: DashMap::new(),
            seethrough: DashMap::new(),
        }
    }
}

impl Plugin for DeathrunEdit {
    async fn disconnected(&self, ctx: &mut Context) {
        let id = ctx.id();
        self.player_dirs.remove(&id);
        self.last_particle_update.remove(&id);
        self.trap_highlights.remove(&id);
        self.pending_welcome.remove(&id);
        self.seethrough.remove(&id);

        let mut state_w = get_server_state(ctx.server()).write().unwrap();
        if let Some(map) = state_w.map.as_mut() {
            let up = map.clear_all_virt_ents_for_player(id);
            let cn = crate::SERVER_BOT_CN as i32;
            if !up.global_broadcast.is_empty() {
                ctx.server().broadcast_chan1(
                    up.global_broadcast
                        .into_iter()
                        .map(|p| Client {
                            client_num: cn,
                            packet: p,
                        })
                        .collect::<Vec<_>>(),
                );
            }
        }
    }

    async fn init(&self, server: &mut Server) {
        *server.map_name.write().unwrap() = "".lossy_cubestring();
        *server.game_mode.write().unwrap() = GameMode::CoopEdit;
        server.reserve_cn(crate::SERVER_BOT_CN);
        let state = DeathrunState {
            map: Some(Map::default_empty()),
            ..Default::default()
        };
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
        server.register_command(
            "#trapinfo",
            "print info about the trap (size, overhead)",
            "",
        );
        server.register_command("#traptime", "extend duration (default 2 000ms)", "<ms>");
        server.register_command(
            "#traptimeout",
            "set trap timeout (default 20 000ms)",
            "<ms>",
        );

        // Matches how play.rs reads the map: playerstart attr 2 is the team,
        // and good players finish by touching any flag.
        server.add_help_lines([
            ui::section("Map setup"),
            ui::tip("/newent playerstart 0 1", "good team spawn"),
            ui::tip("/newent playerstart 0 2", "terrorist spawn"),
            ui::tip("/newent flag", "finish: good players who touch it complete the map"),
        ]);
    }

    async fn pos(&self, ctx: &mut Context, packet: &channel0::Position) {
        let id = ctx.id();
        self.player_dirs.insert(id, packet.dir);

        let server = ctx.server();
        let is_spectator = server
            .players
            .try_get(id)
            .map_or(true, |p| p.client_state == ClientState::Spectator);

        // --- READ PHASE 1: Target Trap & AABB ---
        let target_trap;
        let mut edges_to_draw = None;
        let mut entity_frames = Vec::new();
        let is_trap_edit_active;
        let seethrough_target;

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
                if let Some(hit) =
                    sauer_map::raycast::raycast_with_filter(map, origin, forward, 2048.0, |c| {
                        !c.is_empty() || c.material.volume == sauer_core::Volume::Glass
                    })
                {
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
                state
                    .traps
                    .iter()
                    .position(|trap| trap.contains_button_point(hit_pt, 0.5))
            };

            let edited_trap = state.trap_edit.as_ref().map(|e| e.trap_idx);
            target_trap = edited_trap.or(hit_trap);
            is_trap_edit_active = state.trap_edit.is_some();
            seethrough_target =
                hit_trap.map(|idx| crate::trap::placement::button_key(&state.traps[idx]));

            if let Some(idx) = target_trap {
                let eye = Some((origin, forward));
                entity_frames = guard::entity_frames(&guard::trap_ent_changes(&state, idx), eye);

                let trap = &state.traps[idx];
                let live = state
                    .trap_edit
                    .as_ref()
                    .filter(|e| e.trap_idx == idx)
                    .and_then(|edit| {
                        let old = trap.geom.as_ref().filter(|_| edit.is_edit);
                        crate::trap::commands::geometry_bounds(
                            &edit.original_map,
                            state.map.as_ref()?,
                            old,
                        )
                    });
                let shown = live.or_else(|| trap.geom.as_ref().map(|g| g.clip_box()));

                if let Some((t_min, t_max)) = shown {
                    edges_to_draw = Some(guard::box_edges(t_min, t_max, eye));
                }
            }
        } // Read Lock 1 Dropped

        crate::trap::placement::update_seethrough_button(
            server,
            id,
            &self.seethrough,
            seethrough_target,
        );

        // --- WRITE PHASE 1: Update DashMap and Highlight Virt Ents ---
        let mut clear_highlight = false;
        let mut update_highlight = false;
        let mut target_changed = false;

        let mut hl_state_clone = {
            let mut hl_state =
                self.trap_highlights
                    .entry(id)
                    .or_insert_with(|| TrapHighlightState {
                        trap_idx: None,
                        look_start: tokio::time::Instant::now(),
                        last_sent: tokio::time::Instant::now(),
                        active: false,
                    });

            if hl_state.trap_idx != target_trap {
                target_changed = true;
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
                    server.broadcast_chan1(
                        up.global_broadcast
                            .into_iter()
                            .map(|p| Client {
                                client_num: cn,
                                packet: p,
                            })
                            .collect::<Vec<_>>(),
                    );
                }
                if !up.personal.is_empty() {
                    server.send_chan1(
                        id,
                        up.personal
                            .into_iter()
                            .map(|p| Client {
                                client_num: cn,
                                packet: p,
                            })
                            .collect::<Vec<_>>(),
                    );
                }
            }
            hl_state_clone.active = false;
        }

        if update_highlight {
            if let Some(edges) = edges_to_draw {
                let mut fake_ents = Vec::new();
                for edge in &edges {
                    fake_ents.push(sauer_map::virt_ents::FakeEnt {
                        ent_type: EntityType::Particles,
                        position: [edge.0 as f32, edge.1 as f32, edge.2 as f32],
                        attrs: [7, edge.3, edge.4, guard::GEOMETRY_COLOR, 0],
                    });
                }
                let mut state_w = get_server_state(server).write().unwrap();
                if let Some(map) = state_w.map.as_mut() {
                    let up = map.update_virt_ents(id, "highlight", fake_ents);
                    let cn = crate::SERVER_BOT_CN as i32;
                    if !up.global_broadcast.is_empty() {
                        server.broadcast_chan1(
                            up.global_broadcast
                                .into_iter()
                                .map(|p| Client {
                                    client_num: cn,
                                    packet: p,
                                })
                                .collect::<Vec<_>>(),
                        );
                    }
                    if !up.personal.is_empty() {
                        server.send_chan1(
                            id,
                            up.personal
                                .into_iter()
                                .map(|p| Client {
                                    client_num: cn,
                                    packet: p,
                                })
                                .collect::<Vec<_>>(),
                        );
                    }
                }
                hl_state_clone.active = true;
            } else if hl_state_clone.active {
                let mut state_w = get_server_state(server).write().unwrap();
                if let Some(map) = state_w.map.as_mut() {
                    let up = map.clear_virt_ents(id, "highlight");
                    let cn = crate::SERVER_BOT_CN as i32;
                    if !up.global_broadcast.is_empty() {
                        server.broadcast_chan1(
                            up.global_broadcast
                                .into_iter()
                                .map(|p| Client {
                                    client_num: cn,
                                    packet: p,
                                })
                                .collect::<Vec<_>>(),
                        );
                    }
                    if !up.personal.is_empty() {
                        server.send_chan1(
                            id,
                            up.personal
                                .into_iter()
                                .map(|p| Client {
                                    client_num: cn,
                                    packet: p,
                                })
                                .collect::<Vec<_>>(),
                        );
                    }
                }
                hl_state_clone.active = false;
            }
        }

        if let Some(mut entry) = self.trap_highlights.get_mut(&id) {
            entry.active = hl_state_clone.active;
        }

        // Frames around entities the trap deletes (red) or creates (green).
        if update_highlight || target_changed {
            let mut state_w = get_server_state(server).write().unwrap();
            if let Some(map) = state_w.map.as_mut() {
                let up = map.update_virt_ents(id, "trap_ents", entity_frames);
                let cn = crate::SERVER_BOT_CN as i32;
                let wrap = |pkts: Vec<channel1::EditEnt>| {
                    pkts.into_iter()
                        .map(|p| Client {
                            client_num: cn,
                            packet: p,
                        })
                        .collect::<Vec<_>>()
                };
                if !up.global_broadcast.is_empty() {
                    server.broadcast_chan1(wrap(up.global_broadcast));
                }
                if !up.personal.is_empty() {
                    server.send_chan1(id, wrap(up.personal));
                }
            }
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
                    server.broadcast_chan1(
                        up.global_broadcast
                            .into_iter()
                            .map(|p| Client {
                                client_num: cn,
                                packet: p,
                            })
                            .collect::<Vec<_>>(),
                    );
                }
                if !up.personal.is_empty() {
                    server.send_chan1(
                        id,
                        up.personal
                            .into_iter()
                            .map(|p| Client {
                                client_num: cn,
                                packet: p,
                            })
                            .collect::<Vec<_>>(),
                    );
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

            let r = [yaw_rad.cos(), yaw_rad.sin(), 0.0];

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
                        map,
                        origin,
                        forward_edit,
                        TRAP_EDIT_PARTICLE_MAX_DIST,
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
                    server.broadcast_chan1(
                        up.global_broadcast
                            .into_iter()
                            .map(|p| Client {
                                client_num: cn,
                                packet: p,
                            })
                            .collect::<Vec<_>>(),
                    );
                }
                if !up.personal.is_empty() {
                    server.send_chan1(
                        id,
                        up.personal
                            .into_iter()
                            .map(|p| Client {
                                client_num: cn,
                                packet: p,
                            })
                            .collect::<Vec<_>>(),
                    );
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
                ui::send_warn(
                    &server,
                    id,
                    "Cannot edit traps while #edittrap is active. Use #endtrap first.",
                );
                return;
            }
            if !state.active_trap_stack.is_empty() {
                ui::send_warn(
                    &server,
                    id,
                    "Cannot edit traps while a trap is currently activated. Wait for it to reset.",
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

            let hit = match sauer_map::raycast::raycast_with_filter(map, origin, ray, 2048.0, |c| {
                !c.is_empty() || c.material.volume == sauer_core::Volume::Glass
            }) {
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

    async fn map_crc(&self, ctx: &mut Context, _packet: &channel1::MapCrc) {
        let id = ctx.id();
        let server = ctx.server();
        // A locked server keeps non-masters spectating, like vanilla.
        let locked = matches!(server.master_mode(), MasterMode::Locked | MasterMode::Private)
            && server.get_privilege(id) < Privilege::Master;
        if !is_waiting_for_map(server) && !locked {
            let is_spectator = server
                .players
                .try_get(id)
                .map_or(false, |p| p.client_state == ClientState::Spectator);
            if is_spectator {
                server.set_spectator(id, false);
                ui::send(
                    server,
                    id,
                    ui::body("Map loaded. You can now spawn and edit freely."),
                );
            }
        }

        if let Some((_, step)) = self.pending_welcome.remove(&id) {
            let has_map = get_server_state(server).read().unwrap().map.is_some();
            if matches!(step, WelcomeStep::InitialLoad) && has_map {
                crate::map_state::send_map_to_async(server, id);
                info!("sent map state to id {id} after welcome");
                self.pending_welcome.insert(id, WelcomeStep::SentMap);
            } else {
                ui::send_welcome_edit(server, id);
            }
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
            text: format!(
                "\x0C1[MAP] \x0C7Receiving map from \x0C5{}\x0C7 ({} KB). Processing...",
                player_name, size_kb
            )
            .lossy_cubestring(),
        });

        let data_clone = data.clone();
        let server_clone = std::sync::Arc::clone(server);
        let total_bytes = data.len();

        tokio::spawn(async move {
            let _start_time = std::time::Instant::now();
            let parse_result = tokio::task::spawn_blocking(
                move || -> Result<(DeathrunState, Vec<String>, Vec<u8>, i32), String> {
                    let t_start = std::time::Instant::now();

                    let mut map = Map::from_bytes(&data_clone)
                        .map_err(|e: ogz_rs::OgzError| e.to_string())?;
                    let t_parsed = t_start.elapsed();

                    map.strip_virtual_entities();

                    let loaded = crate::trap::persist::load(&map);
                    crate::trap::placement::rebuild_all_trap_buttons(&mut map, &loaded.traps);
                    for trap in &loaded.traps {
                        let (min, max) = trap.button_box();
                        map.protect_region(min, max);
                    }
                    if let Some(v) = loaded.vault {
                        map.protect_region(v.bounds().0, v.bounds().1);
                    }

                    let mut state = DeathrunState {
                        map: Some(map),
                        traps: loaded.traps,
                        vault: loaded.vault,
                        record_generation: loaded.generation,
                        ..Default::default()
                    };
                    let mut warnings = loaded.warnings;
                    // Entity links dropped while loading, or a vault left behind by an
                    // offline /mapenlarge: write the fixed-up state back into the map.
                    let mut sync = crate::trap::persist::Sync::default();
                    let fixed = crate::trap::persist::relocate_if_needed(&mut state, &mut sync)
                        .and_then(|()| {
                            if warnings.is_empty() && sync.is_empty() {
                                Ok(())
                            } else {
                                crate::trap::persist::save(&mut state, &mut sync)
                            }
                        });
                    if let Err(e) = fixed {
                        warnings.push(format!("Could not update the trap vault: {e}"));
                    }

                    let map = state.map.as_ref().unwrap();
                    let world_size = map.size;
                    let out_bytes = map
                        .to_ogz_bytes_with(flate2::Compression::fast())
                        .map_err(|e: ogz_rs::OgzError| e.to_string())?;

                    tracing::info!(
                        "Map loaded: [Parse: {:?}] [Total: {:?}]",
                        t_parsed,
                        t_start.elapsed()
                    );
                    Ok((state, warnings, out_bytes, world_size))
                },
            )
            .await
            .expect("Map parsing panicked");

            match parse_result {
                Ok((loaded, warnings, out_bytes, world_size)) => {
                    update_server_items(loaded.map.as_ref().unwrap(), &server_clone);
                    let loaded_traps_len = loaded.traps.len();

                    let uploading_map_name = {
                        let mut s = get_server_state(&server_clone).write().unwrap();
                        let name = s
                            .uploading_map_name
                            .take()
                            .unwrap_or_else(|| "untitled".lossy_cubestring());
                        *s = loaded;
                        name
                    };
                    info!("id {id} uploaded a map (world_size={world_size}); state loaded");

                    *server_clone.map_name.write().unwrap() = uploading_map_name.clone();
                    *server_clone.game_mode.write().unwrap() = GameMode::CoopEdit;

                    server_clone.broadcast_chan1(&sauer_proto::channel1::ServMsg {
                        text: format!(
                            "\x0C1[MAP] \x0C7Loaded \x0C5{}\x0C7 traps ({} KB map).",
                            loaded_traps_len,
                            total_bytes / 1024
                        )
                        .lossy_cubestring(),
                    });
                    for w in warnings {
                        server_clone.broadcast_chan1(&sauer_proto::channel1::ServMsg {
                            text: format!("\x0C6[TRAP]\x0C7 {w}").lossy_cubestring(),
                        });
                    }

                    let pkt = sauer_proto::channel2::SendMap { data: out_bytes };
                    for client_id in server_clone.client_ids() {
                        server_clone.send_chan2(client_id, &pkt);
                    }

                    ui::send(&server_clone, id, ui::body("Map successfully uploaded!"));
                }
                Err(e) => warn!("id {id} sent an invalid map: {e}"),
            }
        });
    }

    async fn on_client_welcomed(&self, ctx: &mut Context) {
        let id = ctx.id();
        let server = ctx.server();

        // First one in gets master and locks the server, so nobody joins in on their editing.
        if server.client_count() == 1 {
            server.set_privilege(id, Privilege::Master);
            server.set_master_mode(MasterMode::Locked);
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

        if is_waiting_for_map(server) {
            server.set_spectator(id, true);
        }

        // The map and the info wall go out from map_crc once the client has loaded.
        self.pending_welcome.insert(id, WelcomeStep::InitialLoad);
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
        guard::geometry_edit(ctx, packet.sel.bounds(), |map| map.apply_edit_tex(packet));
    }

    async fn edit_mat(&self, ctx: &mut Context, packet: &channel1::EditMat) {
        guard::geometry_edit(ctx, packet.sel.bounds(), |map| map.apply_edit_mat(packet));
    }

    async fn del_cube(&self, ctx: &mut Context, packet: &channel1::DelCube) {
        guard::geometry_edit(ctx, packet.sel.bounds(), |map| {
            let ok = map.apply_del_cube(packet);
            if ok {
                // The Sauerbraten client's `delcube` command locally calls `makeempty`,
                // which resets materials to MAT_AIR. However, it does not send an explicit
                // EditMat packet. We must manually mirror this material wipe on the server.
                map.set_material(&packet.sel, Default::default());
            }
            ok
        });
    }

    async fn replace(&self, ctx: &mut Context, packet: &channel1::Replace) {
        let bounds = if packet.in_sel != 0 {
            packet.sel.bounds()
        } else {
            let s = get_server_state(ctx.server())
                .read()
                .unwrap()
                .map
                .as_ref()
                .map(|m| m.size)
                .unwrap_or(0);
            ([0, 0, 0], [s, s, s])
        };
        guard::geometry_edit(ctx, bounds, |map| map.apply_replace(packet));
    }

    async fn edit_face(&self, ctx: &mut Context, packet: &channel1::EditFace) {
        guard::geometry_edit(ctx, guard::edit_face_bounds(packet), |map| {
            map.apply_edit_face(packet)
        });
    }

    async fn edit_ent(&self, ctx: &mut Context, packet: &channel1::EditEnt) {
        guard::entity_edit(ctx, &[packet.ent_id as usize], |map| {
            map.apply_edit_ent(packet)
        });
    }

    async fn edit_var(&self, ctx: &mut Context, packet: &channel1::EditVar) {
        with_map_state_mut(ctx, |state| state.apply_edit_var(packet));
    }

    async fn edit_v_slot(&self, ctx: &mut Context, packet: &channel1::EditVSlot) {
        guard::geometry_edit(ctx, packet.sel.bounds(), |map| {
            map.apply_edit_v_slot(packet)
        });
    }

    async fn flip(&self, ctx: &mut Context, packet: &channel1::Flip) {
        let mut ok = true;
        let (min, max) = packet.sel.bounds();
        with_map_state_mut(ctx, |state| {
            if state.is_protected(min, max) {
                ok = false;
            }
        });
        if !ok {
            handle_edit_reject(ctx, min, max);
        }
    }

    async fn rotate(&self, ctx: &mut Context, packet: &channel1::Rotate) {
        let mut ok = true;
        let (min, max) = packet.sel.bounds();
        with_map_state_mut(ctx, |state| {
            if state.is_protected(min, max) {
                ok = false;
            }
        });
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
                p.clipboard = Some(sauer_server::ClipboardData { payload });
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

        let payload = server
            .players
            .try_get(id)
            .and_then(|p| p.clipboard.as_ref().map(|cb| cb.payload.clone()));
        if let Some(payload) = payload {
            let min = [packet.sel.o.x, packet.sel.o.y, packet.sel.o.z];
            let max = [
                min[0] + payload.sel.s.x * packet.sel.grid,
                min[1] + payload.sel.s.y * packet.sel.grid,
                min[2] + payload.sel.s.z * packet.sel.grid,
            ];
            guard::geometry_edit(ctx, (min, max), |map| map.apply_paste(packet, &payload));
        }
    }

    async fn undo(&self, ctx: &mut Context, packet: &channel1::UndoClient) {
        undo_redo(ctx, &packet.payload);
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
            state.vault = None;
            return;
        }

        // The base plugin already relayed the enlarge, so clients see it
        // before the new air octants and the relocated vault arrive.
        let mut sync = crate::trap::persist::Sync::default();
        let result = crate::trap::persist::client_enlarged(&mut state, &mut sync);
        let out = crate::trap::persist::packets(&state, sync);
        drop(state);
        crate::trap::persist::send(ctx.server(), out);
        if let Err(e) = result {
            ui::send_warn(
                ctx.server(),
                id,
                &format!("\x0C3[ERROR]\x0C7 Could not move the trap vault: {e}"),
            );
        }
    }

    async fn map_vote(&self, ctx: &mut Context, packet: &channel1::MapVote) {
        let id = ctx.id();

        if get_server_state(ctx.server())
            .read()
            .unwrap()
            .trap_edit
            .is_some()
        {
            ui::send_warn(ctx.server(), id, "Cannot vote map while editing a trap.");
            ctx.cancel();
            return;
        }
        if packet.mode != GameMode::CoopEdit {
            ctx.cancel();
            return;
        }

        ctx.cancel();
        let server = ctx.server();

        let mut state = get_server_state(server).write().unwrap();
        state.waiting_for_map = true;
        state.uploading_map_name = Some(packet.map_name.clone());
        drop(state);

        for client_id in server.client_ids() {
            server.set_spectator(client_id, true);
        }

        server.send_chan1(
            id,
            &channel1::MapChange {
                map_name: packet.map_name.clone(),
                mode: packet.mode,
                has_items: false,
            },
        );

        let player_name = server
            .players
            .try_get(id)
            .map(|p| p.name.strip_colors().to_utf8())
            .unwrap_or_else(|| "Someone".to_string());
        let map_str = packet.map_name.strip_colors().to_utf8();
        ui::broadcast_map_changed(server, &player_name, &map_str);
    }

    async fn redo(&self, ctx: &mut Context, packet: &channel1::RedoClient) {
        undo_redo(ctx, &packet.payload);
    }
}

fn undo_redo(ctx: &Context, payload: &sauer_proto::data::UndoPayload) {
    match &payload.data {
        sauer_proto::data::UndoData::Cubes { sel, .. } => {
            guard::geometry_edit(ctx, sel.bounds(), |map| map.apply_undo_redo(payload));
        }
        sauer_proto::data::UndoData::Entities(ents) => {
            let ids: Vec<usize> = ents.iter().map(|ue| ue.index as usize).collect();
            guard::entity_edit(ctx, &ids, |map| map.apply_undo_redo(payload));
        }
    }
}
