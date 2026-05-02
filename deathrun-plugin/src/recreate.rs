use crate::map_state::get_server_state;
use cube_string::IntoCubeStringLossy;
use rand::seq::SliceRandom;
use sauer_proto::{
    Client, channel1,
    data::{SelInfo, Vec3},
};
use sauer_server::Server;
use std::sync::Arc;

pub(crate) fn collect_chunks(
    node: &sauer_map::Node,
    node_pos: [i32; 3],
    node_size: i32,
    chunk_size_limit: i32,
    out: &mut Vec<([i32; 3], i32, sauer_map::Node)>,
) {
    if node_size <= chunk_size_limit {
        if let sauer_map::Node::Leaf(cube) = node {
            if cube.is_empty() && cube.material == ogz_rs::data::Material::default() {
                return;
            }
        }
        out.push((node_pos, node_size, node.clone()));
        return;
    }

    match node {
        sauer_map::Node::Branch(children) => {
            let child_size = node_size / 2;
            for i in 0..8 {
                let cx = (i & 1) as i32;
                let cy = ((i >> 1) & 1) as i32;
                let cz = ((i >> 2) & 1) as i32;
                let child_pos = [
                    node_pos[0] + cx * child_size,
                    node_pos[1] + cy * child_size,
                    node_pos[2] + cz * child_size,
                ];
                collect_chunks(&children[i], child_pos, child_size, chunk_size_limit, out);
            }
        }
        sauer_map::Node::Leaf(cube) => {
            if !(cube.is_empty() && cube.material == ogz_rs::data::Material::default()) {
                out.push((node_pos, node_size, node.clone()));
            }
        }
    }
}

pub async fn visual_rebuild_map<F>(
    server: &Server,
    target_cns: &[u32],
    author_cn: i32,
    world_size: i32,
    root: &sauer_map::Node,
    vslots: &[Option<ogz_rs::data::VSlot>],
    variables: &[ogz_rs::data::Variable],
    entities: &[ogz_rs::data::Entity],
    chunk_divisions: i32,
    chunk_delay_ms: u64,
    send_newmap: bool,
    mut entity_transformer: F,
) where
    F: FnMut(u32, usize, &ogz_rs::data::Entity) -> Option<ogz_rs::data::Entity>,
{
    if target_cns.is_empty() {
        return;
    }

    println!("world_size: {world_size}");
    let clear_size = world_size.max(1024);

    for &cn in target_cns {
        if send_newmap {
            server.send_chan1(
                cn,
                &Client {
                    client_num: cn as i32,
                    packet: channel1::NewMap {
                        size: (world_size.trailing_zeros() as i32).max(10),
                    },
                },
            );
        }

        server.send_chan1(
            cn,
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
            if let Some(mut p) = server.players.try_get_mut(cn) {
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
                    cn,
                    &Client {
                        client_num: author_cn,
                        packet: channel1::EditVar { name, val },
                    },
                );
            }
        }

        for i in 0..entities.len() {
            server.send_chan1(
                cn,
                &Client {
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
                },
            );
        }
    }

    tokio::time::sleep(tokio::time::Duration::from_millis(500)).await;

    let mut chunks = Vec::new();
    let chunk_size_limit = world_size / chunk_divisions;
    collect_chunks(root, [0, 0, 0], world_size, chunk_size_limit, &mut chunks);

    // Optional random shuffle to make it look cool, like in original recreate
    use rand::seq::SliceRandom;
    chunks.shuffle(&mut rand::thread_rng());

    crate::geo_sync::send_chunks_to_clients(
        server,
        target_cns,
        author_cn,
        chunks,
        vslots,
        chunk_delay_ms,
    )
    .await;

    tokio::time::sleep(tokio::time::Duration::from_millis(500)).await;

    for &cn in target_cns {
        for (i, ent) in entities.iter().enumerate() {
            if let Some(transformed) = entity_transformer(cn, i, ent) {
                server.send_chan1(
                    cn,
                    &Client {
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
                    },
                );
            }
        }
    }
}

pub fn spawn_recreate_map(server: Arc<Server>, cn: u32, divisions: i32, delay_ms: u64) {
    let (map_size, root, entities, vslots, variables) = {
        let state_lock = get_server_state(&server).read().unwrap();
        let state = match state_lock.map.as_ref() {
            Some(s) => s,
            None => {
                server.send_chan1(
                    cn,
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
            state.world.size,
            state.world.root.clone(),
            state.world.entities.clone(),
            state.meta.vslots.clone(),
            vars,
        )
    };

    tokio::spawn(async move {
        server.broadcast_chan1(&channel1::ServMsg {
            text: "Morphing map sequence initiated...".lossy_cubestring(),
        });

        let cns = server.client_cns();
        visual_rebuild_map(
            &server,
            &cns,
            cn as i32,
            map_size,
            &root,
            &vslots,
            &variables,
            &entities,
            divisions,
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
