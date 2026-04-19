use crate::{game::GameState, modes, registry::ClientRegistry};
use sauer_proto::{
    channel1::{self, SpawnClient, TrySpawn},
    data::ClientState,
};
use sauer_server::{
    Server,
    plugin::{Control, PASS},
};
use std::sync::Arc;

const RESPAWN_DELAY_MS: u64 = 5_000;

pub async fn on_try_spawn(server: &Arc<Server>, cn: u32, _packet: &TrySpawn) -> Control {
    let registry = server.extra.get::<ClientRegistry>().unwrap();
    let game = server.extra.get::<GameState>().unwrap();

    let Some(client) = registry.get(cn) else { return PASS };
    if client.state != ClientState::Dead {
        return PASS;
    }

    // Enforce respawn delay (5 s after death; no delay on first spawn).
    let can_spawn = client.death_ms.map_or(true, |died_at| {
        game.gamemillis().saturating_sub(died_at) >= RESPAWN_DELAY_MS
    });
    drop(client);

    if !can_spawn {
        return PASS;
    }

    let state = modes::default_spawn_state(game.mode());
    server.send_chan1(cn, &channel1::SpawnState { client_num: cn as i32, state });

    PASS
}

pub async fn on_spawn_client(server: &Arc<Server>, cn: u32, packet: &SpawnClient) -> Control {
    let registry = server.extra.get::<ClientRegistry>().unwrap();
    let game = server.extra.get::<GameState>().unwrap();

    {
        let Some(mut client) = registry.get_mut(cn) else { return PASS };
        if client.state != ClientState::Dead {
            return PASS;
        }

        let state = modes::default_spawn_state(game.mode());
        client.life_sequence = packet.life_sequence;
        client.gun_select = packet.gun_select;
        client.health = state.health;
        client.max_health = state.max_health;
        client.armour = state.armour;
        client.armour_type = state.armour_type;
        client.ammo = state.ammo;
        client.state = ClientState::Alive;
        client.death_ms = None;
    }

    // Broadcast the spawn to all other clients.
    let state = modes::default_spawn_state(game.mode());
    server.broadcast_except_chan1(
        cn,
        &channel1::Client {
            client_num: cn as i32,
            packet: channel1::SpawnServer { state },
        },
    );

    PASS
}
