use sauer_kit::paste;
use sauer_proto::data::{ClipboardPayload, Cn};
use sauer_server::Server;
use std::time::Duration;

pub async fn broadcast_payloads(
    server: &Server,
    owner_cn: Cn,
    payloads: Vec<ClipboardPayload>,
    delay_ms: u64,
) {
    if payloads.is_empty() {
        return;
    }
    if delay_ms == 0 {
        paste::broadcast(server, owner_cn, payloads);
    } else {
        paste::broadcast_slowly(server, owner_cn, payloads, Duration::from_millis(delay_ms)).await;
    }
    after_paste(server);
}

/// Sends right away, for callers that track what they paste themselves
/// (the button placement preview).
pub fn broadcast_now(server: &Server, owner_cn: Cn, payloads: Vec<ClipboardPayload>) {
    paste::broadcast(server, owner_cn, payloads);
}

fn after_paste(server: &Server) {
    // The paste turned any see-through trap buttons in it solid again, and
    // may have covered the placement preview.
    crate::trap::placement::mark_seethrough_stale(server);
    crate::trap::place::repaint(server);
}

pub async fn send_payloads_to_clients(
    server: &Server,
    target_ids: &[u64],
    owner_cn: Cn,
    payloads: Vec<ClipboardPayload>,
    delay_ms: u64,
) {
    if payloads.is_empty() {
        return;
    }
    paste::send(
        server,
        target_ids,
        owner_cn,
        payloads,
        Duration::from_millis(delay_ms),
    )
    .await;
    after_paste(server);
}
