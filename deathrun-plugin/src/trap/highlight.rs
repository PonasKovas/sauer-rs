//! The outline drawn around the trap a player looks at (or edits, or moves).

use super::TrapId;
use crate::map_state::get_server_state;
use dashmap::DashMap;
use sauer_kit::virt_ents::FakeEnt;
use sauer_server::Server;
use std::time::{Duration, Instant};

/// Fake-entity namespace of the outline.
const NAMESPACE: &str = "highlight";
/// The outline follows the player's view at most this often.
const REDRAW_EVERY: Duration = Duration::from_millis(100);

#[derive(Clone, Copy, Default)]
struct Highlight {
    trap: Option<TrapId>,
    last_drawn: Option<Instant>,
    shown: bool,
}

/// Each player's trap outline.
#[derive(Default)]
pub struct Highlights(DashMap<u64, Highlight>);

/// What [`Highlights::update`] did.
pub struct Update {
    /// The player turned to another trap, or away from one.
    pub target_changed: bool,
    /// The outline was drawn again (it is drawn every [`REDRAW_EVERY`]).
    pub redrawn: bool,
}

impl Highlights {
    /// Outlines `trap` for `player` with `outline`, or nothing for `None`.
    /// The outline is built by the caller because it follows the view.
    pub fn update(
        &self,
        server: &Server,
        player: u64,
        trap: Option<TrapId>,
        outline: Option<Vec<FakeEnt>>,
    ) -> Update {
        let now = Instant::now();
        // Decided under the map entry, sent after it is released.
        let (target_changed, redraw, shown) = {
            let mut h = self.0.entry(player).or_default();
            let target_changed = h.trap != trap;
            let was_shown = h.shown;
            if target_changed {
                h.trap = trap;
            }
            let redraw = trap.is_some()
                && h.last_drawn
                    .is_none_or(|t| target_changed || now.duration_since(t) >= REDRAW_EVERY);
            if redraw {
                h.last_drawn = Some(now);
            }
            (target_changed, redraw, was_shown)
        };

        let show = match (redraw, outline) {
            (true, Some(ents)) => Some(ents),
            // Nothing to draw any more, or no trap.
            (true, None) => None,
            (false, _) if target_changed => None,
            (false, _) => {
                return Update {
                    target_changed,
                    redrawn: false,
                };
            }
        };
        let mut state = get_server_state(server).write();
        let now_shown = match show {
            Some(ents) => {
                state
                    .show_virt(player, NAMESPACE, ents)
                    .send(server, player, crate::SERVER_BOT_CN);
                true
            }
            None => {
                if shown {
                    state
                        .virt
                        .clear(player, NAMESPACE)
                        .send(server, player, crate::SERVER_BOT_CN);
                }
                false
            }
        };
        drop(state);
        if let Some(mut h) = self.0.get_mut(&player) {
            h.shown = now_shown;
        }
        Update {
            target_changed,
            redrawn: redraw,
        }
    }

    /// Takes the outline away, if there is one.
    pub fn clear(&self, server: &Server, player: u64) {
        self.update(server, player, None, None);
    }

    /// Forgets a player who left.
    pub fn forget(&self, player: u64) {
        self.0.remove(&player);
    }
}
