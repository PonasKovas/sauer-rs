use crate::aim::Aim;
use crate::map_state::{
    DeathrunState, get_server_state, is_waiting_for_map, update_server_items, with_map_state_mut,
};
use crate::trap::frame;
use crate::trap::{commands::Command, guard};
use crate::ui;
use bytes::Bytes;
use cube_string::{CubeString, IntoCubeStringLossy};
use dashmap::DashMap;
use parking_lot::RwLock;
use sauer_core::Aabb;
use sauer_kit::coop_edit;
use sauer_map::Map;
use sauer_proto::{
    Client, channel0,
    channel1::{self, EditMode},
    data::{Direction, EntityType, GameMode, MasterMode, PlayerState, Privilege},
};
use sauer_server::{
    Flow, Server, Status,
    plugin::{Context, Plugin},
};
use tracing::{info, warn};

pub const TRAP_EDIT_PARTICLE_ATTRS: [i32; 5] = [34, 100, 100, 0, 0];
pub const TRAP_EDIT_PARTICLE_YAW_OFFSET: f32 = 30.0;
pub const TRAP_EDIT_PARTICLE_PITCH_OFFSET: f32 = 0.0;
pub const TRAP_EDIT_PARTICLE_MAX_DIST: f32 = 1000.0;

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
    highlights: crate::trap::highlight::Highlights,
    pending_welcome: DashMap<u64, WelcomeStep>,
    notes: crate::ServerNotes,
}

impl DeathrunEdit {
    pub fn new() -> Self {
        Self {
            player_dirs: DashMap::new(),
            last_particle_update: DashMap::new(),
            highlights: Default::default(),
            pending_welcome: DashMap::new(),
            notes: Default::default(),
        }
    }

    /// Lines shown in every welcome wall and at the end of `#help` (see `ServerNotes`).
    pub fn with_notes(mut self, notes: Vec<String>) -> Self {
        self.notes = crate::ServerNotes::from_strings(notes);
        self
    }

    /// The particle beside the view of everyone while a trap is being edited,
    /// so they see their edits go into the trap. Moves at most every 100 ms.
    fn update_edit_particle(
        &self,
        server: &Server,
        id: u64,
        aim: &Aim,
        dir: Direction,
        editing: bool,
    ) {
        if !editing {
            if self.last_particle_update.remove(&id).is_some() {
                let mut state = get_server_state(server).write();
                state
                    .virt
                    .clear(id, "edit_particle")
                    .send(server, id, crate::SERVER_BOT_CN);
            }
            return;
        }
        let now = tokio::time::Instant::now();
        match self.last_particle_update.get_mut(&id) {
            Some(last) if now.duration_since(*last).as_millis() < 100 => return,
            Some(mut last) => *last = now,
            None => {
                self.last_particle_update.insert(id, now);
            }
        }

        // Off to the side of the view ray, a bit in front of whatever is there.
        let [f, r, u] = crate::aim::view_basis(dir.yaw as f32, dir.pitch as f32);
        let local = crate::aim::forward(
            TRAP_EDIT_PARTICLE_YAW_OFFSET,
            TRAP_EDIT_PARTICLE_PITCH_OFFSET,
        );
        let dir: [f32; 3] =
            std::array::from_fn(|i| r[i] * local[0] + f[i] * local[1] + u[i] * local[2]);
        let side = Aim {
            origin: aim.origin,
            forward: dir,
        };
        let dist = {
            let state = get_server_state(server).read();
            state
                .map
                .as_ref()
                .and_then(|map| {
                    sauer_map::raycast::raycast_with_filter(
                        map,
                        side.origin,
                        side.forward,
                        TRAP_EDIT_PARTICLE_MAX_DIST,
                        |c| !c.is_empty() || c.material != sauer_core::Material::default(),
                    )
                })
                .map_or(TRAP_EDIT_PARTICLE_MAX_DIST, |hit| (hit.dist - 1.0).max(1.0))
        };
        let particle = sauer_kit::virt_ents::FakeEnt {
            ent_type: EntityType::Particles,
            position: side.at(dist),
            attrs: TRAP_EDIT_PARTICLE_ATTRS,
        };
        let mut state = get_server_state(server).write();
        state
            .show_virt(id, "edit_particle", vec![particle])
            .send(server, id, crate::SERVER_BOT_CN);
    }
}

impl Plugin for DeathrunEdit {
    async fn disconnected(&self, ctx: &mut Context) {
        let id = ctx.id();
        self.player_dirs.remove(&id);
        self.last_particle_update.remove(&id);
        self.highlights.forget(id);
        self.pending_welcome.remove(&id);
        crate::trap::placement::forget_seethrough(ctx.server(), id);
        crate::trap::place::cancel_for(ctx.server(), id, "the player left");

        let mut state_w = get_server_state(ctx.server()).write();
        {
            let up = state_w.virt.clear_player(id);
            up.send(ctx.server(), id, crate::SERVER_BOT_CN);
        }
    }

    async fn init(&self, server: &mut Server) {
        self.notes.install(server);
        server.game.write().map_name = "".lossy_cubestring();
        server.game.write().mode = GameMode::CoopEdit;
        crate::add_server_bot(server);
        let state = DeathrunState {
            map: Some(Map::default_empty()),
            ..Default::default()
        };
        server.extra.insert(RwLock::new(state));
        server
            .extra
            .insert(crate::trap::placement::SeeThrough::default());

        Command::register_all(server);

        // Matches how play.rs reads the map: playerstart attr 2 is the team,
        // and good players finish by touching any flag.
        server.add_help_lines([
            ui::section("Map setup"),
            ui::tip("/newent playerstart 0 1", "good team spawn"),
            ui::tip("/newent playerstart 0 2", "terrorist spawn"),
            ui::tip(
                "/newent flag",
                "finish: good players who touch it complete the map",
            ),
        ]);
    }

    async fn pos(&self, ctx: &mut Context, packet: &channel0::Position) -> Flow {
        let id = ctx.id();
        self.player_dirs.insert(id, packet.dir);
        let server = ctx.server();
        let is_spectator = server
            .players
            .with(id, |p| p.status == Status::Spectator)
            .unwrap_or(true);
        let aim = Aim::new(packet.pos, packet.dir);

        if is_spectator {
            crate::trap::place::cancel_for(server, id, "the player went spectator");
        } else {
            crate::trap::place::follow(server, id, aim);
        }

        // The trap being edited, else the one whose button this player moves,
        // else the one whose button they aim at gets outlined.
        let (target, editing, seethrough, outline, entity_frames) =
            {
                let state = get_server_state(server).read();
                let hit_trap = (!is_spectator)
                    .then(|| state.trap_at(aim.hit_point(state.map.as_ref())))
                    .flatten();
                let edited = state.mode.editing().map(|e| e.trap);
                let moving = state
                    .mode
                    .placing()
                    .filter(|p| p.player == id)
                    .and_then(|p| p.trap);
                let target = edited.or(moving).or(hit_trap);
                let seethrough = hit_trap
                    .and_then(|t| state.trap(t))
                    .map(crate::trap::placement::button_key);

                let (outline, entity_frames) =
                    match target.and_then(|t| state.trap(t)) {
                        Some(trap) => {
                            // While editing, the frame follows the edit so far.
                            let live = state.mode.editing().filter(|e| e.trap == trap.id).and_then(
                                |edit| {
                                    crate::trap::commands::geometry_bounds(
                                        &edit.original_map,
                                        state.map.as_ref()?,
                                        trap.geom.as_ref(),
                                    )
                                },
                            );
                            let shown = live.or_else(|| trap.geom.as_ref().map(|g| g.clip_box()));
                            let outline =
                                shown.map(|b| frame::outline(b, Some(&aim), guard::GEOMETRY_COLOR));
                            let changes = guard::trap_ent_changes(&state, trap.id);
                            (outline, guard::entity_frames(&changes, Some(&aim)))
                        }
                        None => (None, Vec::new()),
                    };
                (target, edited.is_some(), seethrough, outline, entity_frames)
            };

        crate::trap::placement::update_seethrough_button(server, id, seethrough);
        let update = self.highlights.update(server, id, target, outline);
        // Frames around entities the trap deletes (red) or creates (green).
        if update.redrawn || update.target_changed {
            let mut state = get_server_state(server).write();
            state
                .show_virt(id, "trap_ents", entity_frames)
                .send(server, id, crate::SERVER_BOT_CN);
        }

        self.update_edit_particle(server, id, &aim, packet.dir, editing);
        Flow::Continue
    }

    async fn on_command(&self, ctx: &mut Context, args: &[CubeString]) -> Flow {
        let name = args.first().and_then(|s| s.as_str()).unwrap_or("");
        let Some(cmd) = Command::parse(name) else {
            return Flow::Continue;
        };
        let id = ctx.id();
        let server = ctx.server();
        let Some((spectating, pos)) = server
            .players
            .with(id, |p| (p.status == Status::Spectator, p.position))
        else {
            return Flow::Continue;
        };
        if spectating {
            ui::send_warn(server, id, "Cannot use trap commands while spectating.");
            return Flow::Continue;
        }
        let Some(dir) = self.player_dirs.get(&id).map(|d| *d) else {
            return Flow::Continue;
        };
        crate::trap::commands::run(ctx, cmd, args, Aim::new(pos, dir));
        Flow::Continue
    }

    async fn shoot(&self, ctx: &mut Context, packet: &channel1::Shoot) -> Flow {
        let id = ctx.id();
        let server = ctx.server().clone();

        let hit_pt = [packet.to.x, packet.to.y, packet.to.z];

        // Shooting while placing a button puts it down, like typing the command again.
        if let Some(kind) = crate::trap::place::placing_kind(&server, id) {
            let Some(pos) = server.players.with(id, |p| p.position) else {
                return Flow::Continue;
            };
            let Some(dir) = self.player_dirs.get(&id).map(|d| *d) else {
                return Flow::Continue;
            };
            crate::trap::place::command(&server, id, kind, Aim::new(pos, dir));
            return Flow::Continue;
        }

        let trap_to_trigger = {
            let state = crate::map_state::get_server_state(&server).read();
            if state.mode.editing().is_some() {
                return Flow::Continue;
            }

            state.trap_at(hit_pt)
        };

        if let Some(trap_idx) = trap_to_trigger {
            crate::trap::trigger_trap(&server, trap_idx, id);
        }
        Flow::Continue
    }

    async fn map_crc(&self, ctx: &mut Context, _packet: &channel1::MapCrc) -> Flow {
        let id = ctx.id();
        let server = ctx.server();
        // A freshly loaded map has every button solid again, and no placement preview.
        crate::trap::placement::mark_seethrough_stale_for(server, id);
        crate::trap::place::reshow_for(server, id);
        // A locked server keeps non-masters spectating, like vanilla.
        let locked = matches!(
            server.master_mode(),
            MasterMode::Locked | MasterMode::Private
        ) && server.get_privilege(id) < Privilege::Master;
        if !is_waiting_for_map(server) && !locked {
            let is_spectator = server
                .players
                .with(id, |p| p.status == Status::Spectator)
                .unwrap_or(false);
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
            let has_map = get_server_state(server).read().map.is_some();
            if matches!(step, WelcomeStep::InitialLoad) && has_map {
                crate::map_sync::send_map_file(server, id);
                info!("sent map state to id {id} after welcome");
                self.pending_welcome.insert(id, WelcomeStep::SentMap);
            } else {
                ui::send_welcome_edit(server, id);
            }
        }
        Flow::Continue
    }

    async fn edit_mode(&self, ctx: &mut Context, _packet: &channel1::EditMode) -> Flow {
        let id = ctx.id();
        let cn = ctx.server().get_cn(id).unwrap_or(sauer_proto::data::Cn(0));
        if is_waiting_for_map(ctx.server()) {
            ctx.server().send(
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
            return Flow::Cancel;
        }
        Flow::Continue
    }

    async fn on_spawn(&self, ctx: &mut Context, _state: &mut PlayerState) -> Flow {
        if is_waiting_for_map(ctx.server()) {
            return Flow::Cancel;
        }
        Flow::Continue
    }

    async fn map(&self, ctx: &mut Context, data: &Bytes) -> Flow {
        let id = ctx.id();

        let busy = get_server_state(ctx.server()).read().mode.busy_reason();
        if let Some(why) = busy {
            ui::send_warn(
                ctx.server(),
                id,
                &format!("Cannot upload a map while {why}."),
            );
            return Flow::Cancel;
        }
        let server = ctx.server();

        let is_spectator = server
            .players
            .with(id, |p| p.status == Status::Spectator)
            .unwrap_or(false);
        let is_master = server.get_privilege(id) >= Privilege::Master;

        if is_spectator && !is_master {
            ui::send_warn(server, id, "You must be unspectated to upload a map.");
            return Flow::Cancel;
        }

        let size_kb = data.len() / 1024;
        let player_name = crate::ui::player_name(server, id);

        server.broadcast(&sauer_proto::channel1::ServMsg {
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

                    sauer_kit::virt_ents::strip(&mut map.entities);

                    let loaded = crate::trap::persist::load(&map);
                    crate::trap::placement::rebuild_all_trap_buttons(&mut map, &loaded.traps);
                    let mut protected = sauer_kit::coop_edit::Protected::default();
                    for trap in &loaded.traps {
                        protected.protect(trap.button_box());
                    }
                    if let Some(v) = loaded.vault {
                        protected.protect(v.bounds());
                    }

                    let mut state = DeathrunState {
                        map: Some(map),
                        protected,
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
                        .to_ogz_bytes_merged(flate2::Compression::fast())
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
                    let loaded_traps_len = loaded.traps.len();

                    let uploading_map_name = {
                        let mut s = get_server_state(&server_clone).write();
                        // Someone started editing or placing while the map was parsed.
                        if let Some(why) = s.mode.busy_reason() {
                            drop(s);
                            ui::send_warn(
                                &server_clone,
                                id,
                                &format!("Map upload dropped: {why}. Send it again later."),
                            );
                            return;
                        }
                        update_server_items(loaded.map.as_ref().unwrap(), &server_clone);
                        let name = s
                            .uploading_map_name
                            .take()
                            .unwrap_or_else(|| "untitled".lossy_cubestring());
                        *s = loaded;
                        name
                    };
                    info!("id {id} uploaded a map (world_size={world_size}); state loaded");

                    {
                        let mut game = server_clone.game.write();
                        game.map_name = uploading_map_name.clone();
                        game.mode = GameMode::CoopEdit;
                    }

                    server_clone.broadcast(&sauer_proto::channel1::ServMsg {
                        text: format!(
                            "\x0C1[MAP] \x0C7Loaded \x0C5{}\x0C7 traps ({} KB map).",
                            loaded_traps_len,
                            total_bytes / 1024
                        )
                        .lossy_cubestring(),
                    });
                    for w in warnings {
                        server_clone.broadcast(&sauer_proto::channel1::ServMsg {
                            text: format!("\x0C6[TRAP]\x0C7 {w}").lossy_cubestring(),
                        });
                    }

                    let pkt = sauer_proto::channel2::SendMap { data: out_bytes };
                    for client_id in server_clone.client_ids() {
                        server_clone.send(client_id, &pkt);
                    }

                    ui::send(&server_clone, id, ui::body("Map successfully uploaded!"));
                }
                Err(e) => warn!("id {id} sent an invalid map: {e}"),
            }
        });
        Flow::Cancel
    }

    async fn on_client_welcomed(&self, ctx: &mut Context) {
        let id = ctx.id();
        let server = ctx.server();

        // First one in gets master and locks the server, so nobody joins in on their editing.
        if server.client_count() == 1 {
            server.set_privilege(id, Privilege::Master);
            server.set_master_mode(MasterMode::Locked);
        }

        if is_waiting_for_map(server) {
            server.set_spectator(id, true);
        }

        // The map and the info wall go out from map_crc once the client has loaded.
        self.pending_welcome.insert(id, WelcomeStep::InitialLoad);
    }

    async fn get_map(&self, ctx: &mut Context, _packet: &channel1::GetMap) -> Flow {
        let id = ctx.id();
        crate::map_sync::send_map_file(ctx.server(), id);
        info!("sent map state to id {id} on N_GETMAP");
        Flow::Cancel
    }

    async fn spectator(&self, ctx: &mut Context, packet: &channel1::SpectatorClient) -> Flow {
        let id = ctx.id();
        if !packet.val && is_waiting_for_map(ctx.server()) {
            ui::send_warn(
                ctx.server(),
                id,
                "Waiting for map upload. Use /sendmap to proceed.",
            );
            return Flow::Cancel;
        }
        Flow::Continue
    }

    // ── Coop-edit map tracking + Interceptors + Trap immunities ──────────────────────

    async fn edit_tex(&self, ctx: &mut Context, packet: &channel1::EditTex) -> Flow {
        guard::geometry_edit(ctx, packet.sel.bounds(), |map| {
            coop_edit::edit_tex(map, packet)
        });
        Flow::Continue
    }

    async fn edit_mat(&self, ctx: &mut Context, packet: &channel1::EditMat) -> Flow {
        guard::geometry_edit(ctx, packet.sel.bounds(), |map| {
            coop_edit::edit_mat(map, packet)
        });
        Flow::Continue
    }

    async fn del_cube(&self, ctx: &mut Context, packet: &channel1::DelCube) -> Flow {
        guard::geometry_edit(ctx, packet.sel.bounds(), |map| {
            coop_edit::del_cube(map, packet)
        });
        Flow::Continue
    }

    async fn replace(&self, ctx: &mut Context, packet: &channel1::Replace) -> Flow {
        let size = get_server_state(ctx.server())
            .read()
            .map
            .as_ref()
            .map_or(0, |m| m.size);
        let bounds = match packet.in_sel {
            true => packet.sel.bounds(),
            false => Aabb::cube([0; 3], size),
        };
        guard::geometry_edit(ctx, bounds, |map| coop_edit::replace(map, packet));
        Flow::Continue
    }

    async fn edit_face(&self, ctx: &mut Context, packet: &channel1::EditFace) -> Flow {
        let bounds = coop_edit::edit_face_bounds(&packet.sel, packet.dir, packet.mode as i32);
        guard::geometry_edit(ctx, bounds, |map| coop_edit::edit_face(map, packet));
        Flow::Continue
    }

    async fn edit_ent(&self, ctx: &mut Context, packet: &channel1::EditEnt) -> Flow {
        let pos = coop_edit::entity_box([packet.pos.x, packet.pos.y, packet.pos.z]);
        guard::entity_edit(ctx, &[packet.ent_id as usize], &[pos], |map| {
            coop_edit::edit_ent(map, packet)
        });
        Flow::Continue
    }

    async fn edit_var(&self, ctx: &mut Context, packet: &channel1::EditVar) -> Flow {
        with_map_state_mut(ctx, |map| coop_edit::edit_var(map, packet));
        Flow::Continue
    }

    async fn edit_v_slot(&self, ctx: &mut Context, packet: &channel1::EditVSlot) -> Flow {
        guard::geometry_edit(ctx, packet.sel.bounds(), |map| {
            coop_edit::edit_vslot(map, packet)
        });
        Flow::Continue
    }

    // The server doesn't track flips and rotations yet; it only refuses them
    // where edits may not go.
    async fn flip(&self, ctx: &mut Context, packet: &channel1::Flip) -> Flow {
        guard::refuse_if_blocked(ctx, packet.sel.bounds());
        Flow::Continue
    }

    async fn rotate(&self, ctx: &mut Context, packet: &channel1::Rotate) -> Flow {
        guard::refuse_if_blocked(ctx, packet.sel.bounds());
        Flow::Continue
    }

    async fn copy(&self, ctx: &mut Context, packet: &channel1::CopyClient) -> Flow {
        let id = ctx.id();
        let server = ctx.server().clone();

        if server.game.read().mode != GameMode::CoopEdit {
            return Flow::Continue;
        }

        server.players.with_mut(id, |p| p.clipboard = None);

        if let Some(state) = crate::map_state::get_server_state(&server)
            .write()
            .map
            .as_mut()
        {
            if let Some(sel) = packet.sel.validate(state.size) {
                let payload = state.copy_selection(&sel);
                server.players.with_mut(id, |p| p.clipboard = Some(payload));
            }
        }

        let cn = server.get_cn(id).unwrap_or(sauer_proto::data::Cn(0));
        server.broadcast_except(
            id,
            &sauer_proto::Client {
                client_num: cn,
                packet: sauer_proto::channel1::CopyServer {
                    sel: packet.sel.clone(),
                },
            },
        );
        Flow::Cancel
    }

    async fn paste(&self, ctx: &mut Context, packet: &channel1::PasteClient) -> Flow {
        let id = ctx.id();
        let server = ctx.server().clone();

        let payload = server.players.with(id, |p| p.clipboard.clone()).flatten();
        if let Some(payload) = payload {
            let mut sel = packet.sel.clone();
            sel.s = payload.sel.s;
            guard::geometry_edit(ctx, sel.bounds(), |map| {
                coop_edit::paste(map, packet, &payload)
            });
        }
        Flow::Continue
    }

    async fn undo(&self, ctx: &mut Context, packet: &channel1::UndoClient) -> Flow {
        undo_redo(ctx, &packet.payload);
        Flow::Continue
    }

    async fn new_map(&self, ctx: &mut Context, packet: &channel1::NewMap) -> Flow {
        let id = ctx.id();
        let server = ctx.server();

        let busy = get_server_state(server).read().mode.busy_reason();
        if let Some(why) = busy {
            ui::send_warn(server, id, &format!("Cannot use newmap while {why}."));
            crate::map_sync::send_map_file(ctx.server(), id);
            return Flow::Cancel;
        }
        let server = ctx.server();
        let mut state = get_server_state(server).write();
        if packet.size >= 0 {
            let world_size = 1_i32 << packet.size.clamp(10, 16);
            // A new map starts over: no traps, vault, protections or fake entities.
            *state = DeathrunState {
                map: Some(Map::new_empty(world_size)),
                ..Default::default()
            };
            return Flow::Continue;
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
        Flow::Continue
    }

    async fn map_vote(&self, ctx: &mut Context, packet: &channel1::MapVote) -> Flow {
        let id = ctx.id();

        let busy = get_server_state(ctx.server()).read().mode.busy_reason();
        if let Some(why) = busy {
            ui::send_warn(
                ctx.server(),
                id,
                &format!("Cannot vote for a map while {why}."),
            );
            return Flow::Cancel;
        }
        if packet.mode != GameMode::CoopEdit {
            return Flow::Cancel;
        }

        let server = ctx.server();

        let mut state = get_server_state(server).write();
        state.waiting_for_map = true;
        state.uploading_map_name = Some(packet.map_name.clone());
        drop(state);

        for client_id in server.client_ids() {
            server.set_spectator(client_id, true);
        }

        server.send(
            id,
            &channel1::MapChange {
                map_name: packet.map_name.clone(),
                mode: packet.mode,
                has_items: false,
            },
        );

        let player_name = crate::ui::player_name(server, id);
        let map_str = packet.map_name.strip_colors().to_utf8();
        ui::broadcast_map_changed(server, &player_name, &map_str);
        Flow::Cancel
    }

    async fn redo(&self, ctx: &mut Context, packet: &channel1::RedoClient) -> Flow {
        undo_redo(ctx, &packet.payload);
        Flow::Continue
    }
}

fn undo_redo(ctx: &Context, payload: &sauer_proto::data::UndoPayload) {
    match payload {
        sauer_proto::data::UndoPayload::Cubes { sel, .. } => {
            guard::geometry_edit(ctx, sel.bounds(), |map| coop_edit::undo_redo(map, payload));
        }
        sauer_proto::data::UndoPayload::Entities(ents) => {
            let ids: Vec<usize> = ents.iter().map(|ue| ue.index as usize).collect();
            let boxes: Vec<Aabb> = ents
                .iter()
                .map(|ue| coop_edit::entity_box(ue.ent.position))
                .collect();
            guard::entity_edit(ctx, &ids, &boxes, |map| coop_edit::undo_redo(map, payload));
        }
    }
}
