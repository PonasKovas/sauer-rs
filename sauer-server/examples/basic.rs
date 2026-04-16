use sauer_server::{BasePlugin, ServerBuilder};

#[tokio::main]
async fn main() {
    sauer_server::utils::init_default_tracing();

    let server = ServerBuilder::new()
        .max_peers(1024)
        .plugin(BasePlugin::new());

    if let Err(e) = server.run().await {
        println!("Error setting up the server: {e}");
    }
}
