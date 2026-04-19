use crate::{
    options::{BasePluginOptions, get_options},
    players::{
        Players, clear_player, get_players, init_player,
        model::{init_model, set_model},
        name::{init_name, set_name},
        position::set_position,
        team::set_team,
    },
};
use sauer_proto::{
    Client, channel0,
    channel1::{self, ServInfo},
    data::SetTeamReason,
};
use sauer_server::{
    Server,
    plugin::{Context, Plugin},
};
use tracing::info;

use std::sync::RwLock;

mod error;
pub mod options;
pub mod players;
mod util;
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
        server.extra.insert(RwLock::new(self.initial_options.clone()));
        server.extra.insert(Players::new());
    }

    async fn connected(&self, ctx: &mut Context) {
        let session_id = init_player(ctx.server(), ctx.cn());

        let options = get_options(ctx.server()).read().unwrap();
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
        clear_player(ctx.server(), ctx.cn());
    }

    async fn connect(&self, ctx: &mut Context, packet: &channel1::Connect) {
        info!("{} (cn {}) connected.", packet.name, ctx.cn());

        init_name(ctx.server(), ctx.cn(), packet.name.clone());
        init_model(ctx.server(), ctx.cn(), packet.player_model);

        let options = get_options(ctx.server()).read().unwrap();
        if options.instant_welcome {
            drop(options);
            welcome(ctx.server(), ctx.cn());
        }
    }

    async fn pos(&self, ctx: &mut Context, packet: &channel0::Position) {
        set_position(ctx.server(), ctx.cn(), packet.pos);
        ctx.server().broadcast_except_chan0(ctx.cn(), packet);
    }
    async fn teleport(&self, ctx: &mut Context, packet: &channel0::Teleport) {
        ctx.server().broadcast_chan0(packet);
    }
    async fn jumppad(&self, ctx: &mut Context, packet: &channel0::JumpPad) {
        ctx.server().broadcast_chan0(packet);
    }

    async fn text(&self, ctx: &mut Context, packet: &channel1::Text) {
        let mut packet = packet.clone();

        packet.text = packet.text.filter(cube_string::FilterOptions {
            strip_colors: true,
            preserve_whitespace: true,
            collapse_spaces: true,
            max_len: Some(5000),
        });

        let name = get_players(ctx.server()).get(ctx.cn()).name.clone();
        info!("[CHAT] {name}: {}", packet.text);

        ctx.server().broadcast_chan1(&Client {
            client_num: ctx.cn() as i32,
            packet,
        });
    }
    async fn switch_name(&self, ctx: &mut Context, packet: &channel1::SwitchName) {
        set_name(ctx.server(), ctx.cn(), packet.name.clone());
    }
    async fn switch_model(&self, ctx: &mut Context, packet: &channel1::SwitchModel) {
        set_model(ctx.server(), ctx.cn(), packet.model);
    }
    async fn switch_team(&self, ctx: &mut Context, packet: &channel1::SwitchTeam) {
        set_team(
            ctx.server(),
            ctx.cn(),
            packet.team.clone(),
            SetTeamReason::Switched,
        );
    }

    async fn ping(&self, ctx: &mut Context, packet: &channel1::Ping) {
        ctx.server()
            .send_chan1(ctx.cn(), &channel1::Pong { ping: packet.time });
    }
    async fn client_ping(&self, ctx: &mut Context, packet: &channel1::ClientPing) {
        ctx.server().broadcast_except_chan1(
            ctx.cn(),
            &Client {
                client_num: ctx.cn() as i32,
                packet: packet.clone(),
            },
        );
    }
}
