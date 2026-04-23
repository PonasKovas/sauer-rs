#!/usr/bin/env rust-script
//! ```cargo
//! [dependencies]
//! regex = "1"
//! ```
//!
//! Generates a concise Markdown reference for the sauer-proto packet protocol.
//! Usage: ./gen-proto-docs.rs [path/to/sauer-proto/src]
//! Defaults to ./sauer-proto/src relative to CWD.

use regex::Regex;
use std::collections::HashMap;
use std::fmt::Write as _;
use std::fs;
use std::path::Path;

// ---------------------------------------------------------------------------
// Data structures
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, Default)]
struct StructDef {
    name: String,
    tag: Option<String>,
    doc: String,
    fields: Vec<(String, String)>, // (field_name, type)
}

#[derive(Debug, Clone, Default)]
struct EnumDef {
    name: String,
    doc: String,
    repr: String,
    variants: Vec<(String, String, String)>, // (variant, value, doc)
}

#[derive(Debug, Clone)]
struct PacketEntry {
    tag: String,
    name: String,
    client_wrapped: bool,
    reliable: bool,
}

#[derive(Debug, Clone)]
struct PacketEnum {
    route: String,
    entries: Vec<PacketEntry>,
}

#[derive(Debug, Clone)]
struct RouteImpl {
    type_expr: String, // "Sound" or "Client<Sound>"
    route: String,
    reliable: bool,
}

// ---------------------------------------------------------------------------
// Parsing helpers
// ---------------------------------------------------------------------------

/// Extract the bodies (content between balanced parens) of all calls to `kw!(`.
fn extract_macro_bodies(src: &str, kw: &str) -> Vec<String> {
    let mut out = Vec::new();
    let pat = format!("{kw}(");
    let mut pos = 0;
    while pos < src.len() {
        let Some(rel) = src[pos..].find(&pat) else { break };
        let start = pos + rel + pat.len();
        let mut depth = 1i32;
        let mut i = start;
        let bytes = src.as_bytes();
        while i < bytes.len() {
            match bytes[i] {
                b'(' => depth += 1,
                b')' => {
                    depth -= 1;
                    if depth == 0 {
                        break;
                    }
                }
                _ => {}
            }
            i += 1;
        }
        out.push(src[start..i].to_string());
        pos = i + 1;
    }
    out
}

/// Split text into (leading doc comment, remainder).
fn extract_leading_doc(text: &str) -> (String, String) {
    let mut doc = Vec::new();
    let mut rest = Vec::new();
    let mut done = false;
    for line in text.lines() {
        if done {
            rest.push(line);
            continue;
        }
        let t = line.trim();
        if t.starts_with("///") {
            doc.push(t.trim_start_matches("///").trim().to_string());
        } else if t.is_empty() || t.starts_with("#[") {
            // skip derive/cfg attrs
        } else {
            done = true;
            rest.push(line);
        }
    }
    (doc.join(" "), rest.join("\n"))
}

// ---------------------------------------------------------------------------
// Packet enum parsing
// ---------------------------------------------------------------------------

fn parse_packet_enums(src: &str) -> Vec<PacketEnum> {
    let bodies = extract_macro_bodies(src, "make_packet_enum!");
    let route_re = Regex::new(r"\[([^\]]+)\]\s+\w+\s*\{").unwrap();
    // Match entries like: [N_TAG] ([RELIABLE false])? StructName ([N_CLIENT ...])? ([DUPLICATE ...])?
    let entry_re = Regex::new(
        r"\[(\w+)\](\s+\[RELIABLE\s+false\])?\s+(\w+)(\s+\[N_CLIENT\s+\w+\])?(?:\s+\[DUPLICATE\s+\w+\])?",
    )
    .unwrap();

    bodies
        .iter()
        .filter_map(|body| {
            let route = route_re.captures(body)?.get(1)?.as_str().to_string();
            let mut entries = Vec::new();
            for cap in entry_re.captures_iter(body) {
                entries.push(PacketEntry {
                    tag: cap[1].to_string(),
                    name: cap[3].to_string(),
                    client_wrapped: cap.get(4).is_some(),
                    reliable: cap.get(2).is_none(),
                });
            }
            Some(PacketEnum { route, entries })
        })
        .collect()
}

// ---------------------------------------------------------------------------
// make_struct! parsing
// ---------------------------------------------------------------------------

fn parse_make_structs(src: &str) -> Vec<StructDef> {
    let bodies = extract_macro_bodies(src, "make_struct!");
    let tag_re = Regex::new(r"^\[(\w+)\]\s+(\w+)\s*\{([\s\S]*)\}").unwrap();
    let notag_re = Regex::new(r"^(\w+)\s*\{([\s\S]*)\}").unwrap();
    // Field: word: type (stopping at => or , or end of content)
    let field_re = Regex::new(r"(?m)^\s+(\w+):\s*([\w\[\]<>, :;*&']+?)(?:\s*=>|\s*,|\s*$)").unwrap();

    bodies
        .iter()
        .filter_map(|body| {
            let (doc, rest) = extract_leading_doc(body);
            let rest = rest.trim().to_string();

            let (tag, name, fields_str) = if let Some(cap) = tag_re.captures(&rest) {
                (Some(cap[1].to_string()), cap[2].to_string(), cap[3].to_string())
            } else if let Some(cap) = notag_re.captures(&rest) {
                (None, cap[1].to_string(), cap[2].to_string())
            } else {
                return None;
            };

            let mut fields = Vec::new();
            for cap in field_re.captures_iter(&fields_str) {
                let fname = cap[1].trim().to_string();
                let ftype = cap[2].trim().trim_end_matches(',').trim().to_string();
                if !fname.is_empty() && fname != "fn" && fname != "pub" {
                    fields.push((fname, ftype));
                }
            }

            Some(StructDef { name, tag, doc, fields })
        })
        .collect()
}

// ---------------------------------------------------------------------------
// make_enum! parsing
// ---------------------------------------------------------------------------

fn parse_make_enums(src: &str) -> Vec<EnumDef> {
    let bodies = extract_macro_bodies(src, "make_enum!");
    let head_re = Regex::new(r"\[(\w+)\]\s+(\w+)\s*\{([\s\S]*)\}").unwrap();
    let variant_re = Regex::new(r"(\w+)\s*=\s*(-?\d+)").unwrap();

    bodies
        .iter()
        .filter_map(|body| {
            let (doc, rest) = extract_leading_doc(body);
            let rest = rest.trim().to_string();
            let cap = head_re.captures(&rest)?;
            let repr = cap[1].to_string();
            let name = cap[2].to_string();
            let variants_str = cap[3].to_string();

            let mut variants = Vec::new();
            let mut curr_doc = String::new();
            for line in variants_str.lines() {
                let t = line.trim();
                if t.starts_with("///") {
                    curr_doc = t.trim_start_matches("///").trim().to_string();
                } else {
                    for vcap in variant_re.captures_iter(t) {
                        let vname = vcap[1].to_string();
                        if vname == "default" {
                            continue; // skip #[default]
                        }
                        variants.push((vname, vcap[2].to_string(), curr_doc.clone()));
                        curr_doc = String::new();
                    }
                    if !t.is_empty() && !t.starts_with("//") && !t.starts_with("#") {
                        curr_doc = String::new();
                    }
                }
            }

            Some(EnumDef { name, doc, repr, variants })
        })
        .collect()
}

// ---------------------------------------------------------------------------
// Manual struct/enum parsing (pub struct/enum defined without macros)
// ---------------------------------------------------------------------------

fn parse_manual_structs(src: &str) -> Vec<StructDef> {
    // Grab outer doc, then `pub struct Name<...> {`
    let block_re = Regex::new(r"(?m)^pub struct (\w+)(?:<[^>]+>)?\s*\{([^}]*)\}").unwrap();
    let field_re = Regex::new(r"(?m)^\s+pub (\w+):\s*([\w\[\]<>, :;*&']+)").unwrap();
    // Collect per-field docs by scanning lines
    let _doc_before_re = Regex::new(r"///(.*)").unwrap();

    let mut results = Vec::new();

    for cap in block_re.captures_iter(src) {
        let name = cap[1].to_string();
        let body = &cap[2];

        // Extract doc comment that precedes this struct in the source
        let struct_pos = cap.get(0).unwrap().start();
        let prefix = &src[..struct_pos];
        let doc = extract_block_doc(prefix);

        let mut fields = Vec::new();
        let mut pending_doc = String::new();
        for line in body.lines() {
            let t = line.trim();
            if t.starts_with("///") {
                pending_doc = t.trim_start_matches("///").trim().to_string();
            } else if let Some(fc) = field_re.captures(line) {
                let ftype = fc[2].trim().trim_end_matches(|c: char| c == ',' || c == ';').trim().to_string();
                fields.push((fc[1].to_string(), ftype));
                pending_doc = String::new();
            } else if !t.is_empty() {
                pending_doc = String::new();
            }
        }

        results.push(StructDef { name, tag: None, doc, fields });
    }
    results
}

fn parse_manual_enums(src: &str) -> Vec<EnumDef> {
    let block_re = Regex::new(r"(?m)^pub enum (\w+)\s*\{([^}]*)\}").unwrap();
    let tuple_variant_re = Regex::new(r"^(\w+)\(([^)]+)\)").unwrap();
    let value_variant_re = Regex::new(r"^(\w+)\s*=\s*(-?\d+)").unwrap();
    let unit_variant_re  = Regex::new(r"^(\w+)\s*,?$").unwrap();
    // Extract repr type from attributes preceding the enum
    let repr_re = Regex::new(r"#\[repr\((\w+)\)\]").unwrap();

    let mut results = Vec::new();

    for cap in block_re.captures_iter(src) {
        let name = cap[1].to_string();
        let body = cap[2].to_string();
        let match_start = cap.get(0).unwrap().start();
        let prefix = &src[..match_start];
        let doc = extract_block_doc(prefix);

        // Grab repr from the ~5 lines before the enum
        let pre_lines: Vec<&str> = prefix.lines().rev().take(8).collect();
        let repr = pre_lines.iter()
            .find_map(|l| repr_re.captures(l).map(|c| c[1].to_string()))
            .unwrap_or_default();

        let mut variants = Vec::new();
        let mut pending_doc = String::new();
        for line in body.lines() {
            // Strip #[...] attribute prefixes
            let t = line.trim();
            let t = if t.starts_with("#[") {
                t.splitn(2, ']').nth(1).unwrap_or("").trim()
            } else { t };
            if t.starts_with("///") {
                pending_doc = t.trim_start_matches("///").trim().to_string();
            } else if let Some(vc) = tuple_variant_re.captures(t) {
                variants.push((vc[1].to_string(), vc[2].to_string(), pending_doc.clone()));
                pending_doc = String::new();
            } else if let Some(vc) = value_variant_re.captures(t) {
                variants.push((vc[1].to_string(), vc[2].to_string(), pending_doc.clone()));
                pending_doc = String::new();
            } else if let Some(vc) = unit_variant_re.captures(t) {
                let vname = vc[1].to_string();
                if !vname.is_empty() {
                    variants.push((vname, String::new(), pending_doc.clone()));
                    pending_doc = String::new();
                }
            } else if !t.is_empty() && !t.starts_with("//") {
                pending_doc = String::new();
            }
        }

        results.push(EnumDef { name, doc, repr, variants });
    }
    results
}

/// Extract the doc comment immediately preceding a position in source.
fn extract_block_doc(prefix: &str) -> String {
    let mut doc_lines: Vec<String> = Vec::new();
    for line in prefix.lines().rev() {
        let t = line.trim();
        if t.starts_with("///") {
            doc_lines.push(t.trim_start_matches("///").trim().to_string());
        } else if t.is_empty() || t.starts_with("#[") {
            continue;
        } else {
            break;
        }
    }
    doc_lines.reverse();
    doc_lines.join(" ")
}

// ---------------------------------------------------------------------------
// Routing map
// ---------------------------------------------------------------------------

fn build_routing_map(all_enums: &[PacketEnum]) -> HashMap<String, Vec<RouteImpl>> {
    let mut map: HashMap<String, Vec<RouteImpl>> = HashMap::new();
    for pe in all_enums {
        for entry in &pe.entries {
            let canonical = entry.name.clone();
            let (type_expr, route) = if entry.client_wrapped {
                (format!("Client<{}>", canonical), pe.route.clone())
            } else {
                (canonical.clone(), pe.route.clone())
            };
            map.entry(canonical).or_default().push(RouteImpl {
                type_expr,
                route,
                reliable: entry.reliable,
            });
        }
    }
    map
}

fn format_routes(impls: &[RouteImpl]) -> String {
    impls
        .iter()
        .map(|r| {
            let rel = if !r.reliable { " *(unreliable)*" } else { "" };
            format!("`{}` on `{}`{}", r.type_expr, r.route, rel)
        })
        .collect::<Vec<_>>()
        .join(" · ")
}

// ---------------------------------------------------------------------------
// Markdown generation
// ---------------------------------------------------------------------------

fn write_struct(out: &mut String, s: &StructDef, routing: &HashMap<String, Vec<RouteImpl>>) {
    let tag_str = s.tag.as_deref().map(|t| format!(" (`{t}`)")).unwrap_or_default();
    writeln!(out, "### `{}`{}", s.name, tag_str).unwrap();
    if !s.doc.is_empty() {
        writeln!(out, "{}", s.doc).unwrap();
    }
    if let Some(impls) = routing.get(&s.name) {
        writeln!(out, "  \n*Encode+Decode:* {}", format_routes(impls)).unwrap();
    }
    if s.fields.is_empty() {
        writeln!(out, "  \n*(no fields)*").unwrap();
    } else {
        writeln!(out).unwrap();
        for (fname, ftype) in &s.fields {
            writeln!(out, "- `{fname}: {ftype}`").unwrap();
        }
    }
    writeln!(out).unwrap();
}

fn write_enum(out: &mut String, e: &EnumDef) {
    let repr = if e.repr.is_empty() {
        String::new()
    } else {
        format!(" (repr `{}`)", e.repr)
    };
    writeln!(out, "### `{}`{}", e.name, repr).unwrap();
    if !e.doc.is_empty() {
        writeln!(out, "{}", e.doc).unwrap();
    }
    writeln!(out).unwrap();
    for (vname, val, vdoc) in &e.variants {
        let val_str = if val.is_empty() {
            String::new()
        } else {
            format!(" = `{val}`")
        };
        let doc_str = if vdoc.is_empty() {
            String::new()
        } else {
            format!(" — {vdoc}")
        };
        writeln!(out, "- `{vname}`{val_str}{doc_str}").unwrap();
    }
    writeln!(out).unwrap();
}

// ---------------------------------------------------------------------------
// Main
// ---------------------------------------------------------------------------

fn main() {
    let src_arg = std::env::args()
        .nth(1)
        .unwrap_or_else(|| "sauer-proto/src".to_string());
    let src = Path::new(&src_arg);

    macro_rules! read {
        ($p:expr) => {
            fs::read_to_string(src.join($p))
                .unwrap_or_else(|e| panic!("cannot read {}: {e}", $p))
        };
    }

    let chan0 = read!("messages/channel0.rs");
    let chan1 = read!("messages/channel1.rs");
    let chan2 = read!("messages/channel2.rs");
    let data  = read!("data.rs");

    // sub-module files are referenced in the hardcoded sections below; not parsed individually
    let _chan2_structs_unused = &chan2;

    // Build routing maps per channel
    let chan0_enums = parse_packet_enums(&chan0);
    let chan1_enums = parse_packet_enums(&chan1);
    let chan2_enums = parse_packet_enums(&chan2);
    let all_enums: Vec<PacketEnum> = chan0_enums
        .iter()
        .chain(chan1_enums.iter())
        .chain(chan2_enums.iter())
        .cloned()
        .collect();
    let routing = build_routing_map(&all_enums);

    // Parse structs per channel
    let chan0_structs = parse_make_structs(&chan0);
    let chan1_structs = parse_make_structs(&chan1);
    let _chan2_structs = parse_make_structs(&chan2);

    // Manual structs in channels
    let chan0_manual = parse_manual_structs(&chan0);
    let chan1_manual = parse_manual_structs(&chan1);

    // Data enums and structs
    let data_enums = parse_make_enums(&data);
    let data_structs = parse_make_structs(&data);
    let data_manual_enums = parse_manual_enums(&data);
    let data_manual_structs = parse_manual_structs(&data);

    // ------------------------------------------------------------------
    // Generate markdown
    // ------------------------------------------------------------------
    let mut out = String::new();

    writeln!(out, "# sauer-proto Protocol Reference\n").unwrap();
    writeln!(out, "> Auto-generated concise reference. Source: `sauer-proto/src/`.\n").unwrap();

    // ------------------------------------------------------------------
    // SECTION: Core API
    // ------------------------------------------------------------------
    writeln!(out, "## Core API\n").unwrap();

    writeln!(out, "### `Encode<R>` trait").unwrap();
    writeln!(out, "Serialize a packet onto a wire-format buffer for route `R`.  ").unwrap();
    writeln!(out, "Do **not** call directly — use `Writer::push` instead.  ").unwrap();
    writeln!(out, "Key methods: `encode(&self, out: &mut Vec<u8>)`, `is_reliable(&self) -> bool`,").unwrap();
    writeln!(out, "`requires_client_wrapper(&self) -> Option<i32>` (returns `Some(cn)` if packet must be wrapped in `N_CLIENT`).\n").unwrap();

    writeln!(out, "### `Decode<R>` trait").unwrap();
    writeln!(out, "Deserialize a packet from wire-format bytes for route `R`.  ").unwrap();
    writeln!(out, "Do **not** call directly — use `Reader::next` instead.  ").unwrap();
    writeln!(out, "Key method: `decode(reader: &mut &[u8], client_number: Option<i32>, session: &mut Session) -> Result<Self, ParseError>`.\n").unwrap();

    writeln!(out, "### `Reader<'a, R>`").unwrap();
    writeln!(out, "Stateful deserializer bound to protocol route `R`.  ").unwrap();
    writeln!(out, "Transparently handles `N_CLIENT` envelope unwrapping.  ").unwrap();
    writeln!(out, "- `Reader::new(slice: &[u8], session: &mut Session) -> Self` — must use a consistent `Session` per connection.").unwrap();
    writeln!(out, "- `next<T: Decode<R>>(&mut self) -> Option<Result<T, ParseError>>` — decode next message; returns `None` at end-of-buffer.").unwrap();
    writeln!(out, "- `is_empty(&self) -> bool`\n").unwrap();

    writeln!(out, "### `Writer<R>`").unwrap();
    writeln!(out, "Sink-based serializer bound to protocol route `R`.  ").unwrap();
    writeln!(out, "Transparently batches `N_CLIENT` envelopes.  ").unwrap();
    writeln!(out, "- `Writer::new() -> Self`").unwrap();
    writeln!(out, "- `push<T: Encode<R>>(&mut self, msg: &T)` — serialize and append a packet; no need to wrap in the channel enum.").unwrap();
    writeln!(out, "- `consume(self) -> (Vec<u8>, SendMode)` — finalize, returns raw bytes and reliability mode.\n").unwrap();

    writeln!(out, "### `Session`").unwrap();
    writeln!(out, "Per-connection parsing state. Keep one `Session` per connection for the entire lifetime.  ").unwrap();
    writeln!(out, "- `is_hold_mode: bool` — set automatically on `N_MAPCHANGE`; controls `N_INITFLAGS` parsing (M_HOLD gamemode includes extra hold-team data).\n").unwrap();

    writeln!(out, "### `SendMode`").unwrap();
    writeln!(out, "Returned by `Writer::consume`. Variants: `Reliable`, `Unreliable` (sequenced).  ").unwrap();
    writeln!(out, "If any packet pushed to a writer is reliable, the whole ENet packet is reliable.\n").unwrap();

    writeln!(out, "### `PacketSequence<R>` trait").unwrap();
    writeln!(out, "Lets slices, arrays, `Vec`, and tuples of packet structs be pushed to a `Writer<R>` via `push_to`.  ").unwrap();
    writeln!(out, "Blanket-implemented for homogeneous collections and heterogeneous tuples up to 16 elements.\n").unwrap();

    writeln!(out, "### `Client<T>`").unwrap();
    writeln!(out, "Wrapper produced when decoding server channel-1 packets that arrive inside an `N_CLIENT` envelope.  ").unwrap();
    writeln!(out, "- `client_num: i32` — the originating client.").unwrap();
    writeln!(out, "- `packet: T` — the inner packet.\n").unwrap();

    // ------------------------------------------------------------------
    // SECTION: Routes
    // ------------------------------------------------------------------
    writeln!(out, "## Route Markers (`sauer_proto::route`)\n").unwrap();
    writeln!(out, "The generic parameter on `Reader`/`Writer` and `Encode`/`Decode` encodes direction + channel.\n").unwrap();
    writeln!(out, "| Route | Channel | Direction |").unwrap();
    writeln!(out, "|-------|---------|-----------|").unwrap();
    writeln!(out, "| `S2C<Chan0>` | 0 — positions | Server → Client |").unwrap();
    writeln!(out, "| `C2S<Chan0>` | 0 — positions | Client → Server |").unwrap();
    writeln!(out, "| `S2C<Chan1>` | 1 — game messages | Server → Client |").unwrap();
    writeln!(out, "| `C2S<Chan1>` | 1 — game messages | Client → Server |").unwrap();
    writeln!(out, "| `S2C<Chan2>` | 2 — file transfer | Server → Client |").unwrap();
    writeln!(out, "\n`Chan0`, `Chan1`, `Chan2` are zero-sized channel markers. `S2C<C>` and `C2S<C>` are direction markers.\n").unwrap();

    // ------------------------------------------------------------------
    // SECTION: Data types
    // ------------------------------------------------------------------
    writeln!(out, "## Data Types (`sauer_proto::data`)\n").unwrap();
    writeln!(out, "Helper types used as fields in packet structs. Not packets themselves.\n").unwrap();

    writeln!(out, "### Enums\n").unwrap();
    for e in &data_enums {
        write_enum(&mut out, e);
    }
    // Manual enums in data.rs (Movement, Strafe)
    for e in &data_manual_enums {
        if matches!(e.name.as_str(), "Movement" | "Strafe") {
            write_enum(&mut out, e);
        }
    }

    writeln!(out, "### Structs\n").unwrap();
    for s in &data_structs {
        write_struct(&mut out, s, &routing);
    }
    // Manual structs from data.rs
    for s in &data_manual_structs {
        write_struct(&mut out, s, &routing);
    }

    // Data sub-module types (hardcoded since they're manually-written structs/enums)
    writeln!(out, "### `data::Vec3<T>`").unwrap();
    writeln!(out, "3D vector. Generic over element type (usually `f32` or `i32`).").unwrap();
    writeln!(out, "- `x: T`, `y: T`, `z: T`").unwrap();
    writeln!(out, "  \nFloating-point positions on the wire are quantized: multiplied by `DMF` (1/16 world units) before encoding.\n").unwrap();

    writeln!(out, "### `data::Direction`").unwrap();
    writeln!(out, "Packed yaw+pitch direction. Wire: 2 bytes, `yaw + biased_pitch * 360`.").unwrap();
    writeln!(out, "- `yaw: u16` — 0–359 degrees").unwrap();
    writeln!(out, "- `pitch: i8` — −90 to +90 degrees\n").unwrap();

    writeln!(out, "### `data::EditVarValue`").unwrap();
    writeln!(out, "Strict typing for `N_EDITVAR` variable values. Discriminant encoded as `i32` before name+value.").unwrap();
    writeln!(out, "- `Int(i32)` — discriminant 0").unwrap();
    writeln!(out, "- `Float(f32)` — discriminant 1").unwrap();
    writeln!(out, "- `String(CubeString)` — discriminant 2\n").unwrap();

    writeln!(out, "### `data::BaseInfo`").unwrap();
    writeln!(out, "State of a single capture base (used in `N_BASES` server packet).").unwrap();
    writeln!(out, "- `ammotype: Option<ItemType>` — `None` encodes as 0").unwrap();
    writeln!(out, "- `owner: CubeString`").unwrap();
    writeln!(out, "- `enemy: CubeString`").unwrap();
    writeln!(out, "- `converted: bool`").unwrap();
    writeln!(out, "- `ammo: i32`\n").unwrap();

    writeln!(out, "### `data::BaseSpawn`").unwrap();
    writeln!(out, "Spawn position of a capture base (used in `N_BASES` client packet).").unwrap();
    writeln!(out, "- `ammotype: Option<ItemType>`").unwrap();
    writeln!(out, "- `pos: Vec3<f32>`\n").unwrap();

    writeln!(out, "### `data::FlagState`").unwrap();
    writeln!(out, "Full state of a CTF flag, used in `N_INITFLAGS`. Complex conditional encoding.").unwrap();
    writeln!(out, "- `version: i32`").unwrap();
    writeln!(out, "- `spawn_index: i32`").unwrap();
    writeln!(out, "- `owner: FlagStateOwner`").unwrap();
    writeln!(out, "- `invis: bool`").unwrap();
    writeln!(out, "- `hold: Option<FlagStateHoldTeam>` — **must be `Some` only when `Session::is_hold_mode` is true**; stream corrupts otherwise.\n").unwrap();

    writeln!(out, "### `data::FlagStateOwner`").unwrap();
    writeln!(out, "- `Owned {{ owner: i32 }}` — encoded as `owner_id >= 0`").unwrap();
    writeln!(out, "- `Dropped {{ dropped: bool, drop_loc: Vec3<f32> }}` — encoded as `owner_id = -1`; `drop_loc` only present when `dropped = true`\n").unwrap();

    writeln!(out, "### `data::FlagStateHoldTeam`").unwrap();
    writeln!(out, "- `None` — encoded as team = -1").unwrap();
    writeln!(out, "- `Team {{ team: i32, hold_time: i32 }}`\n").unwrap();

    // ------------------------------------------------------------------
    // SECTION: Channel 0
    // ------------------------------------------------------------------
    writeln!(out, "---\n").unwrap();
    writeln!(out, "## Channel 0 Packets (`sauer_proto::channel0`)\n").unwrap();
    writeln!(out, "High-frequency physics / movement updates. Most packets are *unreliable* (sequenced).  ").unwrap();
    writeln!(out, "Enum types: `Chan0Server` for `S2C<Chan0>`, `Chan0Client` for `C2S<Chan0>`.\n").unwrap();

    // Merge make_struct + manual structs, deduplicate by name
    let mut seen = std::collections::HashSet::new();
    let mut chan0_all: Vec<&StructDef> = Vec::new();
    for s in chan0_structs.iter().chain(chan0_manual.iter()) {
        if seen.insert(s.name.clone()) {
            chan0_all.push(s);
        }
    }
    for s in &chan0_all {
        write_struct(&mut out, s, &routing);
    }

    // ------------------------------------------------------------------
    // SECTION: Channel 1
    // ------------------------------------------------------------------
    writeln!(out, "---\n").unwrap();
    writeln!(out, "## Channel 1 Packets (`sauer_proto::channel1`)\n").unwrap();
    writeln!(out, "Reliable game-state messages, chat, events (default reliable unless noted).  ").unwrap();
    writeln!(out, "Enum types: `Chan1Server` for `S2C<Chan1>`, `Chan1Client` for `C2S<Chan1>`.  ").unwrap();
    writeln!(out, "Packets with `Client<T>` routing arrive inside an `N_CLIENT` envelope on the server→client direction — the `client_num` field identifies the originating player.\n").unwrap();

    let mut seen = std::collections::HashSet::new();
    let mut chan1_all: Vec<&StructDef> = Vec::new();
    for s in chan1_structs.iter().chain(chan1_manual.iter()) {
        if seen.insert(s.name.clone()) {
            chan1_all.push(s);
        }
    }
    // Filter out Client<T> wrapper struct itself (covered in core API)
    for s in &chan1_all {
        if s.name == "Client" || s.name == "ClientWrapperState" {
            continue;
        }
        write_struct(&mut out, s, &routing);
    }

    // ------------------------------------------------------------------
    // SECTION: Channel 2
    // ------------------------------------------------------------------
    writeln!(out, "---\n").unwrap();
    writeln!(out, "## Channel 2 Packets (`sauer_proto::channel2`)\n").unwrap();
    writeln!(out, "Bulk file transfers (demo downloads, map sends). Server → Client only.  ").unwrap();
    writeln!(out, "Enum type: `Chan2Server` for `S2C<Chan2>`.  ").unwrap();
    writeln!(out, "Data field uses `read_all_remaining` — consumes all bytes to end of packet.\n").unwrap();

    let chan2_all = parse_make_structs(&chan2);
    for s in &chan2_all {
        write_struct(&mut out, s, &routing);
    }

    // ------------------------------------------------------------------
    // SECTION: ExtInfo (brief mention)
    // ------------------------------------------------------------------
    writeln!(out, "---\n").unwrap();
    writeln!(out, "## ExtInfo (`sauer_proto::extinfo`)\n").unwrap();
    writeln!(out, "Out-of-band UDP protocol for server browser queries. Not covered in detail here — see `sauer_proto::extinfo` module directly.\n").unwrap();

    // Write to stdout or a file
    let dest = std::env::args().nth(2).unwrap_or_else(|| "sauer-proto-ref.md".to_string());
    fs::write(&dest, &out).unwrap_or_else(|e| panic!("cannot write {dest}: {e}"));
    eprintln!("Written {} bytes to {dest}", out.len());
}
