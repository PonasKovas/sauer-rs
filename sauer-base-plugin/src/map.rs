use crate::{game::GameState, modes, options::BasePluginOptions, registry::ClientRegistry};
use sauer_proto::{
    channel1::{self, ForceIntermission, GameSpeedClient, ItemList, MapVote, NewMap, PauseGameClient},
    data::{GameMode, Privilege},
};
use sauer_server::{
    Server,
    plugin::{Control, PASS},
};
use std::sync::{Arc, atomic::Ordering};
use tracing::info;

pub async fn on_map_vote(
    server: &Arc<Server>,
    cn: u32,
    packet: &MapVote,
    options: &BasePluginOptions,
) -> Control {
    let registry = server.extra.get::<ClientRegistry>().unwrap();
    let game = server.extra.get::<GameState>().unwrap();

    let privilege = registry.get(cn).map(|c| c.privilege()).unwrap_or(Privilege::None);

    if privilege >= Privilege::Master {
        info!("cn {} forced map {} ({:?})", cn, packet.map_name, packet.mode);
        game.change_map(packet.map_name.clone(), packet.mode, server, &registry, options).await;
        return PASS;
    }

    let vote_count = game.record_vote(cn, packet.map_name.clone(), packet.mode);
    let eligible = registry.client_count();
    info!(
        "cn {} voted for {} ({:?}): {}/{}",
        cn, packet.map_name, packet.mode, vote_count, eligible
    );

    if vote_count * 2 > eligible {
        info!("vote passed for {} ({:?})", packet.map_name, packet.mode);
        game.change_map(packet.map_name.clone(), packet.mode, server, &registry, options).await;
    }

    PASS
}

pub async fn on_new_map(server: &Arc<Server>, cn: u32, packet: &NewMap) -> Control {
    let game = server.extra.get::<GameState>().unwrap();

    if game.mode() != GameMode::CoopEdit {
        return PASS;
    }

    info!("cn {} resized/cleared map to size {}", cn, packet.size);
    game.notgotitems.store(false, Ordering::Release);
    game.items.lock().unwrap().clear();

    server.broadcast_chan1(&channel1::MapChange {
        map_name: game.map_name(),
        mode: GameMode::CoopEdit,
        has_items: false,
    });

    PASS
}

pub async fn on_item_list(server: &Arc<Server>, cn: u32, packet: &ItemList) -> Control {
    let game = server.extra.get::<GameState>().unwrap();

    if !game.notgotitems() {
        return PASS;
    }

    game.notgotitems.store(false, Ordering::Release);
    *game.items.lock().unwrap() = packet.items.clone();
    info!("cn {} provided item list: {} items", cn, packet.items.len());

    PASS
}

pub async fn on_force_intermission(
    server: &Arc<Server>,
    cn: u32,
    _packet: &ForceIntermission,
) -> Control {
    let registry = server.extra.get::<ClientRegistry>().unwrap();
    let privilege = registry.get(cn).map(|c| c.privilege()).unwrap_or(Privilege::None);

    if privilege < Privilege::Master {
        return PASS;
    }

    let game = server.extra.get::<GameState>().unwrap();
    info!("cn {} forced intermission", cn);
    game.start_intermission(server);

    PASS
}

pub async fn on_pause_game(server: &Arc<Server>, cn: u32, packet: &PauseGameClient) -> Control {
    let registry = server.extra.get::<ClientRegistry>().unwrap();
    let privilege = registry.get(cn).map(|c| c.privilege()).unwrap_or(Privilege::None);

    if privilege < Privilege::Master {
        return PASS;
    }

    let game = server.extra.get::<GameState>().unwrap();
    game.paused.store(packet.val, Ordering::Release);
    info!("cn {} {} the game", cn, if packet.val { "paused" } else { "unpaused" });

    server.broadcast_chan1(&channel1::PauseGameServer {
        is_paused: packet.val,
        client_num: cn as i32,
    });

    PASS
}

pub async fn on_game_speed(server: &Arc<Server>, cn: u32, packet: &GameSpeedClient) -> Control {
    let registry = server.extra.get::<ClientRegistry>().unwrap();
    let privilege = registry.get(cn).map(|c| c.privilege()).unwrap_or(Privilege::None);

    if privilege < Privilege::Admin {
        return PASS;
    }

    let speed = packet.speed.clamp(10, 1000);
    let game = server.extra.get::<GameState>().unwrap();
    game.gamespeed.store(speed, Ordering::Release);
    info!("cn {} set game speed to {}", cn, speed);

    server.broadcast_chan1(&channel1::GameSpeedServer {
        speed,
        client_num: cn as i32,
    });

    PASS
}

/// Send current map state to a newly connected client.
pub(crate) fn send_map_info(server: &Server, cn: u32) {
    let Some(game) = server.extra.get::<GameState>() else { return };
    let mode = game.mode();
    server.send_chan1(
        cn,
        &channel1::MapChange {
            map_name: game.map_name(),
            mode,
            has_items: modes::has_items(mode),
        },
    );
    if modes::has_items(mode) && !game.notgotitems() {
        let items = game.items.lock().unwrap().clone();
        if !items.is_empty() {
            server.send_chan1(cn, &channel1::ItemList { items });
        }
    }
}
