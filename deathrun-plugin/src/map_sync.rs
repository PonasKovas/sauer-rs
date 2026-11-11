//! Getting map geometry to clients: pasting changed regions, rebuilding the
//! whole map in place for players who already have one, and sending the map
//! file.

use crate::map_state::get_server_state;
use dashmap::DashMap;
use ogz_rs::data::{Entity, EntityType, Variable};
use sauer_core::Aabb;
use sauer_kit::coop_edit::edit_ent_packet;
use sauer_kit::paste;
use sauer_map::Map;
use sauer_proto::data::{ClipboardPayload, Cn, SelInfo};
use sauer_proto::{Client, channel1, channel2};
use sauer_server::Server;
use std::collections::HashMap;
use std::sync::Arc;
use std::time::Duration;

/// Pastes `payloads` on every client as the server bot, then fixes what a
/// paste breaks (see-through buttons, the placement preview). Runs in its
/// own task, so callers may hold the state lock.
pub fn paste_everywhere(server: &Arc<Server>, payloads: Vec<ClipboardPayload>) {
    if payloads.is_empty() {
        return;
    }
    let server = Arc::clone(server);
    tokio::spawn(async move {
        paste::broadcast(&server, crate::SERVER_BOT_CN, payloads);
        after_paste(&server);
    });
}

/// Pastes right away and nothing more, for the button placement preview,
/// which tracks what it pastes itself.
pub fn paste_preview(server: &Server, payloads: Vec<ClipboardPayload>) {
    paste::broadcast(server, crate::SERVER_BOT_CN, payloads);
}

fn after_paste(server: &Server) {
    // The paste turned any see-through trap buttons in it solid again, and
    // may have covered the placement preview.
    crate::trap::placement::mark_seethrough_stale(server);
    crate::trap::place::repaint(server);
}

/// Sends the server's map as a file, as for `/getmap`.
pub fn send_map_file(server: &Arc<Server>, id: u64) {
    let server = Arc::clone(server);
    tokio::spawn(async move {
        let map = get_server_state(&server).read().map.clone();
        let bytes = tokio::task::spawn_blocking(move || {
            map.to_ogz_bytes_merged(flate2::Compression::fast())
        })
        .await;
        match bytes {
            Ok(Ok(data)) => server.send(id, &channel2::SendMap { data }),
            Ok(Err(e)) => tracing::error!("failed to serialize map state: {e}"),
            Err(e) => tracing::error!("serializing the map panicked: {e}"),
        }
    });
}

/// Map vars each client has, so a rebuild only sends the ones that differ.
#[derive(Default)]
pub struct KnownVars(DashMap<u64, HashMap<cube_string::CubeString, Variable>>);

impl KnownVars {
    pub fn forget(&self, id: u64) {
        self.0.remove(&id);
    }
}

/// Which sides a map has their own player starts for (attr 1: 1 good, 2 evil).
#[derive(Clone, Copy)]
struct Sides {
    good: bool,
    evil: bool,
}

impl Sides {
    fn of(map: &Map) -> Self {
        let has = |team| {
            map.entities
                .iter()
                .any(|e| e.ent_type == EntityType::PlayerStart && e.attrs[1] == team)
        };
        Self {
            good: has(1),
            evil: has(2),
        }
    }
}

/// The player start as one side sees it: their own side's starts become
/// plain starts, the other side's go, and untagged starts stay unless the
/// map tags starts for this side.
fn player_start_for(ent: &Entity, evil: bool, sides: Sides) -> Option<Entity> {
    if ent.ent_type != EntityType::PlayerStart {
        return Some(ent.clone());
    }
    let (own, other, has_own) = if evil {
        (2, 1, sides.evil)
    } else {
        (1, 2, sides.good)
    };
    let mut ent = ent.clone();
    match ent.attrs[1] {
        t if t == own => ent.attrs[1] = 0,
        t if t == other => return None,
        0 if has_own => return None,
        _ => {}
    }
    Some(ent)
}

/// How long to wait between pastes of a rebuild, so clients keep up.
const REBUILD_PASTE_DELAY: Duration = Duration::from_millis(20);

/// Replaces the map `targets` have with `map`, without them reloading:
/// a new empty map, the vars that differ, then the geometry as pastes and
/// the entities with player starts for their side (`evil` is the
/// terrorist, if any).
pub async fn rebuild(server: &Server, targets: &[u64], map: &Map, evil: Option<u64>) {
    if targets.is_empty() {
        return;
    }
    let from = crate::SERVER_BOT_CN;
    let known = server
        .extra
        .get::<KnownVars>()
        .expect("DeathrunPlay::init sets up KnownVars");

    for &id in targets {
        start_over(server, id, map, from, known);
    }

    tokio::time::sleep(Duration::from_millis(500)).await;
    let mut payloads =
        paste::chunk_region(map, Aabb::cube([0; 3], map.size), paste::SMALL_PASTE_BYTES);
    // In a random order: it looks better building up than sweeping across.
    rand::seq::SliceRandom::shuffle(payloads.as_mut_slice(), &mut rand::thread_rng());
    paste::send(server, targets, from, payloads, REBUILD_PASTE_DELAY).await;
    tokio::time::sleep(Duration::from_millis(500)).await;

    let sides = Sides::of(map);
    for &id in targets {
        let is_evil = evil == Some(id);
        let ents: Vec<_> = map
            .entities
            .iter()
            .enumerate()
            .filter_map(|(i, e)| Some((i, player_start_for(e, is_evil, sides)?)))
            .collect();
        for chunk in ents.chunks(50) {
            let packets: Vec<_> = chunk
                .iter()
                .map(|(i, e)| Client {
                    client_num: from,
                    packet: edit_ent_packet(*i, e),
                })
                .collect();
            server.send(id, packets);
        }
    }
    after_paste(server);
}

/// A new map the size of `map` on `id`'s client, its vars, and every
/// entity slot emptied.
fn start_over(server: &Server, id: u64, map: &Map, from: Cn, known: &KnownVars) {
    // The client resets all map vars to defaults on a new map.
    known.0.insert(
        id,
        sauer_map::map_vars::new_map_vars()
            .map(|var| (sauer_map::map_vars::var_name(&var).clone(), var))
            .collect(),
    );
    let cn = server.get_cn(id).unwrap_or(Cn(0));
    server.send(
        id,
        &Client {
            client_num: cn,
            packet: channel1::NewMap {
                size: (map.size.trailing_zeros() as i32).max(10),
            },
        },
    );
    // A new map isn't empty: it has a floor.
    let clear = map.size.max(1024) / 2;
    server.send(
        id,
        &Client {
            client_num: from,
            packet: channel1::DelCube {
                sel: SelInfo::new([0; 3], [2; 3], clear, sauer_proto::data::Orient::Left),
            },
        },
    );

    for var in &map.vars {
        let name = sauer_map::map_vars::var_name(var).clone();
        if sauer_map::map_vars::is_read_only(&name.to_string()) {
            continue;
        }
        // Only vars the client doesn't already have go out.
        let changed = known
            .0
            .entry(id)
            .or_default()
            .insert(name.clone(), var.clone())
            .as_ref()
            != Some(var);
        if changed {
            server.send(
                id,
                &Client {
                    client_num: from,
                    packet: channel1::EditVar {
                        name,
                        val: sauer_map::map_vars::edit_value(var),
                    },
                },
            );
        }
    }

    let empty = Entity::EMPTY;
    for start in (0..map.entities.len()).step_by(50) {
        let end = (start + 50).min(map.entities.len());
        let packets: Vec<_> = (start..end)
            .map(|i| Client {
                client_num: from,
                packet: edit_ent_packet(i, &empty),
            })
            .collect();
        server.send(id, packets);
    }
}

/// Sends the map's entities in slots `ids` to everyone, as the server bot.
pub fn broadcast_ents(server: &Server, map: &Map, ids: impl IntoIterator<Item = usize>) {
    let packets: Vec<_> = ids
        .into_iter()
        .map(|i| Client {
            client_num: crate::SERVER_BOT_CN,
            packet: edit_ent_packet(i, map.entities.get(i).unwrap_or(&Entity::EMPTY)),
        })
        .collect();
    for chunk in packets.chunks(50) {
        server.broadcast(chunk.to_vec());
    }
}
