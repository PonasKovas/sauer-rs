use cube_string::{Color, CubeString};
use sauer_proto::channel1::ServMsg;
use sauer_server::Server;

// ─── Theme ────────────────────────────────────────────────────────────────────
// Tweak these colors to taste.

pub struct Theme {
    pub accent: Color, // headers, server name
    pub good: Color,   // good team
    pub evil: Color,   // evil player
    pub body: Color,   // normal body text
    pub hi: Color,     // inline keyword / value highlights
    pub tip: Color,    // command bullet markers
    pub warn: Color,   // errors and warnings
    pub sep: Color,    // separator lines and section dividers
    pub dim: Color,    // de-emphasised secondary text
}

pub const THEME: Theme = Theme {
    accent: Color::Orange,
    good: Color::Cyan,
    evil: Color::Red,
    body: Color::White,
    hi: Color::Yellow,
    tip: Color::Green,
    warn: Color::Yellow,
    sep: Color::Magenta,
    dim: Color::Gray,
};

// ─── String builders ──────────────────────────────────────────────────────────

const SEP_STR: &str = "================================";
const SEC_STR: &str = "  ---  ";

fn b() -> cube_string::CubeStringBuilder {
    CubeString::builder()
}

pub fn sep() -> CubeString {
    b().color(THEME.sep).push_str(SEP_STR).build()
}

pub fn header(text: &str) -> CubeString {
    b().color(THEME.sep)
        .push_str("# ")
        .color(THEME.accent)
        .push_str(text)
        .color(THEME.sep)
        .push_str(" #")
        .build()
}

pub fn section(text: &str) -> CubeString {
    b().color(THEME.sep)
        .push_str(SEC_STR)
        .color(THEME.dim)
        .push_str(text)
        .color(THEME.sep)
        .push_str(SEC_STR)
        .build()
}

pub fn body(text: &str) -> CubeString {
    b().color(THEME.body).push_str(text).build()
}

pub fn tip(cmd: &str, desc: &str) -> CubeString {
    b().color(THEME.tip)
        .push_str("  > ")
        .color(THEME.hi)
        .push_str(cmd)
        .color(THEME.dim)
        .push_str("   ")
        .color(THEME.body)
        .push_str(desc)
        .build()
}

pub fn warn_msg(text: &str) -> CubeString {
    b().color(THEME.warn)
        .push_str("! ")
        .color(THEME.body)
        .push_str(text)
        .build()
}

// ─── Send helpers ─────────────────────────────────────────────────────────────

pub fn send(server: &Server, cn: u32, text: CubeString) {
    server.send_chan1(cn, &ServMsg { text });
}

pub fn broadcast(server: &Server, text: CubeString) {
    server.broadcast_chan1(&ServMsg { text });
}

pub fn send_warn(server: &Server, cn: u32, text: &str) {
    send(server, cn, warn_msg(text));
}

pub fn broadcast_warn(server: &Server, text: &str) {
    broadcast(server, warn_msg(text));
}

fn send_banner(server: &Server, cn: u32, lines: &[CubeString]) {
    send(server, cn, sep());
    for l in lines {
        send(server, cn, l.clone());
    }
    send(server, cn, sep());
}

fn broadcast_banner(server: &Server, lines: &[CubeString]) {
    broadcast(server, sep());
    for l in lines {
        broadcast(server, l.clone());
    }
    broadcast(server, sep());
}

// ─── Game event messages ──────────────────────────────────────────────────────

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

pub fn broadcast_round_over(server: &Server, winner: &str) {
    broadcast_banner(
        server,
        &[
            header("ROUND OVER"),
            b().color(THEME.hi)
                .push_str("  Winner: ")
                .color(THEME.accent)
                .push_str(winner)
                .build(),
        ],
    );
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

pub fn send_welcome_edit(server: &Server, cn: u32) {
    send_banner(
        server,
        cn,
        &[
            header("DEATHRUN EDIT SERVER"),
            body("  Build and test Deathrun maps here."),
            section("Getting started"),
            body("  Just start editing, or"),
            tip(
                "/map",
                "select local map to start editing (/sendmap afterwards to start editing)",
            ),
            section("Trap commands  (aim at a wall first)"),
            tip("#newtrap", "create a trap at the aimed surface"),
            tip("#deltrap", "remove the trap at the aimed surface"),
            tip(
                "#settrap",
                "reset and enter edit mode for a trap (just make edits now)",
            ),
            tip("#edittrap", "enter edit mode for a trap without resetting"),
            tip("#endtrap", "finish editing the trap"),
            tip("#traptime <ms>", "extend duration  (default 2 000ms)"),
            tip("#traptimeout <ms>", "set trap timeout (default 20 000ms)"),
            section("Saving"),
            tip(
                "/savemap",
                "like a normal map. It must be edited on deathrun server for the traps to work.",
            ),
        ],
    );
}

pub fn send_welcome_play(server: &Server, cn: u32, map_name: Option<&str>, is_solo: bool) {
    let mut lines = vec![
        header("DEATHRUN"),
        b().color(THEME.evil)
            .push_str("  Terrorist")
            .color(THEME.body)
            .push_str(": controls traps, tries to stop the good team.")
            .build(),
        b().color(THEME.good)
            .push_str("  Good")
            .color(THEME.body)
            .push_str(": complete the map alive or kill the terrorist!")
            .build(),
    ];

    if let Some(name) = map_name {
        lines.push(
            b().color(THEME.dim)
                .push_str("  Map: ")
                .color(THEME.hi)
                .push_str(name)
                .build(),
        );
    }

    if is_solo {
        lines.push(
            b().color(THEME.dim)
                .push_str("  Practice mode - respawn freely until more players join.")
                .build(),
        );
    } else {
        lines.push(body("  One life per round. Don't die!"));
    }

    send_banner(server, cn, &lines);
}
