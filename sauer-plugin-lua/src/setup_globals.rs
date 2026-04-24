use mlua::{Function, Lua, Table, Value, Variadic};
use tracing::{Level, event};

/// Set up per-plugin logging functions (`print`, `trace`, `debug`, `info`,
/// `warn`, `error`) in the plugin's sandboxed environment table so that log
/// output is tagged with the plugin name.
pub(crate) fn setup_plugin_env(lua: &Lua, env: &Table, name: &str) -> Result<(), mlua::Error> {
    let make_log = |level: Level| -> mlua::Result<mlua::Function> {
        let n = name.to_owned();
        lua.create_function(move |lua, args: Variadic<Value>| log(level, &n, lua, args))
    };

    env.set("print", make_log(Level::INFO)?)?;
    env.set("trace", make_log(Level::TRACE)?)?;
    env.set("debug", make_log(Level::DEBUG)?)?;
    env.set("info", make_log(Level::INFO)?)?;
    env.set("warn", make_log(Level::WARN)?)?;
    env.set("error", make_log(Level::ERROR)?)?;

    env.set("PROTOCOL_VERSION", sauer_proto::PROTOCOL_VERSION)?;

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
