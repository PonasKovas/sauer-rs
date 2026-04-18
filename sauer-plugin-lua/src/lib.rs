use crate::{error::LuaPluginErrorExt, setup_globals::setup_globals};
use mlua::Lua;
use sauer_server::Server;
use tokio::fs::{self, DirEntry};
use tracing::error;

mod error;
mod options;
mod setup_globals;

pub use error::LuaPluginLoadError;
pub use options::LuaPluginOptions;

#[derive(Default)]
pub struct LuaPlugin {
    lua_instances: Vec<Lua>,
    init: Vec<LuaCallback>,
}

struct LuaCallback {
    plugin_name: String,
    callback: mlua::Function,
}

impl LuaPlugin {
    pub async fn load(options: &LuaPluginOptions) -> Result<Self, LuaPluginLoadError> {
        let mut luaplugin = Self::default();

        let mut entries = fs::read_dir(&options.plugin_directory)
            .await
            .in_path(&options.plugin_directory)?;

        while let Some(entry) = entries
            .next_entry()
            .await
            .in_path(&options.plugin_directory)?
        {
            let (plugin_name, lua) = process_fs_entry(options, &entry)
                .await
                .in_path(&entry.path())?;

            add_callbacks(options, &mut luaplugin, &plugin_name, lua).map_err(|e| {
                LuaPluginLoadError::LuaErrorInPlugin {
                    e,
                    name: plugin_name,
                }
            })?;
        }

        Ok(luaplugin)
    }
}

async fn process_fs_entry(
    _options: &LuaPluginOptions,
    entry: &DirEntry,
) -> Result<(String, Lua), LuaPluginLoadError> {
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
    let is_directory = file_type.is_dir();

    let lua = Lua::new();

    if is_directory {
        // Allow the lua script to use `require(xyz)` to load lua scripts in its own directory
        let package: mlua::Table = lua.globals().get("package")?;
        let current_path: String = package.get("path")?;
        package.set("path", format!("{entry_path_str}/?.lua;{current_path}"))?;

        path.push("main.lua");
    }

    setup_globals(&lua, &name)?;

    let contents = fs::read_to_string(&path).await?;
    let path_str = path.to_str().ok_or(LuaPluginLoadError::NonUtf8Path)?;

    lua.load(&contents)
        .set_name(&format!("@{path_str}"))
        .exec()?;

    Ok((name, lua))
}

fn add_callbacks(
    _options: &LuaPluginOptions,
    luaplugin: &mut LuaPlugin,
    name: &str,
    lua: Lua,
) -> Result<(), mlua::Error> {
    let globals = lua.globals();

    if globals.contains_key("init")? {
        luaplugin.init.push(LuaCallback {
            plugin_name: name.to_owned(),
            callback: globals.get("init")?,
        });
    }

    luaplugin.lua_instances.push(lua);

    Ok(())
}

impl sauer_server::Plugin for LuaPlugin {
    async fn init(&self, _server: &mut Server) {
        for callback in &self.init {
            if let Err(e) = callback.callback.call::<()>(()) {
                error!(target: "lua", "[{}] {e}", callback.plugin_name);
            }
        }
    }
}
