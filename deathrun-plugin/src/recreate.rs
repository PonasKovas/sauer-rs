use crate::map_state::get_server_state;
use cube_string::IntoCubeStringLossy;
use sauer_proto::{
    Client, channel1,
    data::{SelInfo, Vec3},
};
use sauer_server::Server;
use std::sync::Arc;

pub async fn visual_rebuild_map<F>(
    server: &Server,
    target_ids: &[u64],
    author_cn: i32,
    world: &sauer_map::World,
    vslots: &[Option<ogz_rs::data::VSlot>],
    variables: &[ogz_rs::data::Variable],
    entities: &[ogz_rs::data::Entity],
    chunk_delay_ms: u64,
    send_newmap: bool,
    mut entity_transformer: F,
) where
    F: FnMut(u64, usize, &ogz_rs::data::Entity) -> Option<ogz_rs::data::Entity>,
{
    if target_ids.is_empty() {
        return;
    }

    println!("world_size: {}", world.size);
    let clear_size = world.size.max(1024);

    for &id in target_ids {
        if send_newmap {
            let cn = server.get_cn(id).unwrap_or(0) as i32;
            server.send_chan1(
                id,
                &Client {
                    client_num: cn,
                    packet: channel1::NewMap {
                        size: (world.size.trailing_zeros() as i32).max(10),
                    },
                },
            );
        }

        server.send_chan1(
            id,
            &Client {
                client_num: author_cn,
                packet: channel1::DelCube {
                    sel: SelInfo {
                        o: Vec3 { x: 0, y: 0, z: 0 },
                        s: Vec3 { x: 2, y: 2, z: 2 },
                        grid: clear_size / 2,
                        orient: sauer_proto::data::Orient::Left,
                        cx: 0,
                        cxs: 2,
                        cy: 0,
                        cys: 2,
                        corner: 0,
                    },
                },
            },
        );

        for var in variables {
            let name = match var {
                ogz_rs::data::Variable::Int { name, .. } => name.clone(),
                ogz_rs::data::Variable::Float { name, .. } => name.clone(),
                ogz_rs::data::Variable::String { name, .. } => name.clone(),
            };

            let mut send = true;
            if let Some(mut p) = server.players.try_get_mut(id) {
                if let Some(known) = p.known_vars.get(&name) {
                    if known == var {
                        send = false;
                    }
                }
                if send {
                    p.known_vars.insert(name.clone(), var.clone());
                }
            }

            if send {
                let val = match var {
                    ogz_rs::data::Variable::Int { value, .. } => {
                        sauer_proto::data::EditVarValue::Int(*value)
                    }
                    ogz_rs::data::Variable::Float { value, .. } => {
                        sauer_proto::data::EditVarValue::Float(*value)
                    }
                    ogz_rs::data::Variable::String { value, .. } => {
                        sauer_proto::data::EditVarValue::String(value.clone())
                    }
                };
                server.send_chan1(
                    id,
                    &Client {
                        client_num: author_cn,
                        packet: channel1::EditVar { name, val },
                    },
                );
            }
        }

        let mut empty_batch = Vec::new();
        for i in 0..entities.len() {
            empty_batch.push(Client {
                client_num: author_cn,
                packet: channel1::EditEnt {
                    ent_id: i as i32,
                    pos: Vec3 {
                        x: 0.0,
                        y: 0.0,
                        z: 0.0,
                    },
                    type_: sauer_proto::data::EntityType::Empty,
                    attrs: [0; 5],
                },
            });
            if empty_batch.len() >= 50 {
                server.send_chan1(id, empty_batch.clone());
                empty_batch.clear();
            }
        }
        if !empty_batch.is_empty() {
            server.send_chan1(id, empty_batch);
        }
    }

    tokio::time::sleep(tokio::time::Duration::from_millis(500)).await;

    let mut payloads = sauer_map::chunking::chunk_region(
        world,
        vslots,
        [0, 0, 0],
        [world.size, world.size, world.size],
        4000,
    );

    // Optional random shuffle to make it look cool, like in original recreate
    use rand::seq::SliceRandom;
    payloads.shuffle(&mut rand::thread_rng());

    crate::geo_sync::send_payloads_to_clients(
        server,
        target_ids,
        author_cn,
        payloads,
        chunk_delay_ms,
    )
    .await;

    tokio::time::sleep(tokio::time::Duration::from_millis(500)).await;

    for &id in target_ids {
        let mut batch = Vec::new();
        for (i, ent) in entities.iter().enumerate() {
            if let Some(transformed) = entity_transformer(id, i, ent) {
                batch.push(Client {
                    client_num: author_cn,
                    packet: channel1::EditEnt {
                        ent_id: i as i32,
                        pos: sauer_proto::data::Vec3 {
                            x: transformed.position[0],
                            y: transformed.position[1],
                            z: transformed.position[2],
                        },
                        type_: transformed.ent_type,
                        attrs: transformed.attrs.map(|x| x as i32),
                    },
                });
                if batch.len() >= 50 {
                    server.send_chan1(id, batch.clone());
                    batch.clear();
                }
            }
        }
        if !batch.is_empty() {
            server.send_chan1(id, batch);
        }
    }
}

pub fn spawn_recreate_map(server: Arc<Server>, id: u64, delay_ms: u64) {
    let (world, entities, vslots, variables) = {
        let state_lock = get_server_state(&server).read().unwrap();
        let state = match state_lock.map.as_ref() {
            Some(s) => s,
            None => {
                server.send_chan1(
                    id,
                    &channel1::ServMsg {
                        text: "No custom map uploaded to recreate!".lossy_cubestring(),
                    },
                );
                return;
            }
        };
        let vars = match &state.meta.config {
            ogz_rs::data::MapConfig::Modern { variables } => variables.clone(),
            _ => vec![],
        };
        (
            state.world.clone(),
            state.world.entities.clone(),
            state.meta.vslots.clone(),
            vars,
        )
    };

    tokio::spawn(async move {
        server.broadcast_chan1(&channel1::ServMsg {
            text: "Morphing map sequence initiated...".lossy_cubestring(),
        });

        let ids = server.client_ids();
        let author_cn = crate::SERVER_BOT_CN as i32;

        visual_rebuild_map(
            &server,
            &ids,
            author_cn,
            &world,
            &vslots,
            &variables,
            &entities,
            delay_ms,
            false,
            |_, _, ent| Some(ent.clone()),
        )
        .await;

        server.broadcast_chan1(&channel1::ServMsg {
            text: "Map morphing reconstruction complete!".lossy_cubestring(),
        });
    });
}
