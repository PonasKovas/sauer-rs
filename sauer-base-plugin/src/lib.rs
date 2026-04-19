mod client;
mod connect;
mod identity;
mod ping;
mod registry;

pub mod error;
pub mod options;

pub use client::ClientData;
pub use error::BasePluginError;
pub use options::BasePluginOptions;
pub use registry::ClientRegistry;

use sauer_proto::channel1;
use sauer_server::{
    Server,
    plugin::{Context, Control, PASS, Plugin},
};

pub struct BasePlugin {
    options: BasePluginOptions,
}

impl BasePlugin {
    pub fn new() -> Self {
        Self {
            options: BasePluginOptions::default(),
        }
    }

    pub fn with_options(options: BasePluginOptions) -> Self {
        Self { options }
    }
}

impl Default for BasePlugin {
    fn default() -> Self {
        Self::new()
    }
}

impl Plugin for BasePlugin {
    async fn init(&self, server: &mut Server) {
        server.extra.insert(ClientRegistry::new());
    }

    async fn connected(&self, ctx: Context<'_>) -> Control {
        connect::on_connected(ctx.server, ctx.cn, &self.options).await
    }

    async fn disconnected(&self, ctx: Context<'_>) -> Control {
        connect::on_disconnect(ctx.server, ctx.cn).await;
        PASS
    }

    async fn connect(&self, ctx: Context<'_>, packet: &channel1::Connect) -> Control {
        connect::on_connect(ctx.server, ctx.cn, packet).await
    }

    async fn ping(&self, ctx: Context<'_>, packet: &channel1::Ping) -> Control {
        ping::on_ping(ctx.server, ctx.cn, packet).await
    }

    async fn client_ping(&self, ctx: Context<'_>, packet: &channel1::ClientPing) -> Control {
        ping::on_client_ping(ctx.server, ctx.cn, packet).await
    }

    async fn switch_name(&self, ctx: Context<'_>, packet: &channel1::SwitchName) -> Control {
        identity::on_switch_name(ctx.server, ctx.cn, packet).await
    }

    async fn switch_model(&self, ctx: Context<'_>, packet: &channel1::SwitchModel) -> Control {
        identity::on_switch_model(ctx.server, ctx.cn, packet).await
    }

    async fn switch_team(&self, ctx: Context<'_>, packet: &channel1::SwitchTeam) -> Control {
        identity::on_switch_team(ctx.server, ctx.cn, packet).await
    }

    async fn spectator(&self, ctx: Context<'_>, packet: &channel1::SpectatorClient) -> Control {
        identity::on_spectator(ctx.server, ctx.cn, packet).await
    }
}
