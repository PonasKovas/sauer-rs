use crate::{
    LuaPlugin,
    extra::LuaExtraRegistry,
    userdata::{event::EventUD, server::LuaServer, spawn_state::SpawnStateUD},
};
use bytes::Bytes;
use cube_string::CubeString;
use sauer_proto::data::PlayerState;
use sauer_proto::{channel0, channel1, data::GameMode};
use sauer_server::{Context, ServerQueryContext, Plugin, Server};
use sauer_proto::server_query::{ServerQueryRequest, ExtInfoRequest};
use std::sync::Arc;
use tracing::error;

macro_rules! make_callback_impl {
    ( $( [$name:ident, $type:ty] ),* $(,)? ) => {
        $(
        async fn $name(&self, ctx: &mut Context, packet: &$type) {
            let mut event = EventUD::new();

            for callback in &self.callbacks.$name {
                let r = self.lua.scope(|scope| {
                    let packet = scope.create_userdata_ref(packet)?;
                    let event = scope.create_userdata_ref_mut(&mut event)?;

                    callback.callback.call::<()>((ctx.cn(), packet, event))
                });

                if let Err(e) = r {
                    error!(target: "lua", "[{}] {e}", callback.plugin_name);
                }

                if event.cancelled {
                    ctx.cancel();
                    break;
                }
            }
        }
        )*
    };
}

impl Plugin for LuaPlugin {
    async fn init(&self, server: &mut Server) {
        server.extra.insert(LuaExtraRegistry::new());
    }

    async fn start(&self, server: &Arc<Server>) {
        let s = match LuaServer::new(&self.lua, server) {
            Ok(s) => s,
            Err(e) => {
                error!("Failed to set up lua server global: {e}");
                return;
            }
        };

        if let Err(e) = self.lua.globals().set("server", s) {
            error!("Error setting `server` global in lua: {e}");
        }

        for callback in &self.callbacks.start {
            if let Err(e) = callback.callback.call::<()>(()) {
                error!(target: "lua", "[{}] {e}", callback.plugin_name);
            }
        }
    }

    async fn connected(&self, ctx: &mut Context) {
        let mut event = EventUD::new();

        for callback in &self.callbacks.connected {
            let r = self.lua.scope(|scope| {
                let event = scope.create_userdata_ref_mut(&mut event)?;
                callback.callback.call::<()>((ctx.cn(), event))
            });

            if let Err(e) = r {
                error!(target: "lua", "[{}] {e}", callback.plugin_name);
            }

            if event.cancelled {
                ctx.cancel();
                break;
            }
        }
    }

    async fn disconnected(&self, ctx: &mut Context) {
        let mut event = EventUD::new();

        for callback in &self.callbacks.disconnected {
            let r = self.lua.scope(|scope| {
                let event = scope.create_userdata_ref_mut(&mut event)?;
                callback.callback.call::<()>((ctx.cn(), event))
            });

            if let Err(e) = r {
                error!(target: "lua", "[{}] {e}", callback.plugin_name);
            }

            if event.cancelled {
                ctx.cancel();
                break;
            }
        }
    }

    async fn on_client_welcomed(&self, ctx: &mut Context) {
        let mut event = EventUD::new();

        for callback in &self.callbacks.on_client_welcomed {
            let r = self.lua.scope(|scope| {
                let event = scope.create_userdata_ref_mut(&mut event)?;
                callback.callback.call::<()>((ctx.cn(), event))
            });

            if let Err(e) = r {
                error!(target: "lua", "[{}] {e}", callback.plugin_name);
            }

            if event.cancelled {
                ctx.cancel();
                break;
            }
        }
    }

    async fn map_changed(&self, ctx: &mut Context, map_name: &CubeString, mode: GameMode) {
        let mut event = EventUD::new();

        for callback in &self.callbacks.map_changed {
            let r = self.lua.scope(|scope| {
                let event = scope.create_userdata_ref_mut(&mut event)?;
                callback
                    .callback
                    .call::<()>((ctx.cn(), map_name.clone(), mode, event))
            });

            if let Err(e) = r {
                error!(target: "lua", "[{}] {e}", callback.plugin_name);
            }

            if event.cancelled {
                ctx.cancel();
                break;
            }
        }
    }

    async fn on_spawn(&self, ctx: &mut Context, state: &mut PlayerState) {
        let mut ud = SpawnStateUD(state.clone());
        let mut event = EventUD::new();

        for callback in &self.callbacks.on_spawn {
            let r = self.lua.scope(|scope| {
                let state_ref = scope.create_userdata_ref_mut(&mut ud)?;
                let event_ref = scope.create_userdata_ref_mut(&mut event)?;
                callback
                    .callback
                    .call::<()>((ctx.cn(), state_ref, event_ref))
            });

            if let Err(e) = r {
                error!(target: "lua", "[{}] {e}", callback.plugin_name);
            }

            if event.cancelled {
                ctx.cancel();
                break;
            }
        }

        // Copy any modifications back regardless of cancellation — the caller
        // (handle_client) will drop the state if cancelled, so no harm done.
        *state = ud.0;
    }

    async fn on_chat(&self, ctx: &mut Context, text: &mut CubeString) {
        let mut event = EventUD::new();

        for callback in &self.callbacks.on_chat {
            let r = self.lua.scope(|scope| {
                let text_lua = self.lua.create_string(text.as_bytes())?;
                let event = scope.create_userdata_ref_mut(&mut event)?;
                callback.callback.call::<()>((ctx.cn(), text_lua, event))
            });

            if let Err(e) = r {
                error!(target: "lua", "[{}] {e}", callback.plugin_name);
            }

            if event.cancelled {
                ctx.cancel();
                break;
            }
        }
    }

    async fn on_command(&self, ctx: &mut Context, args: &[CubeString]) {
        let mut event = EventUD::new();

        for callback in &self.callbacks.on_command {
            let r = self.lua.scope(|scope| {
                let args_table = self.lua.create_table()?;
                for (i, arg) in args.iter().enumerate() {
                    args_table.raw_set(i + 1, self.lua.create_string(arg.as_bytes())?)?;
                }
                let event = scope.create_userdata_ref_mut(&mut event)?;
                callback.callback.call::<()>((ctx.cn(), args_table, event))
            });

            if let Err(e) = r {
                error!(target: "lua", "[{}] {e}", callback.plugin_name);
            }

            if event.cancelled {
                ctx.cancel();
                break;
            }
        }
    }

    async fn map(&self, ctx: &mut Context, packet: &Bytes) {
        let mut event = EventUD::new();

        for callback in &self.callbacks.map {
            let r = self.lua.scope(|scope| {
                let data = self.lua.create_string(packet.as_ref())?;
                let event = scope.create_userdata_ref_mut(&mut event)?;
                callback.callback.call::<()>((ctx.cn(), data, event))
            });

            if let Err(e) = r {
                error!(target: "lua", "[{}] {e}", callback.plugin_name);
            }

            if event.cancelled {
                ctx.cancel();
                break;
            }
        }
    }

    async fn server_query(&self, ctx: &mut ServerQueryContext, request: &ServerQueryRequest) {
        let mut event = EventUD::new();

        for callback in &self.callbacks.server_query {
            let r = self.lua.scope(|scope| {
                let req_table = self.lua.create_table()?;
                match request {
                    ServerQueryRequest::ServerInfo { ping_id } => {
                        req_table.set("type", "ServerInfo")?;
                        req_table.set("ping_id", *ping_id)?;
                    }
                    ServerQueryRequest::ExtInfo(ExtInfoRequest::Uptime) => {
                        req_table.set("type", "Uptime")?;
                    }
                    ServerQueryRequest::ExtInfo(ExtInfoRequest::PlayerStats { client_number }) => {
                        req_table.set("type", "PlayerStats")?;
                        if let Some(cn) = client_number {
                            req_table.set("client_number", *cn)?;
                        }
                    }
                    ServerQueryRequest::ExtInfo(ExtInfoRequest::TeamScore) => {
                        req_table.set("type", "TeamScore")?;
                    }
                }

                let event = scope.create_userdata_ref_mut(&mut event)?;
                callback.callback.call::<()>((ctx.addr().to_string(), req_table, event))
            });

            if let Err(e) = r {
                error!(target: "lua", "[{}] {e}", callback.plugin_name);
            }

            if event.cancelled {
                ctx.cancel();
                break;
            }
        }
    }

    make_callback_impl! {
        [pos, channel0::Position],
        [teleport, channel0::Teleport],
        [jumppad, channel0::JumpPad],

        [connect, channel1::Connect],
        [auth_answer, channel1::AuthAnswer],
        [ping, channel1::Ping],
        [from_ai, channel1::FromAi],
        [sound, channel1::Sound],
        [edit_mode, channel1::EditMode],
        [map_crc, channel1::MapCrc],
        [check_maps, channel1::CheckMaps],
        [try_spawn, channel1::TrySpawn],
        [gun_select, channel1::GunSelect],
        [spawn_client, channel1::SpawnClient],
        [suicide, channel1::Suicide],
        [shoot, channel1::Shoot],
        [explode, channel1::Explode],
        [item_pickup, channel1::ItemPickup],
        [text, channel1::Text],
        [say_team, channel1::SayTeamClient],
        [switch_name, channel1::SwitchName],
        [switch_model, channel1::SwitchModel],
        [switch_team, channel1::SwitchTeam],
        [map_vote, channel1::MapVote],
        [item_list, channel1::ItemList],
        [edit_ent, channel1::EditEnt],
        [edit_var, channel1::EditVar],
        [client_ping, channel1::ClientPing],
        [master_mode, channel1::MasterModeClient],
        [clear_bans, channel1::ClearBans],
        [kick, channel1::Kick],
        [spectator, channel1::SpectatorClient],
        [set_team, channel1::SetTeamClient],
        [force_intermission, channel1::ForceIntermission],
        [record_demo, channel1::RecordDemo],
        [stop_demo, channel1::StopDemo],
        [clear_demos, channel1::ClearDemos],
        [list_demos, channel1::ListDemos],
        [get_demo, channel1::GetDemo],
        [get_map, channel1::GetMap],
        [new_map, channel1::NewMap],
        [set_master, channel1::SetMaster],
        [add_bot, channel1::AddBot],
        [del_bot, channel1::DelBot],
        [bot_limit, channel1::BotLimit],
        [bot_balance, channel1::BotBalance],
        [auth_try, channel1::AuthTry],
        [auth_kick, channel1::AuthKick],
        [pause_game, channel1::PauseGameClient],
        [game_speed, channel1::GameSpeedClient],
        [copy, channel1::CopyClient],
        [paste, channel1::PasteClient],
        [clipboard, channel1::ClipboardClient],
        [undo, channel1::UndoClient],
        [redo, channel1::RedoClient],
        [edit_face, channel1::EditFace],
        [edit_tex, channel1::EditTex],
        [edit_mat, channel1::EditMat],
        [flip, channel1::Flip],
        [rotate, channel1::Rotate],
        [replace, channel1::Replace],
        [del_cube, channel1::DelCube],
        [edit_v_slot, channel1::EditVSlot],
        [remip, channel1::Remip],
        [serv_cmd, channel1::ServCmd],
        [bases, channel1::BasesClient],
        [rep_ammo, channel1::RepAmmoClient],
        [try_drop_flag, channel1::TryDropFlag],
        [take_flag, channel1::TakeFlagClient],
        [init_flags, channel1::InitFlagsClient],
        [init_tokens, channel1::InitTokensClient],
        [take_token, channel1::TakeTokenClient],
        [deposit_tokens, channel1::DepositTokensClient],
        [taunt, channel1::Taunt],
    }
}
