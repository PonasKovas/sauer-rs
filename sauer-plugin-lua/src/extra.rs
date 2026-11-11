use mlua::{Lua, Value};
use std::collections::HashMap;
use std::sync::Arc;

/// Registry for exposing Rust data to Lua plugins via `server.extra.<name>`.
///
/// Inserted into `Server::extra` by `LuaPlugin` during `Plugin::init`.
/// Any Rust plugin that wants its data accessible from Lua registers a named
/// factory closure here during its own `Plugin::init`.
///
/// # Ordering
///
/// A plugin tuple runs `init` left to right, and `ServerBuilder::plugin` adds
/// to the right. So add `LuaPlugin` **before** any plugin that registers here,
/// and `LuaExtraRegistry` is already in `server.extra` when that plugin
/// initialises: `.plugin(lua).plugin(my_plugin)`.
///
/// # Example
///
/// ```rust,ignore
/// // 1. Wrap your data in an Arc so the factory can cheaply clone it.
/// let data = Arc::new(RwLock::new(MyData::new()));
///
/// // 2. Register in Plugin::init (after LuaPlugin has already run its init).
/// server.extra
///     .get_mut::<LuaExtraRegistry>()
///     .expect("LuaPlugin must be added before this plugin")
///     .register("my_data", {
///         let d = Arc::clone(&data);
///         move |lua| lua.create_userdata(MyWrapper(Arc::clone(&d)))
///     });
/// ```
///
/// ```lua
/// -- From any Lua plugin:
/// local d = server.extra.my_data
/// ```
///
/// `MyWrapper` must implement [`mlua::UserData`].
pub struct LuaExtraRegistry {
    pub(crate) entries: HashMap<String, Arc<dyn Fn(&Lua) -> mlua::Result<Value> + Send + Sync>>,
}

impl LuaExtraRegistry {
    pub fn new() -> Self {
        Self {
            entries: HashMap::new(),
        }
    }

    /// Register a named factory.
    ///
    /// The factory is called with the current [`Lua`] instance each time
    /// `server.extra.<name>` is read from Lua and the key is not already in
    /// the Lua-side store.  It typically captures an `Arc<T>` and returns
    /// `lua.create_userdata(MyWrapper(Arc::clone(&data)))`.
    pub fn register<F>(&mut self, name: impl Into<String>, factory: F)
    where
        F: Fn(&Lua) -> mlua::Result<Value> + Send + Sync + 'static,
    {
        self.entries.insert(name.into(), Arc::new(factory));
    }
}

impl Default for LuaExtraRegistry {
    fn default() -> Self {
        Self::new()
    }
}
