use crate::{
    LuaPlugin,
    callbacks::LuaCallback,
    extra::LuaExtraRegistry,
    userdata::{
        event::EventUD,
        hook_value::{ChatUD, DamageUD},
        server::LuaServer,
        spawn_state::SpawnStateUD,
    },
};
use bytes::Bytes;
use cube_string::CubeString;
use mlua::{IntoLuaMulti, MultiValue, Scope, Value};
use sauer_proto::data::{GameMode, PlayerState};
use sauer_proto::server_query::{ExtInfoRequest, ServerQueryRequest};
use sauer_server::{Context, Flow, Plugin, Server, ServerQueryContext};
use std::sync::Arc;
use tracing::error;

impl LuaPlugin {
    /// Calls each callback with what `args` builds plus an `Event` it can
    /// cancel, and stops at the first one that does.
    fn dispatch<'env>(
        &self,
        callbacks: &[LuaCallback],
        args: impl for<'s> Fn(&'s Scope<'s, 'env>) -> mlua::Result<MultiValue>,
    ) -> Flow {
        for callback in callbacks {
            let r = self.lua.scope(|scope| {
                let event = self.lua.create_userdata(EventUD::new())?;
                let mut values = args(scope)?;
                values.push_back(Value::UserData(event.clone()));
                callback.callback.call::<()>(values)?;
                Ok(event.borrow::<EventUD>()?.cancelled)
            });
            match r {
                Ok(true) => return Flow::Cancel,
                Ok(false) => {}
                Err(e) => error!(target: "lua", "[{}] {e}", callback.plugin_name),
            }
        }
        Flow::Continue
    }

    /// Calls each callback with what `args` builds. For hooks that can't be
    /// cancelled, so there's no `Event`.
    fn notify(&self, callbacks: &[LuaCallback], args: impl Fn() -> mlua::Result<MultiValue>) {
        for callback in callbacks {
            if let Err(e) = args().and_then(|a| callback.callback.call::<()>(a)) {
                error!(target: "lua", "[{}] {e}", callback.plugin_name);
            }
        }
    }
}

macro_rules! packet_callbacks {
    (
        ()
        chan0 { $($c0:tt)* }
        chan1 { $($c1:tt)* }
    ) => {
        packet_callbacks! { @each $($c0)* $($c1)* }
    };
    (@each $( [$msg_id:ident] $method:ident ($packet:path) $variant:ident, )*) => {
        $(
            async fn $method(&self, ctx: &mut Context, packet: &$packet) -> Flow {
                self.dispatch(&self.callbacks.$method, |scope| {
                    (ctx.id(), scope.create_userdata_ref(packet)?).into_lua_multi(&self.lua)
                })
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

        self.notify(&self.callbacks.start, || Ok(MultiValue::new()));
    }

    async fn connected(&self, ctx: &mut Context) -> Flow {
        self.dispatch(&self.callbacks.connected, |_| {
            ctx.id().into_lua_multi(&self.lua)
        })
    }

    async fn disconnected(&self, ctx: &mut Context) {
        self.notify(&self.callbacks.disconnected, || {
            ctx.id().into_lua_multi(&self.lua)
        });
    }

    async fn on_client_welcomed(&self, ctx: &mut Context) {
        self.notify(&self.callbacks.on_client_welcomed, || {
            ctx.id().into_lua_multi(&self.lua)
        });
    }

    async fn map_changed(&self, ctx: &mut Context, map_name: &CubeString, mode: GameMode) {
        self.notify(&self.callbacks.map_changed, || {
            (ctx.id(), map_name.clone(), mode).into_lua_multi(&self.lua)
        });
    }

    async fn on_spawn(&self, ctx: &mut Context, state: &mut PlayerState) -> Flow {
        let ud = match self.lua.create_userdata(SpawnStateUD(state.clone())) {
            Ok(ud) => ud,
            Err(e) => {
                error!(target: "lua", "{e}");
                return Flow::Continue;
            }
        };
        let flow = self.dispatch(&self.callbacks.on_spawn, |_| {
            (ctx.id(), ud.clone()).into_lua_multi(&self.lua)
        });
        // Changes go back even when cancelled; a cancelled spawn drops them.
        if let Ok(changed) = ud.borrow::<SpawnStateUD>() {
            *state = changed.0.clone();
        }
        flow
    }

    async fn on_chat(&self, ctx: &mut Context, text: &mut CubeString) -> Flow {
        let Ok(ud) = self.lua.create_userdata(ChatUD { text: text.clone() }) else {
            return Flow::Continue;
        };
        let flow = self.dispatch(&self.callbacks.on_chat, |_| {
            (ctx.id(), ud.clone()).into_lua_multi(&self.lua)
        });
        if let Ok(changed) = ud.borrow::<ChatUD>() {
            *text = changed.text.clone();
        }
        flow
    }

    async fn on_damage(
        &self,
        _ctx: &mut Context,
        target_id: u64,
        actor_id: u64,
        damage: &mut i32,
    ) -> Flow {
        let Ok(ud) = self.lua.create_userdata(DamageUD { amount: *damage }) else {
            return Flow::Continue;
        };
        let flow = self.dispatch(&self.callbacks.on_damage, |_| {
            (target_id, actor_id, ud.clone()).into_lua_multi(&self.lua)
        });
        if let Ok(changed) = ud.borrow::<DamageUD>() {
            *damage = changed.amount;
        }
        flow
    }

    async fn on_player_died(&self, _ctx: &mut Context, victim_id: u64, actor_id: u64) {
        self.notify(&self.callbacks.on_player_died, || {
            (victim_id, actor_id).into_lua_multi(&self.lua)
        });
    }

    async fn on_command(&self, ctx: &mut Context, args: &[CubeString]) -> Flow {
        self.dispatch(&self.callbacks.on_command, |_| {
            let table = self.lua.create_table()?;
            for (i, arg) in args.iter().enumerate() {
                table.raw_set(i + 1, self.lua.create_string(arg.as_bytes())?)?;
            }
            (ctx.id(), table).into_lua_multi(&self.lua)
        })
    }

    async fn map(&self, ctx: &mut Context, packet: &Bytes) -> Flow {
        if self.callbacks.map.is_empty() {
            return Flow::Continue;
        }
        let data = match self.lua.create_string(packet.as_ref()) {
            Ok(s) => s,
            Err(e) => {
                error!(target: "lua", "{e}");
                return Flow::Continue;
            }
        };
        self.dispatch(&self.callbacks.map, |_| {
            (ctx.id(), data.clone()).into_lua_multi(&self.lua)
        })
    }

    async fn server_query(
        &self,
        ctx: &mut ServerQueryContext,
        request: &ServerQueryRequest,
    ) -> Flow {
        self.dispatch(&self.callbacks.server_query, |_| {
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
            (ctx.addr().to_string(), req_table).into_lua_multi(&self.lua)
        })
    }

    sauer_server::for_each_packet_hook!(packet_callbacks);
}
