use mlua::{Function, IntoLua, Lua, Table, Value, Variadic};
use tracing::{Level, event};

/// Set up per-plugin logging functions (`print`, `trace`, `debug`, `info`,
/// `warn`, `error`) in the plugin's sandboxed environment table so that log
/// output is tagged with the plugin name.
pub(crate) fn setup_plugin_env(lua: &Lua, env: &Table, name: &str) -> Result<(), mlua::Error> {
    let make_log = |level: Level| -> mlua::Result<mlua::Function> {
        let n = name.to_owned();
        lua.create_function(move |lua, args: Variadic<Value>| log(level, &n, lua, args))
    };

    /// Log a message at INFO level, tagged with the plugin name. Accepts multiple args separated by tabs.
    env.set("print", make_log(Level::INFO)?)?;
    /// Log at TRACE level, tagged with the plugin name.
    env.set("trace", make_log(Level::TRACE)?)?;
    /// Log at DEBUG level, tagged with the plugin name.
    env.set("debug", make_log(Level::DEBUG)?)?;
    /// Log at INFO level, tagged with the plugin name.
    env.set("info", make_log(Level::INFO)?)?;
    /// Log at WARN level, tagged with the plugin name.
    env.set("warn", make_log(Level::WARN)?)?;
    /// Log at ERROR level, tagged with the plugin name.
    env.set("error", make_log(Level::ERROR)?)?;

    /// Protocol version integer this server implements.
    env.set("PROTOCOL_VERSION", sauer_proto::PROTOCOL_VERSION)?;

    let ui_table = lua.create_table()?;
    /// Format text as a bold header for server messages.
    ui_table.set(
        "header",
        lua.create_function(|lua, text: String| sauer_server::ui::header(&text).into_lua(lua))?,
    )?;
    /// Format text as body text for server messages.
    ui_table.set(
        "body",
        lua.create_function(|lua, text: String| sauer_server::ui::body(&text).into_lua(lua))?,
    )?;
    /// Format a command tip with a command name and description.
    ui_table.set(
        "tip",
        lua.create_function(|lua, (cmd, desc): (String, String)| {
            sauer_server::ui::tip(&cmd, &desc).into_lua(lua)
        })?,
    )?;
    /// Format text as a warning message.
    ui_table.set(
        "warn_msg",
        lua.create_function(|lua, text: String| sauer_server::ui::warn_msg(&text).into_lua(lua))?,
    )?;
    /// Format text as a section header.
    ui_table.set(
        "section",
        lua.create_function(|lua, text: String| sauer_server::ui::section(&text).into_lua(lua))?,
    )?;
    /// Return a horizontal separator string.
    ui_table.set(
        "sep",
        lua.create_function(|lua, ()| sauer_server::ui::sep().into_lua(lua))?,
    )?;
    env.set("ui", ui_table)?;

    Ok(())
}

fn log(level: Level, name: &str, lua: &Lua, args: Variadic<Value>) -> Result<(), mlua::Error> {
    let tostring: Function = lua.globals().get("tostring")?;

    let mut parts = Vec::new();
    for arg in args {
        parts.push(tostring.call::<String>(arg)?);
    }
    let output = parts.join("\t");

    let (file, line) = lua
        .inspect_stack(1, |debug| {
            let file = debug.source().short_src.map(|s| s.into_owned());
            let line = debug.current_line();
            (file, line)
        })
        .unwrap_or((None, None));

    let file = file.unwrap_or_else(|| "unknown".to_string());
    let line = line
        .as_ref()
        .map(usize::to_string)
        .unwrap_or_else(|| "unknown".to_string());

    match level {
        Level::TRACE => event!(target: "lua", Level::TRACE, file, line, "[{name}] {output}"),
        Level::DEBUG => event!(target: "lua", Level::DEBUG, file, line, "[{name}] {output}"),
        Level::INFO => event!(target: "lua", Level::INFO,  file, line, "[{name}] {output}"),
        Level::WARN => event!(target: "lua", Level::WARN,  file, line, "[{name}] {output}"),
        Level::ERROR => event!(target: "lua", Level::ERROR, file, line, "[{name}] {output}"),
    }

    Ok(())
}
