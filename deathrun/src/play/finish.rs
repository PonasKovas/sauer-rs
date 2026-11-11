//! Reaching the flag.

use super::{PlayMaps, round::check_win_conditions};
use crate::map_state::get_server_state;
use crate::ui;

use parking_lot::RwLock;
use sauer_proto::{Client, channel1, data::EntityType};
use sauer_server::Server;
use std::sync::Arc;
use tokio::time::{Duration, Instant};

/// How close to a flag a good player has to get to finish the map.
const FLAG_REACH: f32 = 24.0;

/// Finishes the map for `id` if they are at a flag.
pub(super) fn check_finish(
    server: &Arc<Server>,
    maps: &Arc<RwLock<PlayMaps>>,
    id: u64,
    pos: [f32; 3],
    round_start: Option<Instant>,
) {
    let flag = {
        let state = get_server_state(server).read();
        let near = |e: &&ogz_rs::data::Entity| {
            let d: f32 = (0..3).map(|i| (pos[i] - e.position[i]).powi(2)).sum();
            e.ent_type == EntityType::Flag && d < FLAG_REACH * FLAG_REACH
        };
        state.map.entities.iter().find(near).map(|e| e.position)
    };
    let Some(flag) = flag else {
        return;
    };
    let time = round_start.map_or(Duration::ZERO, |t| t.elapsed());
    let first = {
        let mut lock = maps.write();
        if lock.finished_players.iter().any(|(f_id, _)| *f_id == id) {
            return;
        }
        lock.finished_players.push((id, time));
        lock.finished_players.len() == 1
    };

    let text = format!(
        "\x0C1{} \x0C7finished the map in \x0C5{:.2}s\x0C7!",
        ui::player_name(server, id),
        time.as_secs_f32()
    );
    ui::broadcast(server, ui::body(&text));
    for client_id in server.client_ids() {
        if let Some(client_cn) = server.get_cn(client_id) {
            server.send(
                client_id,
                &Client {
                    client_num: client_cn,
                    packet: channel1::Sound { sound_id: 60 },
                },
            );
        }
    }
    if first {
        fireworks(server, flag);
    }
    check_win_conditions(server, maps, None);
}

/// Particle effects at the flag for the first one to finish.
fn fireworks(server: &Server, pos: [f32; 3]) {
    const EFFECTS: [[i16; 5]; 2] = [[0, 1000, 0, -1, 0], [12, 1000, 1000, 0, 0]];
    let placed: Vec<(usize, [i16; 5])> = {
        let mut state = get_server_state(server).write();
        let map = &mut state.map;
        EFFECTS
            .iter()
            .map(|&attrs| {
                let ent = ogz_rs::data::Entity {
                    position: pos,
                    attrs,
                    ent_type: EntityType::Particles,
                    extra_data: Vec::new(),
                };
                let slot = map
                    .entities
                    .iter()
                    .position(|e| e.ent_type == EntityType::Empty);
                let i = match slot {
                    Some(i) => {
                        map.entities[i] = ent;
                        i
                    }
                    None => {
                        map.entities.push(ent);
                        map.entities.len() - 1
                    }
                };
                (i, attrs)
            })
            .collect()
    };
    for (i, attrs) in placed {
        server.broadcast(&Client {
            client_num: crate::SERVER_BOT_CN,
            packet: channel1::EditEnt {
                ent_id: i as i32,
                pos: sauer_proto::data::Vec3 {
                    x: pos[0],
                    y: pos[1],
                    z: pos[2],
                },
                type_: EntityType::Particles,
                attrs: attrs.map(i32::from),
            },
        });
    }
}
