use mlua::Table;

pub(crate) struct LuaCallback {
    pub plugin_name: String,
    pub callback: mlua::Function,
}

/// Declares `Callbacks` with one list per hook, and the function that fills
/// them from a plugin's globals. Packet hooks come from the server's list.
macro_rules! callbacks {
    (
        ($($hook:ident),*)
        chan0 { $($c0:tt)* }
        chan1 { $($c1:tt)* }
    ) => {
        callbacks! { @each ($($hook),*) $($c0)* $($c1)* }
    };
    (@each ($($hook:ident),*) $( [$msg_id:ident] $method:ident ($packet:path) $variant:ident, )*) => {
        #[derive(Default)]
        pub(crate) struct Callbacks {
            $( pub $hook: Vec<LuaCallback>, )*
            $( pub $method: Vec<LuaCallback>, )*
        }

        /// Adds every function in `env` named after a hook.
        pub(crate) fn add_callbacks_from_env(
            env: &Table,
            plugin_name: &str,
            callbacks: &mut Callbacks,
        ) -> Result<(), mlua::Error> {
            $( add_callback(env, plugin_name, stringify!($hook), &mut callbacks.$hook)?; )*
            $( add_callback(env, plugin_name, stringify!($method), &mut callbacks.$method)?; )*
            Ok(())
        }
    };
}

sauer_server::for_each_packet_hook!(
    callbacks,
    start,
    connected,
    disconnected,
    on_client_welcomed,
    map_changed,
    on_spawn,
    on_chat,
    on_command,
    map,
    server_query
);

fn add_callback(
    env: &Table,
    plugin_name: &str,
    name: &str,
    list: &mut Vec<LuaCallback>,
) -> Result<(), mlua::Error> {
    if env.contains_key(name)? {
        list.push(LuaCallback {
            plugin_name: plugin_name.to_owned(),
            callback: env.get(name)?,
        });
    }
    Ok(())
}
