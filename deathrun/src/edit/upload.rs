//! Getting a new map in: a vote names it, a player uploads it with /sendmap.

use crate::map_state::{DeathrunState, get_server_state, update_server_items};
use crate::trap::persist;
use crate::ui;
use bytes::Bytes;
use cube_string::IntoCubeStringLossy;
use sauer_map::Map;
use sauer_proto::{channel1, channel2, data::GameMode};
use sauer_server::{Flow, Server, Status};
use std::sync::Arc;
use tracing::{info, warn};

/// A map parsed off the main tasks, ready to replace the current one.
struct Parsed {
    state: DeathrunState,
    warnings: Vec<String>,
    /// The map as clients get it.
    file: Vec<u8>,
}

/// `/sendmap`: checks the player may upload, then parses the map in the
/// background and swaps it in.
pub(super) fn receive(server: &Arc<Server>, id: u64, data: &Bytes) -> Flow {
    let busy = get_server_state(server).read().mode.busy_reason();
    if let Some(why) = busy {
        ui::send_warn(server, id, &format!("Cannot upload a map while {why}."));
        return Flow::Cancel;
    }
    let is_spectator = server
        .players
        .with(id, |p| p.status == Status::Spectator)
        .unwrap_or(false);
    let is_master = server.get_privilege(id) >= sauer_proto::data::Privilege::Master;
    if is_spectator && !is_master {
        ui::send_warn(server, id, "You must be unspectated to upload a map.");
        return Flow::Cancel;
    }

    server.broadcast(&channel1::ServMsg {
        text: format!(
            "\x0C1[MAP] \x0C7Receiving map from \x0C5{}\x0C7 ({} KB). Processing...",
            ui::player_name(server, id),
            data.len() / 1024
        )
        .lossy_cubestring(),
    });

    let server = Arc::clone(server);
    let data = data.clone();
    tokio::spawn(async move {
        let size = data.len();
        match tokio::task::spawn_blocking(move || parse(&data)).await {
            Ok(Ok(parsed)) => install(&server, id, parsed, size),
            Ok(Err(e)) => warn!("id {id} sent an invalid map: {e}"),
            Err(e) => warn!("parsing the map from id {id} panicked: {e}"),
        }
    });
    Flow::Cancel
}

fn parse(data: &[u8]) -> Result<Parsed, String> {
    let start = std::time::Instant::now();
    let mut map = Map::from_bytes(data).map_err(|e| e.to_string())?;
    let parsed_in = start.elapsed();

    let loaded = persist::ingest(&mut map);
    let mut state = DeathrunState {
        map: map,
        protected: loaded.protected(),
        traps: loaded.traps,
        vault: loaded.vault,
        record_generation: loaded.generation,
        ..Default::default()
    };
    let mut warnings = loaded.warnings;
    // Entity links dropped while loading, or a vault left behind by an
    // offline /mapenlarge: write the fixed-up state back into the map.
    let mut sync = persist::Sync::default();
    let fixed = persist::relocate_if_needed(&mut state, &mut sync).and_then(|()| {
        if warnings.is_empty() && sync.is_empty() {
            Ok(())
        } else {
            persist::save(&mut state, &mut sync)
        }
    });
    if let Err(e) = fixed {
        warnings.push(format!("Could not update the trap vault: {e}"));
    }

    let file = state
        .map
        .to_ogz_bytes_merged(flate2::Compression::fast())
        .map_err(|e| e.to_string())?;
    info!(
        "Map loaded: [Parse: {parsed_in:?}] [Total: {:?}]",
        start.elapsed()
    );
    Ok(Parsed {
        state,
        warnings,
        file,
    })
}

/// Makes the parsed map the server's and sends it to everyone.
fn install(server: &Server, id: u64, parsed: Parsed, size: usize) {
    let Parsed {
        state,
        warnings,
        file,
    } = parsed;
    let traps = state.traps.len();
    let world_size = state.map.size;
    let name = {
        let mut s = get_server_state(server).write();
        // Someone started editing or placing while the map was parsed.
        if let Some(why) = s.mode.busy_reason() {
            drop(s);
            let msg = format!("Map upload dropped: {why}. Send it again later.");
            ui::send_warn(server, id, &msg);
            return;
        }
        {
            let map = &state.map;
            update_server_items(map, server);
        }
        let name = s
            .awaiting_upload
            .take()
            .unwrap_or_else(|| "untitled".lossy_cubestring());
        *s = state;
        name
    };
    info!("id {id} uploaded a map (world_size={world_size}); state loaded");
    {
        let mut game = server.game.write();
        game.map_name = name;
        game.mode = GameMode::CoopEdit;
    }

    server.broadcast(&channel1::ServMsg {
        text: format!(
            "\x0C1[MAP] \x0C7Loaded \x0C5{traps}\x0C7 traps ({} KB map).",
            size / 1024
        )
        .lossy_cubestring(),
    });
    for w in warnings {
        server.broadcast(&channel1::ServMsg {
            text: format!("\x0C6[TRAP]\x0C7 {w}").lossy_cubestring(),
        });
    }
    server.broadcast(&channel2::SendMap { data: file });
    ui::send(server, id, ui::body("Map successfully uploaded!"));
}

/// A map vote names the next map: everyone spectates until someone uploads it.
pub(super) fn vote(server: &Server, id: u64, packet: &channel1::MapVote) -> Flow {
    let busy = get_server_state(server).read().mode.busy_reason();
    if let Some(why) = busy {
        ui::send_warn(server, id, &format!("Cannot vote for a map while {why}."));
        return Flow::Cancel;
    }
    if packet.mode != GameMode::CoopEdit {
        return Flow::Cancel;
    }
    {
        let mut state = get_server_state(server).write();
        state.awaiting_upload = Some(packet.map_name.clone());
    }
    for client_id in server.joined_ids() {
        server.set_spectator(client_id, true);
    }
    server.send(
        id,
        &channel1::MapChange {
            map_name: packet.map_name.clone(),
            mode: packet.mode,
            has_items: false,
        },
    );
    let map = packet.map_name.strip_colors().to_utf8();
    ui::broadcast_map_changed(server, &ui::player_name(server, id), &map);
    Flow::Cancel
}
