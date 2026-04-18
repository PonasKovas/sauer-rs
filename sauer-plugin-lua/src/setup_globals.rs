use mlua::{Function, Lua, Value, Variadic};
use tracing::{Level, event};

pub(crate) fn setup_globals(lua: &Lua, name: &str) -> Result<(), mlua::Error> {
    // print is replaced with info
    let name_clone = name.to_owned();
    let print = lua.create_function(move |lua, args: Variadic<Value>| {
        log(Level::INFO, &name_clone, lua, args)
    })?;

    let name_clone = name.to_owned();
    let trace = lua.create_function(move |lua, args: Variadic<Value>| {
        log(Level::TRACE, &name_clone, lua, args)
    })?;
    let name_clone = name.to_owned();
    let debug = lua.create_function(move |lua, args: Variadic<Value>| {
        log(Level::DEBUG, &name_clone, lua, args)
    })?;
    let name_clone = name.to_owned();
    let info = lua.create_function(move |lua, args: Variadic<Value>| {
        log(Level::INFO, &name_clone, lua, args)
    })?;
    let name_clone = name.to_owned();
    let warn = lua.create_function(move |lua, args: Variadic<Value>| {
        log(Level::WARN, &name_clone, lua, args)
    })?;
    let name_clone = name.to_owned();
    let error = lua.create_function(move |lua, args: Variadic<Value>| {
        log(Level::ERROR, &name_clone, lua, args)
    })?;

    lua.globals().set("print", print)?;
    lua.globals().set("trace", trace)?;
    lua.globals().set("debug", debug)?;
    lua.globals().set("info", info)?;
    lua.globals().set("warn", warn)?;
    lua.globals().set("error", error)?;

    Ok(())
}

fn log(level: Level, name: &str, lua: &Lua, args: Variadic<Value>) -> Result<(), mlua::Error> {
    // Grab Lua's native `tostring` function so we respect `__tostring` metamethods
    let tostring: Function = lua.globals().get("tostring")?;

    let mut printed_args = Vec::new();

    for arg in args {
        // Convert each argument to a string using Lua's tostring
        let str_val: String = tostring.call(arg)?;
        printed_args.push(str_val);
    }

    // Lua's native print separates arguments with a tab character
    let output = printed_args.join("\t");

    // Level 1 is the Lua code that called this Rust closure
    let (file, line) = lua
        .inspect_stack(1, |debug| {
            // short_src() returns &[u8], so we safely convert it to a String
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
        Level::INFO => event!( target: "lua", Level::INFO, file, line, "[{name}] {output}"),
        Level::WARN => event!(target: "lua", Level::WARN, file, line, "[{name}] {output}"),
        Level::ERROR => event!(target: "lua", Level::ERROR, file, line, "[{name}] {output}"),
    }

    Ok(())
}
