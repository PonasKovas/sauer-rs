use mlua::{UserData, UserDataMethods};
use sauer_server::Server;
use std::sync::Arc;

pub struct LuaServer {
    pub inner: Arc<Server>,
}

impl LuaServer {
    pub fn new(server: &Arc<Server>) -> Self {
        Self {
            inner: Arc::clone(server),
        }
    }
}

impl UserData for LuaServer {
    fn add_methods<M: UserDataMethods<Self>>(methods: &mut M) {
        methods.add_method("peer_address", |_lua, this, cn: u32| {
            Ok(this.inner.get_client(cn).addr().to_string())
        });
    }
}

pub struct LuaMutServer<'a> {
    pub inner: &'a mut Server,
}

impl<'a> LuaMutServer<'a> {
    pub fn new(server: &'a mut Server) -> Self {
        Self { inner: server }
    }
}

impl<'a> UserData for LuaMutServer<'a> {}
