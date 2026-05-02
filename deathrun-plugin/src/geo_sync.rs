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

/// Send a ClipboardServer + PasteServer packet pair to multiple clients.
pub fn send_geometry_to_clients(
    server: &Server,
    target_ids: &[u64],
    owner_cn: i32,
    sel: &sauer_proto::data::SelInfo,
    unpack_len: i32,
    data: &[u8],
) {
    for &target_id in target_ids {
        server.send_chan1(
            target_id,
            &channel1::ClipboardServer {
                client_num: owner_cn,
                unpack_len,
                data: data.to_vec(),
            },
        );
        server.send_chan1(
            target_id,
            &Client {
                client_num: owner_cn,
                packet: channel1::PasteServer { sel: sel.clone() },
            },
        );
    }
}

/// Send all chunks of a world octree to multiple clients using the
/// clipboard-paste visual rebuild pattern.
///
/// `chunks` must already be collected and (optionally) shuffled by the caller.
pub async fn send_chunks_to_clients(
    server: &Server,
    target_ids: &[u64],
    owner_cn: i32,
    chunks: Vec<([i32; 3], i32, sauer_map::Node)>,
    vslots: &[Option<ogz_rs::data::VSlot>],
    delay_ms: u64,
) {
    let mut queue: std::collections::VecDeque<_> = chunks.into();
    while let Some((pos, size, node)) = queue.pop_front() {
        let (unpack_len, data) = sauer_map::clipboard::generate_clipboard(&node, size, vslots);

        if unpack_len > 1000 * 1024 && size > 1 {
            let half = size / 2;
            match node {
                sauer_map::Node::Branch(children) => {
                    for i in 0..8 {
                        let cx = (i & 1) as i32;
                        let cy = ((i >> 1) & 1) as i32;
                        let cz = ((i >> 2) & 1) as i32;
                        let child_pos =
                            [pos[0] + cx * half, pos[1] + cy * half, pos[2] + cz * half];
                        queue.push_back((child_pos, half, children[i].clone()));
                    }
                }
                sauer_map::Node::Leaf(cube) => {
                    let children = sauer_map::subdivide(&cube);
                    for i in 0..8 {
                        let cx = (i & 1) as i32;
                        let cy = ((i >> 1) & 1) as i32;
                        let cz = ((i >> 2) & 1) as i32;
                        let child_pos =
                            [pos[0] + cx * half, pos[1] + cy * half, pos[2] + cz * half];
                        queue.push_back((
                            child_pos,
                            half,
                            sauer_map::Node::Leaf(children[i as usize].clone()),
                        ));
                    }
                }
            }
            continue;
        }

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
        send_geometry_to_clients(server, target_ids, owner_cn, &sel, unpack_len, &data);
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
    let mut queue: std::collections::VecDeque<_> = chunks.into();
    while let Some((pos, size, node)) = queue.pop_front() {
        let (unpack_len, data) = sauer_map::clipboard::generate_clipboard(&node, size, vslots);

        if unpack_len > 1000 * 1024 && size > 1 {
            let half = size / 2;
            match node {
                sauer_map::Node::Branch(children) => {
                    for i in 0..8 {
                        let cx = (i & 1) as i32;
                        let cy = ((i >> 1) & 1) as i32;
                        let cz = ((i >> 2) & 1) as i32;
                        let child_pos =
                            [pos[0] + cx * half, pos[1] + cy * half, pos[2] + cz * half];
                        queue.push_back((child_pos, half, children[i].clone()));
                    }
                }
                sauer_map::Node::Leaf(cube) => {
                    let children = sauer_map::subdivide(&cube);
                    for i in 0..8 {
                        let cx = (i & 1) as i32;
                        let cy = ((i >> 1) & 1) as i32;
                        let cz = ((i >> 2) & 1) as i32;
                        let child_pos =
                            [pos[0] + cx * half, pos[1] + cy * half, pos[2] + cz * half];
                        queue.push_back((
                            child_pos,
                            half,
                            sauer_map::Node::Leaf(children[i as usize].clone()),
                        ));
                    }
                }
            }
            continue;
        }

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
