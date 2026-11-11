//! Sending map geometry to clients as clipboard pastes, the only way the
//! protocol has to change a region of a running client's map.

use sauer_core::Aabb;
use sauer_map::Map;
use sauer_proto::data::{ClipboardPayload, Cn, MAX_CLIENT_UNPACK_LEN};
use sauer_proto::route::{Chan1, S2C};
use sauer_proto::{Client, Encode, channel1};
use sauer_server::Server;

/// Packets of at most this many bytes paste without making clients lag.
pub const SMALL_PASTE_BYTES: usize = 4000;

/// The region as it is in `map`, as pastes of at most [`SMALL_PASTE_BYTES`].
pub fn restore_payloads(map: &Map, b: Aabb) -> Vec<ClipboardPayload> {
    chunk_region(map, b, SMALL_PASTE_BYTES)
}

/// DelCube + clipboard + paste as one packet. Clients keep one clipboard per
/// client number, so sending these separately would let two concurrent
/// senders paste each other's clipboards.
pub fn paste_packets(
    from: Cn,
    payload: ClipboardPayload,
) -> (
    Client<channel1::DelCube>,
    channel1::ClipboardServer,
    Client<channel1::PasteServer>,
) {
    let sel = payload.sel.clone();
    (
        Client {
            client_num: from,
            packet: channel1::DelCube { sel: sel.clone() },
        },
        channel1::ClipboardServer {
            client_num: from,
            payload: Some(payload),
        },
        Client {
            client_num: from,
            packet: channel1::PasteServer { sel },
        },
    )
}

/// Pastes each payload on every client, as if `from` pasted it.
pub fn broadcast(server: &Server, from: Cn, payloads: Vec<ClipboardPayload>) {
    for payload in payloads {
        server.broadcast(paste_packets(from, payload));
    }
}

/// Like [`broadcast`], waiting `delay` between pastes so clients keep up.
pub async fn broadcast_slowly(
    server: &Server,
    from: Cn,
    payloads: Vec<ClipboardPayload>,
    delay: std::time::Duration,
) {
    for payload in payloads {
        server.broadcast(paste_packets(from, payload));
        tokio::time::sleep(delay).await;
    }
}

/// Pastes each payload on the `targets`' clients only.
pub async fn send(
    server: &Server,
    targets: &[u64],
    from: Cn,
    payloads: Vec<ClipboardPayload>,
    delay: std::time::Duration,
) {
    for payload in payloads {
        let packets = paste_packets(from, payload);
        for &id in targets {
            server.send(id, &packets);
        }
        if !delay.is_zero() {
            tokio::time::sleep(delay).await;
        }
    }
}

/// Splits the region into clipboards whose packets fit in `max_bytes`
/// (compressed) and that clients accept (unpacked size). Pass `usize::MAX`
/// for as few clipboards as clients allow.
pub fn chunk_region(map: &Map, b: sauer_core::Aabb, max_bytes: usize) -> Vec<ClipboardPayload> {
    let mut out = Vec::new();
    do_chunk(map, b.min, b.max, max_bytes, &mut out);
    out
}

fn do_chunk(
    map: &Map,
    min: [i32; 3],
    max: [i32; 3],
    max_bytes: usize,
    out: &mut Vec<ClipboardPayload>,
) {
    let (padded_min, [sx, sy, sz], grid) = sauer_map::clipboard::region_layout(map, min, max);
    let shaped = sauer_map::octree::max_shaped_leaf(&map.root, [0; 3], map.size, min, max);

    // Check if it's 1x1x1 and cannot be split further
    // Going smaller than `shaped` would split a shaped leaf.
    let cannot_split = sx <= 1 && sy <= 1 && sz <= 1 && grid <= shaped.max(1);

    let payload =
        sauer_map::clipboard::generate_clipboard_for_region(map, padded_min, [sx, sy, sz], grid);

    let fits = payload.uncompressed().len() <= MAX_CLIENT_UNPACK_LEN
        && (max_bytes == usize::MAX || {
            let packet = channel1::ClipboardServer {
                client_num: sauer_proto::data::Cn(0),
                payload: Some(payload.clone()),
            };
            let mut buf = Vec::new();
            <channel1::ClipboardServer as Encode<S2C<Chan1>>>::encode(&packet, &mut buf);
            buf.len() <= max_bytes
        });

    if fits || cannot_split {
        out.push(payload);
    } else {
        let mid = [
            min[0] + (max[0] - min[0]) / 2,
            min[1] + (max[1] - min[1]) / 2,
            min[2] + (max[2] - min[2]) / 2,
        ];

        let mut sub_boxes = Vec::new();
        for i in 0..8 {
            let cx = i & 1;
            let cy = (i >> 1) & 1;
            let cz = (i >> 2) & 1;

            let c_min = [
                if cx == 0 { min[0] } else { mid[0] },
                if cy == 0 { min[1] } else { mid[1] },
                if cz == 0 { min[2] } else { mid[2] },
            ];

            let c_max = [
                if cx == 0 { mid[0] } else { max[0] },
                if cy == 0 { mid[1] } else { max[1] },
                if cz == 0 { mid[2] } else { max[2] },
            ];

            if c_min[0] < c_max[0] && c_min[1] < c_max[1] && c_min[2] < c_max[2] {
                sub_boxes.push((c_min, c_max));
            }
        }

        for (c_min, c_max) in sub_boxes {
            do_chunk(map, c_min, c_max, max_bytes, out);
        }
    }
}
