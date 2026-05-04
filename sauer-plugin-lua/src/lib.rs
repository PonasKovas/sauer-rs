#![allow(unused_doc_comments)]

use crate::{
    callbacks::{Callbacks, add_callbacks_from_env},
    error::LuaPluginErrorExt,
    setup_globals::setup_plugin_env,
};
use mlua::Lua;
use tokio::fs::{self, DirEntry};

mod callbacks;
mod error;
mod extra;
mod options;
mod plugin;
mod setup_globals;
mod userdata;

pub use error::LuaPluginLoadError;
pub use extra::LuaExtraRegistry;
pub use options::LuaPluginOptions;

pub struct LuaPlugin {
    pub(crate) lua: Lua,
    pub(crate) callbacks: Callbacks,
}

impl LuaPlugin {
    pub async fn load(options: &LuaPluginOptions) -> Result<Self, LuaPluginLoadError> {
        let lua = Lua::new();
        let mut callbacks = Callbacks::default();

        let mut entries = fs::read_dir(&options.plugin_directory)
            .await
            .in_path(&options.plugin_directory)?;

        while let Some(entry) = entries
            .next_entry()
            .await
            .in_path(&options.plugin_directory)?
        {
            load_plugin_entry(&entry, &lua, &mut callbacks)
                .await
                .in_path(&entry.path())?;
        }

        Ok(Self { lua, callbacks })
    }
}

async fn load_plugin_entry(
    entry: &DirEntry,
    lua: &Lua,
    callbacks: &mut Callbacks,
) -> Result<(), LuaPluginLoadError> {
    let file_type = entry.file_type().await?;
    let mut path = entry.path();
    let name = path
        .file_stem()
        .unwrap()
        .to_str()
        .ok_or(LuaPluginLoadError::NonUtf8Path)?
        .to_owned();
    let entry_path_str = path
        .to_str()
        .ok_or(LuaPluginLoadError::NonUtf8Path)?
        .to_owned();

    if file_type.is_dir() {
        let package: mlua::Table = lua.globals().get("package")?;
        let current_path: String = package.get("path")?;
        package.set("path", format!("{entry_path_str}/?.lua;{current_path}"))?;
        path.push("main.lua");
    }

    // Each plugin file gets its own environment table so that global writes
    // (e.g. `function connected()`) don't collide across plugins. Reads that
    // aren't in the env fall through to the real `_G` via `__index`.
    let env = lua.create_table()?;
    let mt = lua.create_table()?;
    mt.set("__index", lua.globals())?;
    env.set_metatable(Some(mt))?;

    // Per-plugin logging: `print`, `trace`, `debug`, `info`, `warn`, `error`
    // all tag output with the plugin name.
    setup_plugin_env(lua, &env, &name)?;

    let contents = fs::read_to_string(&path).await?;
    let path_str = path.to_str().ok_or(LuaPluginLoadError::NonUtf8Path)?;

    lua.load(&contents)
        .set_name(&format!("@{path_str}"))
        .set_environment(env.clone())
        .exec()
        .map_err(|e| LuaPluginLoadError::LuaErrorInPlugin {
            e,
            name: name.clone(),
        })?;

    add_callbacks_from_env(&env, &name, callbacks)
        .map_err(|e| LuaPluginLoadError::LuaErrorInPlugin { e, name })?;

    Ok(())
}
