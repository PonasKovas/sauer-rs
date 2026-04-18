use crate::{
    LuaPlugin,
    userdata::{
        event::EventUD,
        server::{LuaMutServer, LuaServer},
    },
};
use bytes::Bytes;
use sauer_proto::{channel0, channel1};
use sauer_server::{
    Context, Plugin, Server,
    plugin::{BLOCK, Control, PASS},
};
use std::sync::Arc;
use tracing::error;

macro_rules! make_callback_impl {
    ( $( [$name:ident, $type:ty] ),* $(,)? ) => {
        $(
        async fn $name(&self, ctx: Context<'_>, packet: &$type) -> Control {
            let mut event = EventUD::new();

            for callback in &self.callbacks.$name {
                let inst = &self.lua_instances[callback.instance_idx];

                let r = inst.lua.scope(|scope| {
                    let packet = scope.create_userdata_ref(packet)?;
                    let event = scope.create_userdata_ref_mut(&mut event)?;

                    callback.callback.call::<()>((ctx.cn, packet, event))
                });

                if let Err(e) = r {
                    error!(target: "lua", "[{}] {e}", inst.plugin_name);
                }

                if event.cancelled {
                    return BLOCK;
                }
            }

            PASS
        }
        )*
    };
}

impl Plugin for LuaPlugin {
    async fn init(&self, server: &mut Server) {
        for callback in &self.callbacks.init {
            let inst = &self.lua_instances[callback.instance_idx];

            let r = inst.lua.scope(|scope| {
                let server = scope.create_userdata(LuaMutServer::new(server))?;

                callback.callback.call::<()>(server)
            });

            if let Err(e) = r {
                error!(target: "lua", "[{}] {e}", inst.plugin_name);
            }
        }
    }
    async fn start(&self, server: &Arc<Server>) {
        // initialize the `server` global in all instances
        for inst in &self.lua_instances {
            let s = LuaServer::new(server);

            if let Err(e) = inst.lua.globals().set("server", s) {
                let src = inst
                    .lua
                    .inspect_stack(1, |debug| debug.source().short_src.map(|s| s.into_owned()))
                    .flatten()
                    .unwrap_or("unknown".to_owned());

                error!("Error setting `server` global in lua plugin ({src}): {e}",);
            }
        }

        for callback in &self.callbacks.start {
            let inst = &self.lua_instances[callback.instance_idx];

            if let Err(e) = callback.callback.call::<()>(()) {
                error!(target: "lua", "[{}] {e}", inst.plugin_name);
            }
        }
    }
    async fn connected(&self, ctx: Context<'_>) -> Control {
        let mut event = EventUD::new();

        for callback in &self.callbacks.connected {
            let inst = &self.lua_instances[callback.instance_idx];

            let r = inst.lua.scope(|scope| {
                let event = scope.create_userdata_ref_mut(&mut event)?;

                callback.callback.call::<()>((ctx.cn, event))
            });

            if let Err(e) = r {
                error!(target: "lua", "[{}] {e}", inst.plugin_name);
            }

            if event.cancelled {
                return BLOCK;
            }
        }

        PASS
    }
    async fn disconnected(&self, ctx: Context<'_>) -> Control {
        let mut event = EventUD::new();

        for callback in &self.callbacks.disconnected {
            let inst = &self.lua_instances[callback.instance_idx];

            let r = inst.lua.scope(|scope| {
                let event = scope.create_userdata_ref_mut(&mut event)?;

                callback.callback.call::<()>((ctx.cn, event))
            });

            if let Err(e) = r {
                error!(target: "lua", "[{}] {e}", inst.plugin_name);
            }

            if event.cancelled {
                return BLOCK;
            }
        }

        PASS
    }

    async fn map(&self, ctx: Context<'_>, packet: &Bytes) -> Control {
        let mut event = EventUD::new();

        for callback in &self.callbacks.map {
            let inst = &self.lua_instances[callback.instance_idx];

            let r = inst.lua.scope(|scope| {
                let data = inst.lua.create_string(packet.as_ref())?;
                let event = scope.create_userdata_ref_mut(&mut event)?;

                callback.callback.call::<()>((ctx.cn, data, event))
            });

            if let Err(e) = r {
                error!(target: "lua", "[{}] {e}", inst.plugin_name);
            }

            if event.cancelled {
                return BLOCK;
            }
        }

        PASS
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
        [edit_mode, channel1::EditModeClient],
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
        [edit_ent, channel1::EditEntClient],
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
