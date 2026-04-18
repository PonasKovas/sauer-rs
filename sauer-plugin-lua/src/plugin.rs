use crate::{
    LuaPlugin,
    userdata::{
        event::EventUD,
        server::{LuaMutServer, LuaServer},
    },
};
use sauer_proto::channel0;
use sauer_server::{
    Context, Plugin, Server,
    plugin::{BLOCK, Control, PASS},
};
use std::sync::Arc;
use tracing::error;

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

    async fn pos(&self, ctx: Context<'_>, packet: &channel0::Position) -> Control {
        let mut event = EventUD::new();

        for callback in &self.callbacks.pos {
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
}
