use sauer_proto::{Client, channel1, data::ClipboardPayload};
use sauer_server::Server;

/// DelCube + clipboard + paste as one packet. Clients keep one clipboard per
/// client number, so sending these separately would let two concurrent
/// senders paste each other's clipboards.
fn paste_packets(
    owner_cn: sauer_proto::data::Cn,
    payload: ClipboardPayload,
) -> (
    Client<channel1::DelCube>,
    channel1::ClipboardServer,
    Client<channel1::PasteServer>,
) {
    let sel = payload.sel.clone();
    (
        Client {
            client_num: owner_cn,
            packet: channel1::DelCube { sel: sel.clone() },
        },
        channel1::ClipboardServer {
            client_num: owner_cn,
            payload: Some(payload),
        },
        Client {
            client_num: owner_cn,
            packet: channel1::PasteServer { sel },
        },
    )
}

pub async fn broadcast_payloads(
    server: &Server,
    owner_cn: sauer_proto::data::Cn,
    payloads: Vec<ClipboardPayload>,
    delay_ms: u64,
) {
    if payloads.is_empty() {
        return;
    }
    for payload in payloads {
        server.broadcast_chan1(paste_packets(owner_cn, payload));

        if delay_ms > 0 {
            tokio::time::sleep(std::time::Duration::from_millis(delay_ms)).await;
        }
    }
    after_paste(server);
}

/// Sends right away, for callers that track what they paste themselves
/// (the button placement preview).
pub fn broadcast_now(
    server: &Server,
    owner_cn: sauer_proto::data::Cn,
    payloads: Vec<ClipboardPayload>,
) {
    for payload in payloads {
        server.broadcast_chan1(paste_packets(owner_cn, payload));
    }
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
    owner_cn: sauer_proto::data::Cn,
    payloads: Vec<ClipboardPayload>,
    delay_ms: u64,
) {
    if payloads.is_empty() {
        return;
    }
    for payload in payloads {
        let packets = paste_packets(owner_cn, payload);
        for &target_id in target_ids {
            server.send_chan1(target_id, &packets);
        }

        if delay_ms > 0 {
            tokio::time::sleep(std::time::Duration::from_millis(delay_ms)).await;
        }
    }
    after_paste(server);
}
