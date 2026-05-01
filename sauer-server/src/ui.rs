use cube_string::{Color, CubeString};
use sauer_proto::channel1::ServMsg;
use crate::Server;

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

pub fn send_banner(server: &Server, cn: u32, lines: &[CubeString]) {
    send(server, cn, sep());
    for l in lines {
        send(server, cn, l.clone());
    }
    send(server, cn, sep());
}

pub fn broadcast_banner(server: &Server, lines: &[CubeString]) {
    broadcast(server, sep());
    for l in lines {
        broadcast(server, l.clone());
    }
    broadcast(server, sep());
}