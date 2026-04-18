use crate::{LuaPlugin, plugin::server::LuaServer};
use sauer_server::{Plugin, Server};
use std::sync::Arc;
use tracing::error;

mod server;

impl Plugin for LuaPlugin {
    async fn init(&self, _server: &mut Server) {
        for callback in &self.init {
            if let Err(e) = callback.callback.call::<()>(()) {
                error!(target: "lua", "[{}] {e}", callback.plugin_name);
            }
        }
    }
    async fn start(&self, server: &Arc<Server>) {
        // initialize the `server` global in all instances
        for lua in &self.lua_instances {
            let s = LuaServer::new(server);

            if let Err(e) = lua.globals().set("server", s) {
                let src = lua
                    .inspect_stack(1, |debug| debug.source().short_src.map(|s| s.into_owned()))
                    .flatten()
                    .unwrap_or("unknown".to_owned());

                error!("Error setting `server` global in lua plugin ({src}): {e}",);
            }
        }

        for callback in &self.start {
            if let Err(e) = callback.callback.call::<()>(()) {
                error!(target: "lua", "[{}] {e}", callback.plugin_name);
            }
        }
    }
}
