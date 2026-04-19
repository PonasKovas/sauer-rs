use crate::registry::ClientRegistry;
use sauer_proto::channel1::{self, ClientPing, Ping};
use sauer_server::{
    Server,
    plugin::{Control, PASS},
};
use std::sync::Arc;

pub async fn on_ping(server: &Arc<Server>, cn: u32, packet: &Ping) -> Control {
    // Echo the client's timestamp back immediately so it can calculate RTT.
    server.send_chan1(cn, &channel1::Pong { ping: packet.time });
    PASS
}

pub async fn on_client_ping(server: &Arc<Server>, cn: u32, packet: &ClientPing) -> Control {
    let registry = server.extra.get::<ClientRegistry>().unwrap();
    registry.record_ping(cn, packet.ping);

    // Broadcast to all other clients so their scoreboards stay in sync.
    server.broadcast_except_chan1(
        cn,
        &channel1::Client {
            client_num: cn as i32,
            packet: channel1::ClientPing { ping: packet.ping },
        },
    );

    PASS
}
