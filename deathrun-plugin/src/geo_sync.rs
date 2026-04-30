use sauer_proto::{
    Client, channel1,
    data::{Orient, SelInfo, Vec3},
};
use sauer_server::Server;

/// Broadcast a ClipboardServer + PasteServer packet pair to all clients.
pub fn broadcast_geometry(
    server: &Server,
    owner_cn: i32,
    sel: &sauer_proto::data::SelInfo,
    unpack_len: i32,
    data: &[u8],
) {
    server.broadcast_chan1(&channel1::ClipboardServer {
        client_num: owner_cn,
        unpack_len,
        data: data.to_vec(),
    });
    server.broadcast_chan1(&Client {
        client_num: owner_cn,
        packet: channel1::PasteServer { sel: sel.clone() },
    });
}

/// Send a ClipboardServer + PasteServer packet pair to a single client.
pub fn send_geometry(
    server: &Server,
    target_cn: u32,
    owner_cn: i32,
    sel: &sauer_proto::data::SelInfo,
    unpack_len: i32,
    data: &[u8],
) {
    server.send_chan1(
        target_cn,
        &channel1::ClipboardServer {
            client_num: owner_cn,
            unpack_len,
            data: data.to_vec(),
        },
    );
    server.send_chan1(
        target_cn,
        &Client {
            client_num: owner_cn,
            packet: channel1::PasteServer { sel: sel.clone() },
        },
    );
}

/// Send all chunks of a world octree to a single client using the
/// clipboard-paste visual rebuild pattern.
///
/// `chunks` must already be collected and (optionally) shuffled by the caller.
pub async fn send_chunks_to_client(
    server: &Server,
    target_cn: u32,
    owner_cn: i32,
    chunks: Vec<([i32; 3], i32, sauer_map::Node)>,
    vslots: &[Option<ogz_rs::data::VSlot>],
    delay_ms: u64,
) {
    for (pos, size, node) in chunks {
        let (unpack_len, data) =
            sauer_map::clipboard::generate_clipboard(&node, size, vslots);
        let sel = SelInfo {
            o: Vec3 {
                x: pos[0],
                y: pos[1],
                z: pos[2],
            },
            s: Vec3 { x: 1, y: 1, z: 1 },
            grid: size,
            orient: Orient::Left,
            cx: 0,
            cxs: 1,
            cy: 0,
            cys: 1,
            corner: 0,
        };
        send_geometry(server, target_cn, owner_cn, &sel, unpack_len, &data);
        if delay_ms > 0 {
            tokio::time::sleep(std::time::Duration::from_millis(delay_ms)).await;
        }
    }
}

/// Broadcast all chunks of a world octree to all clients using the
/// clipboard-paste visual rebuild pattern.
///
/// `chunks` must already be collected and (optionally) shuffled by the caller.
pub async fn broadcast_chunks(
    server: &Server,
    owner_cn: i32,
    chunks: Vec<([i32; 3], i32, sauer_map::Node)>,
    vslots: &[Option<ogz_rs::data::VSlot>],
    delay_ms: u64,
) {
    for (pos, size, node) in chunks {
        let (unpack_len, data) =
            sauer_map::clipboard::generate_clipboard(&node, size, vslots);
        let sel = SelInfo {
            o: Vec3 {
                x: pos[0],
                y: pos[1],
                z: pos[2],
            },
            s: Vec3 { x: 1, y: 1, z: 1 },
            grid: size,
            orient: Orient::Left,
            cx: 0,
            cxs: 1,
            cy: 0,
            cys: 1,
            corner: 0,
        };
        broadcast_geometry(server, owner_cn, &sel, unpack_len, &data);
        if delay_ms > 0 {
            tokio::time::sleep(std::time::Duration::from_millis(delay_ms)).await;
        }
    }
}
