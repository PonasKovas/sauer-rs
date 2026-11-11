mod indicators;
mod upload;

use crate::aim::Aim;
use crate::map_state::{DeathrunState, get_server_state, is_waiting_for_map, with_map_state_mut};
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
    data::{Direction, GameMode, MasterMode, PlayerState, Privilege},
};
use sauer_server::{
    Flow, Server, Status,
    plugin::{Context, Plugin},
};
use tracing::info;

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
            map: Map::default_empty(),
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
                    .then(|| state.trap_at(aim.hit_point(&state.map)))
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
                                        &state.map,
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
            if matches!(step, WelcomeStep::InitialLoad) {
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
        upload::receive(ctx.server(), ctx.id(), data)
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
        let size = get_server_state(ctx.server()).read().map.size;
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

        let payload = {
            let mut state = get_server_state(&server).write();
            let map = &mut state.map;
            packet
                .sel
                .validate(map.size)
                .map(|sel| map.copy_selection(&sel))
        };
        if let Some(payload) = payload {
            server.players.with_mut(id, |p| p.clipboard = Some(payload));
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
                map: Map::new_empty(world_size),
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
        upload::vote(ctx.server(), ctx.id(), packet)
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
