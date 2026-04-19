use crate::{
    names::{Names, clear_name, set_name},
    options::{BasePluginOptions, get_options},
    playermodels::{PlayerModels, set_playermodel},
    session_ids::{SessionIds, clear_session_id, create_session_id},
    teams::{Teams, clear_team, set_initial_team},
};
use sauer_proto::{
    channel0,
    channel1::{self, ServInfo},
};
use sauer_server::{
    Server,
    plugin::{Context, Plugin},
};
use tracing::info;

use std::sync::RwLock;

mod error;
pub mod names;
pub mod options;
pub mod playermodels;
pub mod session_ids;
pub mod teams;
mod welcome;

pub use error::BasePluginError;
pub use welcome::welcome;

pub struct BasePlugin {
    initial_options: BasePluginOptions,
}

impl BasePlugin {
    pub fn new() -> Self {
        Self {
            initial_options: BasePluginOptions::default(),
        }
    }

    pub fn with_options(options: BasePluginOptions) -> Self {
        Self {
            initial_options: options,
        }
    }
}

impl Default for BasePlugin {
    fn default() -> Self {
        Self::new()
    }
}

impl Plugin for BasePlugin {
    async fn init(&self, server: &mut Server) {
        let options = RwLock::new(self.initial_options.clone());
        server.extra.insert(options);
        server.extra.insert(SessionIds::new());
        server.extra.insert(Names::new());
        server.extra.insert(PlayerModels::new());
        server.extra.insert(Teams::new());
    }

    async fn connected(&self, ctx: &mut Context) {
        let options = get_options(ctx.server()).read().unwrap();

        let session_id = create_session_id(ctx.server(), ctx.cn());

        ctx.server().send_chan1(
            ctx.cn(),
            &ServInfo {
                client_num: ctx.cn() as i32,
                protocol: sauer_proto::PROTOCOL_VERSION,
                session_id,
                has_password: options.has_password,
                server_desc: options.server_desc.clone(),
                server_auth: options.server_auth.clone(),
            },
        );
    }

    async fn disconnected(&self, ctx: &mut Context) {
        clear_session_id(ctx.server(), ctx.cn());
        clear_name(ctx.server(), ctx.cn());
        clear_team(ctx.server(), ctx.cn());
    }

    async fn connect(&self, ctx: &mut Context, packet: &channel1::Connect) {
        info!("{} (cn {}) connected.", packet.name, ctx.cn());

        set_name(ctx.server(), ctx.cn(), packet.name.clone());
        set_playermodel(ctx.server(), ctx.cn(), packet.player_model);
        set_initial_team(ctx.server(), ctx.cn());

        let options = get_options(ctx.server()).read().unwrap();
        if options.instant_welcome {
            welcome(ctx.server(), ctx.cn());
        }
    }
}
