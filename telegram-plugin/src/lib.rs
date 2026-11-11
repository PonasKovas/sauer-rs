use cube_string::IntoCubeStringLossy;
use sauer_server::{Context, Flow, Plugin, Server};
use std::sync::Arc;
use teloxide::prelude::*;

pub struct TelegramBridge {
    enabled: bool,
    bot: Bot,
    chat_id: ChatId,
    bot_cn: sauer_proto::data::Cn,
}

impl TelegramBridge {
    /// Create a new Telegram Bridge plugin with specific credentials.
    pub fn new(enabled: bool, token: impl Into<String>, chat_id: i64) -> Self {
        Self {
            enabled,
            bot: Bot::new(token),
            chat_id: ChatId(chat_id),
            bot_cn: sauer_proto::data::Cn(255),
        }
    }

    /// Automatically load the bot token and chat ID from environment variables.
    /// Requires `TELEGRAM_BOT_TOKEN` and `TELEGRAM_CHAT_ID` to be set.
    pub fn from_env() -> Self {
        let token = std::env::var("TELEGRAM_BOT_TOKEN")
            .expect("TELEGRAM_BOT_TOKEN environment variable must be set");
        let chat_id = std::env::var("TELEGRAM_CHAT_ID")
            .expect("TELEGRAM_CHAT_ID environment variable must be set")
            .parse::<i64>()
            .expect("TELEGRAM_CHAT_ID must be a valid integer");

        Self::new(true, token, chat_id)
    }
}

impl Plugin for TelegramBridge {
    async fn init(&self, server: &mut Server) {
        if self.enabled {
            assert!(
                server.reserve_cn(self.bot_cn),
                "the telegram bot cn is already taken"
            );
        }
    }

    async fn on_client_welcomed(&self, ctx: &mut Context) {
        if !self.enabled {
            return;
        }

        use sauer_proto::channel1::{InitClient, SpectatorServer};
        use sauer_proto::data::PlayerModel;

        ctx.server().send(
            ctx.id(),
            &InitClient {
                client_num: self.bot_cn,
                name: "TG".lossy_cubestring(),
                player_model: PlayerModel::MrFixit,
                team: "".lossy_cubestring(),
            },
        );
        ctx.server().send(
            ctx.id(),
            &SpectatorServer {
                client_num: self.bot_cn,
                is_spectator: true,
            },
        );
    }

    async fn start(&self, server: &Arc<Server>) {
        if !self.enabled {
            return;
        }

        let bot = self.bot.clone();
        let target_chat_id = self.chat_id;
        let server_clone = Arc::clone(server);
        let bot_cn = self.bot_cn;

        tokio::spawn(async move {
            tracing::info!("Starting Telegram bridge polling...");

            let handler = Update::filter_message().endpoint(
                move |msg: Message, _bot: Bot, server: Arc<Server>| {
                    let bot_cn = bot_cn;
                    async move {
                        if msg.chat.id == target_chat_id {
                            if let Some(text) = msg.text() {
                                let sender = msg
                                    .from
                                    .as_ref()
                                    .and_then(|u| u.username.clone())
                                    .or_else(|| msg.from.as_ref().map(|u| u.first_name.clone()))
                                    .unwrap_or_else(|| "Unknown".to_string());

                                server.broadcast(&sauer_proto::Client {
                                    client_num: bot_cn,
                                    packet: sauer_proto::channel1::Text {
                                        text: format!("{}: {}", sender, text).lossy_cubestring(),
                                    },
                                });
                            }
                        }
                        Ok::<(), std::convert::Infallible>(())
                    }
                },
            );

            Dispatcher::builder(bot, handler)
                .dependencies(teloxide::dptree::deps![server_clone])
                .build()
                .dispatch()
                .await;
        });
    }

    async fn on_chat(&self, ctx: &mut Context, text: &mut cube_string::CubeString) -> Flow {
        if !self.enabled {
            return Flow::Continue;
        }

        // Do not forward messages from the TG bot itself
        if ctx.server().get_cn(ctx.id()) == Some(self.bot_cn) {
            return Flow::Continue;
        }

        let sender_name = ctx.server().player_name(ctx.id()).to_utf8();
        let msg = text.to_utf8();
        let text_to_send = format!("{}: {}", sender_name, msg);

        let bot = self.bot.clone();
        let chat_id = self.chat_id;

        tokio::spawn(async move {
            if let Err(e) = bot.send_message(chat_id, text_to_send).await {
                tracing::error!("Failed to send chat message to Telegram: {}", e);
            }
        });
        Flow::Continue
    }
}
