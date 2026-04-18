use mlua::{UserData, UserDataMethods};
use sauer_server::Server;
use std::sync::Arc;

pub(crate) struct LuaServer {
    inner: Arc<Server>,
}

impl LuaServer {
    pub(crate) fn new(server: &Arc<Server>) -> Self {
        Self {
            inner: Arc::clone(server),
        }
    }
}

impl UserData for LuaServer {
    fn add_methods<M: UserDataMethods<Self>>(methods: &mut M) {
        methods.add_method("peer_address", |_lua, server, cn: u32| {
            Ok(server.inner.get_client(cn).addr().to_string())
        });
    }
}
