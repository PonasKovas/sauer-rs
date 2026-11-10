use cube_string::CubeString;
use sauer_server::Server;
pub use sauer_server::ui::*;

fn b() -> cube_string::CubeStringBuilder {
    CubeString::builder()
}

// ─── Game event messages ──────────────────────────────────────────────────────

pub fn broadcast_trap_edit_started(server: &Server, trap_idx: usize) {
    broadcast_banner(
        server,
        &[
            header(&format!("TRAP {} EDIT MODE", trap_idx)),
            body("  You are now editing this trap's active state."),
            body("  Any geometry, texture, and entity changes you make"),
            body("  within the bounds of your edits will be recorded."),
            tip("#endtrap", "save your changes and return to normal mode"),
        ],
    );
}

pub fn broadcast_round_start(server: &Server, evil_name: Option<&str>) {
    if let Some(name) = evil_name {
        broadcast_banner(
            server,
            &[
                header("ROUND STARTED"),
                b().color(THEME.evil)
                    .push_str("  Terrorist: ")
                    .color(THEME.hi)
                    .push_str(name)
                    .build(),
                b().color(THEME.good)
                    .push_str("  Good team")
                    .color(THEME.body)
                    .push_str(": reach the end alive!")
                    .build(),
            ],
        );
    } else {
        broadcast_banner(
            server,
            &[
                header("PRACTICE MODE"),
                body("  Respawn freely - no terrorist yet."),
                b().color(THEME.dim)
                    .push_str("  Waiting for more players...")
                    .build(),
            ],
        );
    }
}

pub fn broadcast_round_over(server: &Server, winner: &str, top_times: &[(String, f32)]) {
    let mut lines = vec![
        header("ROUND OVER"),
        b().color(THEME.hi)
            .push_str("  Winner: ")
            .color(THEME.accent)
            .push_str(winner)
            .build(),
    ];

    if !top_times.is_empty() {
        lines.push(section("Top Times"));
        for (i, (name, time)) in top_times.iter().enumerate() {
            lines.push(
                b().color(THEME.dim)
                    .push_str(&format!("    {}. ", i + 1))
                    .color(THEME.good)
                    .push_str(name)
                    .color(THEME.dim)
                    .push_str(" - ")
                    .color(THEME.hi)
                    .push_str(&format!("{:.2}s", time))
                    .build(),
            );
        }
    }

    broadcast_banner(server, &lines);
}

pub fn broadcast_map_changed(server: &Server, player_name: &str, map_name: &str) {
    broadcast_banner(
        server,
        &[
            header("MAP CHANGED"),
            b().color(THEME.hi)
                .push_str(player_name)
                .color(THEME.body)
                .push_str(" selected map ")
                .color(THEME.hi)
                .push_str(map_name)
                .build(),
            body("  Everyone is spectating until the map is uploaded."),
            b().color(THEME.body)
                .push_str("  ")
                .color(THEME.hi)
                .push_str(player_name)
                .color(THEME.body)
                .push_str(": type ")
                .color(THEME.hi)
                .push_str("/sendmap")
                .color(THEME.body)
                .push_str(" to upload and let everyone edit.")
                .build(),
        ],
    );
}

// ─── Welcome messages ─────────────────────────────────────────────────────────

/// The host's notes (see `ServerNotes`) as wall lines, indented like the rest.
pub fn note_lines(notes: &[CubeString]) -> impl Iterator<Item = CubeString> + '_ {
    notes.iter().map(|n| {
        let mut line = b().color(THEME.body).push_str("  ").build();
        line.extend(n.as_bytes().iter().copied());
        line
    })
}

fn server_notes(server: &Server) -> Vec<CubeString> {
    server
        .extra
        .get::<crate::ServerNotes>()
        .map(|n| note_lines(&n.0).collect())
        .unwrap_or_default()
}

pub fn send_welcome_edit(server: &Server, id: u64) {
    let mut lines = vec![
        // Newest lines end up at the bottom of the console, so the most
        // important info goes last.
        section("Saving"),
        tip(
            "/savemap",
            "like a normal map. It must be edited on deathrun server for the traps to work.",
        ),
        body(""),
        b().color(THEME.evil)
            .push_str("  WARNING: DO NOT /savemap while ANY trap is active!")
            .build(),
        b().color(THEME.dim)
            .push_str("  Wait for all triggered traps to finish and revert first,")
            .build(),
        b().color(THEME.dim)
            .push_str("  otherwise the map will be permanently broken.")
            .build(),
        section("Trap commands  (aim at a wall first)"),
        tip("#help", "list all server commands"),
        tip("#traptimeout <ms>", "set trap timeout (default 20 000ms)"),
        tip("#traptime <ms>", "extend duration  (default 2 000ms)"),
        tip("#trapinfo", "print info about the trap (size, overhead)"),
        tip("#endtrap", "finish editing the trap"),
        tip("#edittrap", "enter edit mode for a trap without resetting"),
        tip(
            "#settrap",
            "reset and enter edit mode for a trap (just make edits now)",
        ),
        tip("#deltrap", "remove the trap at the aimed surface"),
        tip("#newtrap", "create a trap at the aimed surface"),
        section("Getting started"),
        body("  Just start editing, or"),
        tip(
            "/map",
            "select local map to start editing (/sendmap afterwards to start editing)",
        ),
    ];
    lines.extend(server_notes(server));
    lines.push(body("  Build and test Deathrun maps here."));
    lines.push(header("DEATHRUN EDIT SERVER"));
    send_banner(server, id, &lines);
}

pub fn send_welcome_play(server: &Server, id: u64, map_name: Option<&str>, is_solo: bool) {
    // Most important info last, it stays visible the longest in the console.
    let mut lines = vec![if is_solo {
        b().color(THEME.dim)
            .push_str("  Practice mode - respawn freely until more players join.")
            .build()
    } else {
        body("  One life per round. Don't die!")
    }];

    if let Some(name) = map_name {
        lines.push(
            b().color(THEME.dim)
                .push_str("  Map: ")
                .color(THEME.hi)
                .push_str(name)
                .build(),
        );
    }

    lines.push(
        b().color(THEME.good)
            .push_str("  Good")
            .color(THEME.body)
            .push_str(": complete the map alive or kill the terrorist!")
            .build(),
    );
    lines.push(
        b().color(THEME.evil)
            .push_str("  Terrorist")
            .color(THEME.body)
            .push_str(": controls traps, tries to stop the good team.")
            .build(),
    );
    lines.extend(server_notes(server));
    lines.push(header("DEATHRUN"));

    send_banner(server, id, &lines);
}
