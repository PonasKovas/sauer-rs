use crate::extra::LuaExtraRegistry;
use mlua::{IntoLua, Lua, RegistryKey, Table, UserData, UserDataFields, UserDataMethods, Value};
use sauer_server::Server;
use std::sync::Arc;

pub struct LuaServer {
    pub inner: Arc<Server>,
    extra_key: RegistryKey,
}

impl LuaServer {
    /// Creates a new `LuaServer` and builds the `server.extra` table.
    ///
    /// The extra table is a plain Lua table with a metatable whose `__index`
    /// falls through to [`LuaExtraRegistry`] on miss, and whose `__newindex`
    /// uses `rawset` so Lua writes land directly in the table without looping
    /// back through the metamethod.
    pub fn new(lua: &Lua, server: &Arc<Server>) -> mlua::Result<Self> {
        let extra_table = lua.create_table()?;

        let mt = lua.create_table()?;

        // __index: on miss, look up the Rust factory registry
        let server_clone = Arc::clone(server);
        mt.set(
            "__index",
            lua.create_function(move |lua, (_t, key): (Value, String)| {
                let factory = server_clone
                    .extra
                    .get::<LuaExtraRegistry>()
                    .and_then(|r| r.entries.get(&key))
                    .map(Arc::clone);

                match factory {
                    Some(f) => f(lua),
                    None => Ok(Value::Nil),
                }
            })?,
        )?;

        // __newindex: rawset so subsequent reads hit the table directly
        mt.set(
            "__newindex",
            lua.create_function(|_lua, (t, k, v): (Table, Value, Value)| t.raw_set(k, v))?,
        )?;

        extra_table.set_metatable(Some(mt))?;

        let extra_key = lua.create_registry_value(extra_table)?;

        Ok(Self {
            inner: Arc::clone(server),
            extra_key,
        })
    }
}

impl UserData for LuaServer {
    fn add_fields<F: UserDataFields<Self>>(fields: &mut F) {
        fields.add_field_method_get("extra", |lua, this| {
            let table: Table = lua.registry_value(&this.extra_key)?;
            table.into_lua(lua)
        });
    }

    fn add_methods<M: UserDataMethods<Self>>(methods: &mut M) {
        methods.add_method("peer_address", |_lua, this, cn: u32| {
            Ok(this.inner.get_client(cn).addr().to_string())
        });
    }
}
