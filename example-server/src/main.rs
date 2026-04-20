use sauer_base_plugin::BasePlugin;
use sauer_base_plugin::options::BasePluginOptions;
use sauer_plugin_lua::LuaPlugin;
use sauer_plugin_lua::LuaPluginOptions;
use sauer_proto::data::GameMode;
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

    let mut base_options = BasePluginOptions::new();
    base_options.mode = GameMode::CoopEdit;

    let server = ServerBuilder::new()
        .bind_addr(([0, 0, 0, 0], 11111))
        .max_peers(1024)
        .plugin(BasePlugin::with_options(base_options))
        .plugin(lua_plugin);

    if let Err(e) = server.run().await {
        println!("Error setting up the server: {e}");
    }
}
