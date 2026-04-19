use crate::{game::GameState, options::BasePluginOptions, registry::ClientRegistry};
use sauer_proto::channel1;
use sauer_server::Server;
use std::sync::Arc;
use tokio::time::{Duration, Instant, interval};

const OVERTIME_MS: u64 = 2 * 60 * 1000;
const INTERMISSION_MS: u64 = 10_000;
const TIMEUP_BROADCAST_MS: u64 = 1_000;
const TICK_MS: u64 = 100;

pub(crate) async fn run_tick_loop(server: Arc<Server>, options: Arc<BasePluginOptions>) {
    let mut ticker = interval(Duration::from_millis(TICK_MS));
    let mut last_tick = Instant::now();
    let mut last_timeup = Instant::now();

    loop {
        ticker.tick().await;

        let Some(game) = server.extra.get::<GameState>() else { return };
        let Some(registry) = server.extra.get::<ClientRegistry>() else { return };

        let now = Instant::now();
        let elapsed_ms = now.duration_since(last_tick).as_millis() as u64;
        last_tick = now;

        // ── Intermission countdown ────────────────────────────────────────────
        if game.is_intermission() {
            let interm_elapsed = game
                .interm_start
                .lock()
                .unwrap()
                .map(|t| t.elapsed().as_millis() as u64)
                .unwrap_or(0);

            if interm_elapsed >= INTERMISSION_MS {
                let (map_name, mode) = game
                    .best_voted()
                    .unwrap_or_else(|| game.next_rotation(&options));
                game.change_map(map_name, mode, &server, &registry, &options).await;
            }
            continue;
        }

        // ── Advance gamemillis ────────────────────────────────────────────────
        let millis = game.advance_time(elapsed_ms);

        // ── Time limit check ──────────────────────────────────────────────────
        if millis >= game.gamelimit {
            // TODO: check for tied score → extend by OVERTIME_MS instead of entering intermission.
            let _ = OVERTIME_MS;
            game.start_intermission(&server);
            continue;
        }

        // ── Periodic N_TIMEUP broadcast ───────────────────────────────────────
        if now.duration_since(last_timeup) >= Duration::from_millis(TIMEUP_BROADCAST_MS) {
            last_timeup = now;
            server.broadcast_chan1(&channel1::TimeUp {
                seconds_remaining: game.remaining_seconds(),
            });
        }
    }
}
