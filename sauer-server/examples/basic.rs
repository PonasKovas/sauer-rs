use cube_string::{Color, CubeString};
use dashmap::DashMap;
use rand::random;
use sauer_proto::{
    PROTOCOL_VERSION,
    channel1::{Connect, ServInfo},
};
use sauer_server::{
    BasePlugin, ServerBuilder,
    plugin::{BLOCK, Context, Control, PASS, Plugin},
};
use std::{sync::Arc, time::Duration};
use tokio::{task::JoinHandle, time::sleep};

struct ChangingNamePlugin {
    tasks: DashMap<u32, JoinHandle<()>>,
}

impl ChangingNamePlugin {
    fn new() -> Self {
        Self {
            tasks: DashMap::new(),
        }
    }
}

impl Plugin for ChangingNamePlugin {
    async fn connect(&self, ctx: Context<'_>, _: &Connect) -> Control {
        if self.tasks.contains_key(&ctx.cn) {
            return BLOCK;
        }

        let cn = ctx.cn;
        let server = Arc::clone(&ctx.server);
        self.tasks.insert(
            ctx.cn,
            tokio::spawn(async move {
                let mut i = 0;
                loop {
                    const TOTAL_WIDTH: usize = 20;
                    const TEXT_WIDTH: usize = 8;
                    let pos = triangle(i, TOTAL_WIDTH - TEXT_WIDTH - 1);

                    let server_desc = CubeString::builder()
                        .push_str(&" ".repeat(pos))
                        .color(random_color())
                        .push_str("sauer")
                        .color(random_color())
                        .push_str("-")
                        .color(random_color())
                        .push_str("rs")
                        .push_str(&" ".repeat(TOTAL_WIDTH - TEXT_WIDTH - pos))
                        .build();

                    server
                        .get_client(cn)
                        .send_chan1(&ServInfo {
                            client_num: cn as i32,
                            protocol: PROTOCOL_VERSION,
                            session_id: 0,
                            has_password: false,
                            server_desc,
                            server_auth: CubeString::new(),
                        })
                        .await;

                    sleep(Duration::from_millis(100)).await;
                    i += 1;
                }
            }),
        );

        PASS
    }

    async fn disconnected(&self, ctx: Context<'_>) -> Control {
        self.tasks.remove(&ctx.cn).map(|handle| handle.1.abort());

        PASS
    }
}

#[tokio::main]
async fn main() {
    sauer_server::utils::init_default_tracing();

    let server = ServerBuilder::new()
        .bind_addr(([0, 0, 0, 0], 11111))
        .max_peers(1024)
        .plugin(BasePlugin::new())
        .plugin(ChangingNamePlugin::new());

    if let Err(e) = server.run().await {
        println!("Error setting up the server: {e}");
    }
}

fn triangle(i: usize, n: usize) -> usize {
    let period = 2 * n;
    let x = i % period;
    if x <= n { x } else { period - x }
}
fn random_color() -> Color {
    match random::<u8>() % 8 {
        0 => Color::Green,
        1 => Color::Blue,
        2 => Color::Yellow,
        3 => Color::Red,
        4 => Color::Magenta,
        5 => Color::Orange,
        6 => Color::White,
        _ => Color::Cyan,
    }
}
