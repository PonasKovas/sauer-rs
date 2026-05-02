use cube_string::IntoCubeStringLossy;
use sauer_proto::channel1::ServMsg;
use sauer_server::{Context, Plugin, Server};
use std::sync::Arc;
use teloxide::prelude::*;

pub struct TelegramBridge {
    enabled: bool,
    bot: Bot,
    chat_id: ChatId,
}

impl TelegramBridge {
    /// Create a new Telegram Bridge plugin with specific credentials.
    pub fn new(enabled: bool, token: impl Into<String>, chat_id: i64) -> Self {
        Self {
            enabled,
            bot: Bot::new(token),
            chat_id: ChatId(chat_id),
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
    async fn start(&self, server: &Arc<Server>) {
        if !self.enabled {
            return;
        }

        let bot = self.bot.clone();
        let target_chat_id = self.chat_id;
        let server_clone = Arc::clone(server);

        tokio::spawn(async move {
            tracing::info!("Starting Telegram bridge polling...");

            // Set up a teloxide handler that listens to text messages and forwards them to Sauerbraten
            let handler = Update::filter_message().endpoint(
                move |msg: Message, _bot: Bot, server: Arc<Server>| {
                    async move {
                        // Only listen to the target group/channel
                        if msg.chat.id == target_chat_id {
                            if let Some(text) = msg.text() {
                                // Find the sender's username, fallback to first name, then "Unknown"
                                let sender = msg
                                    .from()
                                    .and_then(|u| u.username.clone())
                                    .or_else(|| msg.from().map(|u| u.first_name.clone()))
                                    .unwrap_or_else(|| "Unknown".to_string());

                                // Construct output string and convert to CubeString character encoding format
                                let out = format!("[TG] {}: {}", sender, text).lossy_cubestring();

                                // Broadcast using ServMsg
                                server.broadcast_chan1(&ServMsg { text: out });
                            }
                        }
                        Ok::<(), std::convert::Infallible>(())
                    }
                },
            );

            // Start the bot's dispatcher
            Dispatcher::builder(bot, handler)
                .dependencies(teloxide::dptree::deps![server_clone])
                .build()
                .dispatch()
                .await;
        });
    }

    async fn on_chat(&self, ctx: &mut Context, text: &mut cube_string::CubeString) {
        if !self.enabled {
            return;
        }

        // Retrieve the UTF-8 representations of the CubeStrings
        let sender_name = ctx.server().players.get(ctx.id()).name.to_utf8();
        let msg = text.to_utf8();
        let text_to_send = format!("{}: {}", sender_name, msg);

        let bot = self.bot.clone();
        let chat_id = self.chat_id;

        // Spawn as a background task to ensure we don't hold up the server's network loop
        tokio::spawn(async move {
            if let Err(e) = bot.send_message(chat_id, text_to_send).await {
                tracing::error!("Failed to send chat message to Telegram: {}", e);
            }
        });
    }
}
