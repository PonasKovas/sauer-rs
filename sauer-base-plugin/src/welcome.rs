use sauer_proto::channel1::{MapChange, Welcome};
use sauer_server::Server;

use crate::options::get_options;

/// Sends a `N_WELCOME` packet and so on, only call this if you disabled instant welcome in [`BasePluginOptions`]
pub fn welcome(server: &Server, cn: u32) {
    server.send_chan1(cn, &Welcome {});

    let options = get_options(server).read().unwrap();

    server.send_chan1(
        cn,
        &MapChange {
            map_name: options.map.clone(),
            mode: options.mode,
            has_items: false, // TODO
        },
    );
}
