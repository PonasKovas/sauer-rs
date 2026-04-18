use sauer_plugin_lua::LuaPlugin;
use sauer_plugin_lua::LuaPluginOptions;
use sauer_server::BasePlugin;
use sauer_server::ServerBuilder;
use tracing::error;

#[tokio::main]
async fn main() {
    sauer_server::utils::init_default_tracing();

    let lua_plugin = match LuaPlugin::load(&LuaPluginOptions::default()).await {
        Ok(x) => x,
        Err(e) => {
            error!("Error loading lua plugins: {e}");
            return;
        }
    };

    let server = ServerBuilder::new()
        .bind_addr(([0, 0, 0, 0], 11111))
        .max_peers(1024)
        .plugin(BasePlugin::new())  // added first = runs second
        .plugin(lua_plugin);        // added last = runs first (tuple composition: self.1 before self.0)

    if let Err(e) = server.run().await {
        println!("Error setting up the server: {e}");
    }
}
