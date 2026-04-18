use crate::{Instance, LuaPlugin, LuaPluginOptions};
use mlua::Lua;

#[derive(Default)]
pub(crate) struct Callbacks {
    pub init: Vec<LuaCallback>,
    pub start: Vec<LuaCallback>,
    pub connected: Vec<LuaCallback>,
    pub disconnected: Vec<LuaCallback>,
    pub pos: Vec<LuaCallback>,
    pub teleport: Vec<LuaCallback>,
    pub jumppad: Vec<LuaCallback>,
}

pub(crate) struct LuaCallback {
    pub instance_idx: usize,
    pub callback: mlua::Function,
}

pub(crate) fn add_callbacks(
    _options: &LuaPluginOptions,
    luaplugin: &mut LuaPlugin,
    name: &str,
    lua: Lua,
) -> Result<(), mlua::Error> {
    let globals = lua.globals();

    let instance_idx = luaplugin.lua_instances.len();

    macro_rules! add_callback {
        ($name:ident) => {
            if globals.contains_key(stringify!($name))? {
                luaplugin.callbacks.$name.push(LuaCallback {
                    instance_idx,
                    callback: globals.get(stringify!($name))?,
                });
            }
        };
    }

    add_callback!(init);
    add_callback!(start);
    add_callback!(connected);
    add_callback!(disconnected);
    add_callback!(pos);

    luaplugin.lua_instances.push(Instance {
        lua,
        plugin_name: name.to_owned(),
    });

    Ok(())
}
