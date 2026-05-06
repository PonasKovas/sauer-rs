use sauer_proto::{
    Client, channel1,
    data::ClipboardPayload,
};
use sauer_server::Server;

pub async fn broadcast_payloads(
    server: &Server,
    owner_cn: i32,
    payloads: Vec<ClipboardPayload>,
    delay_ms: u64,
) {
    for payload in payloads {
        let sel = payload.sel.clone();
        
        server.broadcast_chan1(&Client {
            client_num: owner_cn,
            packet: channel1::DelCube { sel: sel.clone() },
        });

        server.broadcast_chan1(&channel1::ClipboardServer {
            client_num: owner_cn,
            payload: payload.clone(),
        });

        server.broadcast_chan1(&Client {
            client_num: owner_cn,
            packet: channel1::PasteServer { sel },
        });

        if delay_ms > 0 {
            tokio::time::sleep(std::time::Duration::from_millis(delay_ms)).await;
        }
    }
}

pub async fn send_payloads_to_clients(
    server: &Server,
    target_ids: &[u64],
    owner_cn: i32,
    payloads: Vec<ClipboardPayload>,
    delay_ms: u64,
) {
    for payload in payloads {
        let sel = payload.sel.clone();
        
        let del_cube = Client {
            client_num: owner_cn,
            packet: channel1::DelCube { sel: sel.clone() },
        };
        let clipboard = channel1::ClipboardServer {
            client_num: owner_cn,
            payload: payload.clone(),
        };
        let paste = Client {
            client_num: owner_cn,
            packet: channel1::PasteServer { sel },
        };

        for &target_id in target_ids {
            server.send_chan1(target_id, &del_cube);
            server.send_chan1(target_id, &clipboard);
            server.send_chan1(target_id, &paste);
        }

        if delay_ms > 0 {
            tokio::time::sleep(std::time::Duration::from_millis(delay_ms)).await;
        }
    }
}
