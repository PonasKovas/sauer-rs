mod client;
mod connect;
mod crypto;
mod game;
mod identity;
mod master;
mod ping;
mod registry;

pub mod error;
pub mod options;

pub use client::ClientData;
pub use error::BasePluginError;
pub use game::GameState;
pub use master::MasterState;
pub use options::BasePluginOptions;
pub use registry::ClientRegistry;

use options::BasePluginOptions as Opts;
use sauer_proto::channel1;
use sauer_server::{
    Server,
    plugin::{Context, Control, Plugin, PASS},
};
use std::sync::Arc;

pub struct BasePlugin {
    options: Arc<Opts>,
}

impl BasePlugin {
    pub fn new() -> Self {
        Self {
            options: Arc::new(Opts::default()),
        }
    }

    pub fn with_options(options: Opts) -> Self {
        Self { options: Arc::new(options) }
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
        server.extra.insert(MasterState::new());
        server.extra.insert(GameState::new(
            self.options.initial_map.clone(),
            self.options.initial_mode,
        ));
    }

    async fn start(&self, server: &Arc<Server>) {
        let server = Arc::clone(server);
        let options = Arc::clone(&self.options);
        tokio::spawn(game::run_tick_loop(server, options));
    }

    async fn connected(&self, ctx: Context<'_>) -> Control {
        connect::on_connected(ctx.server, ctx.cn, &self.options).await
    }

    async fn disconnected(&self, ctx: Context<'_>) -> Control {
        connect::on_disconnect(ctx.server, ctx.cn).await;
        PASS
    }

    async fn connect(&self, ctx: Context<'_>, packet: &channel1::Connect) -> Control {
        connect::on_connect(ctx.server, ctx.cn, packet, &self.options).await
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

    async fn set_master(&self, ctx: Context<'_>, packet: &channel1::SetMaster) -> Control {
        master::on_set_master(ctx.server, ctx.cn, packet, &self.options).await
    }

    async fn master_mode(&self, ctx: Context<'_>, packet: &channel1::MasterModeClient) -> Control {
        master::on_master_mode(ctx.server, ctx.cn, packet).await
    }

    async fn kick(&self, ctx: Context<'_>, packet: &channel1::Kick) -> Control {
        master::on_kick(ctx.server, ctx.cn, packet).await
    }

    async fn clear_bans(&self, ctx: Context<'_>, packet: &channel1::ClearBans) -> Control {
        master::on_clear_bans(ctx.server, ctx.cn, packet).await
    }

    async fn auth_try(&self, ctx: Context<'_>, packet: &channel1::AuthTry) -> Control {
        master::on_auth_try(ctx.server, ctx.cn, packet, &self.options).await
    }

    async fn auth_answer(&self, ctx: Context<'_>, packet: &channel1::AuthAnswer) -> Control {
        master::on_auth_ans(ctx.server, ctx.cn, packet).await
    }

    async fn map_vote(&self, ctx: Context<'_>, packet: &channel1::MapVote) -> Control {
        game::on_map_vote(ctx.server, ctx.cn, packet, &self.options).await
    }

    async fn new_map(&self, ctx: Context<'_>, packet: &channel1::NewMap) -> Control {
        game::on_new_map(ctx.server, ctx.cn, packet).await
    }

    async fn item_list(&self, ctx: Context<'_>, packet: &channel1::ItemList) -> Control {
        game::on_item_list(ctx.server, ctx.cn, packet).await
    }

    async fn force_intermission(
        &self,
        ctx: Context<'_>,
        packet: &channel1::ForceIntermission,
    ) -> Control {
        game::on_force_intermission(ctx.server, ctx.cn, packet).await
    }

    async fn pause_game(&self, ctx: Context<'_>, packet: &channel1::PauseGameClient) -> Control {
        game::on_pause_game(ctx.server, ctx.cn, packet).await
    }

    async fn game_speed(&self, ctx: Context<'_>, packet: &channel1::GameSpeedClient) -> Control {
        game::on_game_speed(ctx.server, ctx.cn, packet).await
    }
}
