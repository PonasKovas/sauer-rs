//! Writes `sauer-proto-ref.md`, a short Markdown reference for the sauer-proto
//! packet protocol.

use crate::rust_src::*;
use std::collections::HashMap;
use std::fmt::Write as _;
use std::fs;
use std::path::Path;

#[derive(Debug, Clone)]
struct RouteImpl {
    type_expr: String, // "Sound" or "Client<Sound>"
    route: String,
    reliable: bool,
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
    let tag_str = s
        .tag
        .as_deref()
        .map(|t| format!(" (`{t}`)"))
        .unwrap_or_default();
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
        for f in &s.fields {
            writeln!(out, "- `{}: {}`", f.name, f.ty).unwrap();
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

/// Reads `src` (sauer-proto/src) and writes the reference to `dest`.
pub fn run(src: &Path, dest: &Path) {
    macro_rules! read {
        ($p:expr) => {
            fs::read_to_string(src.join($p)).unwrap_or_else(|e| panic!("cannot read {}: {e}", $p))
        };
    }

    let chan0 = read!("messages/channel0.rs");
    let chan1 = read!("messages/channel1.rs");
    let chan2 = read!("messages/channel2.rs");
    let data = read!("data.rs");
    // Packets any channel can carry.
    let shared = read!("messages/mod.rs");

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
    writeln!(
        out,
        "> Auto-generated concise reference. Source: `sauer-proto/src/`.\n"
    )
    .unwrap();

    // ------------------------------------------------------------------
    // SECTION: Core API
    // ------------------------------------------------------------------
    writeln!(out, "## Core API\n").unwrap();

    writeln!(out, "### `Encode<R>` trait").unwrap();
    writeln!(
        out,
        "Serialize a packet onto a wire-format buffer for route `R`.  "
    )
    .unwrap();
    writeln!(
        out,
        "Do **not** call directly — use `Writer::push` instead.  "
    )
    .unwrap();
    writeln!(
        out,
        "Key methods: `encode(&self, out: &mut Vec<u8>)`, `is_reliable(&self) -> bool`,"
    )
    .unwrap();
    writeln!(out, "`requires_client_wrapper(&self) -> Option<Cn>` (returns `Some(cn)` if packet must be wrapped in `N_CLIENT`).\n").unwrap();

    writeln!(out, "### `Decode<R>` trait").unwrap();
    writeln!(
        out,
        "Deserialize a packet from wire-format bytes for route `R`.  "
    )
    .unwrap();
    writeln!(
        out,
        "Do **not** call directly — use `Reader::next` instead.  "
    )
    .unwrap();
    writeln!(out, "Key method: `decode(reader: &mut &[u8], client_number: Option<Cn>, session: &mut Session) -> Result<Self, ParseError>`.\n").unwrap();

    writeln!(out, "### `Reader<'a, R>`").unwrap();
    writeln!(out, "Stateful deserializer bound to protocol route `R`.  ").unwrap();
    writeln!(
        out,
        "Transparently handles `N_CLIENT` envelope unwrapping.  "
    )
    .unwrap();
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
    writeln!(
        out,
        "Per-connection parsing state. Keep one `Session` per connection for the entire lifetime.  "
    )
    .unwrap();
    writeln!(out, "- `is_hold_mode: bool` — set automatically on `N_MAPCHANGE`; controls `N_INITFLAGS` parsing (hold modes, `GameMode::is_hold`, add a hold team to every flag).\n").unwrap();

    writeln!(out, "### `SendMode`").unwrap();
    writeln!(
        out,
        "Returned by `Writer::consume`. Variants: `Reliable`, `Unreliable` (sequenced).  "
    )
    .unwrap();
    writeln!(
        out,
        "If any packet pushed to a writer is reliable, the whole ENet packet is reliable.\n"
    )
    .unwrap();

    writeln!(out, "### `PacketSequence<R>` trait").unwrap();
    writeln!(out, "Lets slices, arrays, `Vec`, and tuples of packet structs be pushed to a `Writer<R>` via `push_to`.  ").unwrap();
    writeln!(out, "Blanket-implemented for homogeneous collections and heterogeneous tuples up to 16 elements.\n").unwrap();

    writeln!(out, "### `Client<T>`").unwrap();
    writeln!(out, "Wrapper produced when decoding server channel-1 packets that arrive inside an `N_CLIENT` envelope.  ").unwrap();
    writeln!(out, "- `client_num: Cn` — the originating client.").unwrap();
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
    writeln!(
        out,
        "| `S2C<Chan1>` | 1 — game messages | Server → Client |"
    )
    .unwrap();
    writeln!(
        out,
        "| `C2S<Chan1>` | 1 — game messages | Client → Server |"
    )
    .unwrap();
    writeln!(
        out,
        "| `S2C<Chan2>` | 2 — file transfer | Server → Client |"
    )
    .unwrap();
    writeln!(out, "\n`Chan0`, `Chan1`, `Chan2` are zero-sized channel markers. `S2C<C>` and `C2S<C>` are direction markers.\n").unwrap();

    // ------------------------------------------------------------------
    // SECTION: Data types
    // ------------------------------------------------------------------
    writeln!(out, "## Data Types (`sauer_proto::data`)\n").unwrap();
    writeln!(
        out,
        "Helper types used as fields in packet structs. Not packets themselves.\n"
    )
    .unwrap();

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
    writeln!(
        out,
        "3D vector. Generic over element type (usually `f32` or `i32`)."
    )
    .unwrap();
    writeln!(out, "- `x: T`, `y: T`, `z: T`").unwrap();
    writeln!(out, "  \nFloating-point positions on the wire are quantized: multiplied by `DMF` (1/16 world units) before encoding.\n").unwrap();

    writeln!(out, "### `data::Cn`").unwrap();
    writeln!(out, "A client number (`Cn(pub u8)`), the engine's `clientnum`. An `int` on the wire; decoding fails outside 0 to 255 instead of wrapping.\n").unwrap();

    writeln!(out, "### `CubeString`").unwrap();
    writeln!(out, "From the `cube-string` crate: text in the engine's 8 bit character set. Wire: the engine's `sendstring`, one packed int per character, ended by 0.\n").unwrap();

    writeln!(out, "### `data::SelInfo`").unwrap();
    writeln!(
        out,
        "`sauer_core::Selection`, the engine's `selinfo`. Wire: every field in order."
    )
    .unwrap();
    writeln!(
        out,
        "- `o: Vec3<i32>`, `s: Vec3<i32>`, `grid: i32`, `orient: Orient`"
    )
    .unwrap();
    writeln!(
        out,
        "- `cx: i32`, `cxs: i32`, `cy: i32`, `cys: i32`, `corner: i32`\n"
    )
    .unwrap();

    writeln!(out, "### `data::EntityType` / `data::ItemType`").unwrap();
    writeln!(out, "From `sauer-core`. `EntityType` is the engine's entity type (`Light` = 1 up to `Flag` = 30), with `GameSpecific(i32)` for any other value. `ItemType` is the pickup subset, `Shells` = 8 to `Quad` = 18.\n").unwrap();

    writeln!(out, "### `Node`").unwrap();
    writeln!(out, "From `sauer-core`: a cube tree in edit blocks.").unwrap();
    writeln!(out, "- `Branch(Box<[Node; 8]>)`").unwrap();
    writeln!(
        out,
        "- `Leaf(Cube)`: `{{ edges: [u8; 12], textures: [u16; 6], material: Material }}`\n"
    )
    .unwrap();

    writeln!(out, "### `data::VSlotParam`").unwrap();
    writeln!(
        out,
        "A shader param of a vslot: `{{ name: CubeString, values: [f32; 4] }}`.\n"
    )
    .unwrap();

    writeln!(out, "### `data::VSlotData`").unwrap();
    writeln!(out, "A changed vslot an edit block uses: `{{ index: u16, slot: u16, vslot: VSlotProps }}`. `index` is in the sender's numbering, `slot` is the texture it is a variant of.\n").unwrap();

    writeln!(out, "### `data::Direction`").unwrap();
    writeln!(
        out,
        "Packed yaw+pitch direction. Wire: 2 bytes, `yaw + biased_pitch * 360`."
    )
    .unwrap();
    writeln!(out, "- `yaw: u16` — 0–359 degrees").unwrap();
    writeln!(out, "- `pitch: i8` — −90 to +90 degrees\n").unwrap();

    writeln!(out, "### `data::EditVarValue`").unwrap();
    writeln!(out, "Strict typing for `N_EDITVAR` variable values. Discriminant encoded as `i32` before name+value.").unwrap();
    writeln!(out, "- `Int(i32)` — discriminant 0").unwrap();
    writeln!(out, "- `Float(f32)` — discriminant 1").unwrap();
    writeln!(out, "- `String(CubeString)` — discriminant 2\n").unwrap();

    writeln!(out, "### `data::BaseInfo`").unwrap();
    writeln!(
        out,
        "State of a single capture base (used in `N_BASES` server packet)."
    )
    .unwrap();
    writeln!(out, "- `ammotype: Option<ItemType>` — `None` encodes as 0").unwrap();
    writeln!(out, "- `owner: CubeString`").unwrap();
    writeln!(out, "- `enemy: CubeString`").unwrap();
    writeln!(out, "- `converted: bool`").unwrap();
    writeln!(out, "- `ammo: i32`\n").unwrap();

    writeln!(out, "### `data::BaseSpawn`").unwrap();
    writeln!(
        out,
        "Spawn position of a capture base (used in `N_BASES` client packet)."
    )
    .unwrap();
    writeln!(out, "- `ammotype: Option<ItemType>`").unwrap();
    writeln!(out, "- `pos: Vec3<f32>`\n").unwrap();

    writeln!(out, "### `data::FlagState`").unwrap();
    writeln!(
        out,
        "Full state of a CTF flag, used in `N_INITFLAGS`. Complex conditional encoding."
    )
    .unwrap();
    writeln!(out, "- `version: i32`").unwrap();
    writeln!(out, "- `spawn_index: i32`").unwrap();
    writeln!(out, "- `location: FlagLocation`").unwrap();
    writeln!(out, "- `invis: bool`\n").unwrap();
    writeln!(out, "### `data::InitFlags`").unwrap();
    writeln!(
        out,
        "The flags of `N_INITFLAGS`. Must match `Session::is_hold_mode`."
    )
    .unwrap();
    writeln!(out, "- `Plain(Vec<FlagState>)`").unwrap();
    writeln!(
        out,
        "- `Hold(Vec<(FlagState, FlagStateHoldTeam)>)` — each flag is followed by its hold team\n"
    )
    .unwrap();
    writeln!(out, "### `data::FlagLocation`").unwrap();
    writeln!(out, "Wire: `owner, invis`, then `dropped` only when `owner < 0`, then the position only when `dropped`.").unwrap();
    writeln!(out, "- `Home` — owner -1, dropped 0").unwrap();
    writeln!(out, "- `Carried {{ owner: Cn }}` — owner >= 0").unwrap();
    writeln!(
        out,
        "- `Dropped {{ pos: Vec3<f32> }}` — owner -1, dropped 1, then `pos`\n"
    )
    .unwrap();

    writeln!(out, "### `data::FlagStateHoldTeam`").unwrap();
    writeln!(out, "- `None` — encoded as team = -1").unwrap();
    writeln!(out, "- `Team {{ team: i32, hold_time: i32 }}`\n").unwrap();

    writeln!(out, "### `data::VSlotProps`").unwrap();
    writeln!(out, "What a vslot changes on top of its texture slot (`sauer_core::VSlotProps`). Wire: the engine's `packvslot`, a type byte per set field, ended by `0xFF`.").unwrap();
    writeln!(
        out,
        "- `params: Vec<VSlotParam>` — type 0 each: name string, 4 floats"
    )
    .unwrap();
    writeln!(out, "- `scale: Option<f32>` (1), `rotation: Option<i32>` (2), `offset: Option<(i32, i32)>` (3), `scroll: Option<(f32, f32)>` (4)").unwrap();
    writeln!(out, "- `layer: Option<i32>` (5, uint), `alpha: Option<(f32, f32)>` (6), `color: Option<(f32, f32, f32)>` (7)\n").unwrap();

    writeln!(out, "### `data::TexRef`").unwrap();
    writeln!(out, "A texture in `N_EDITT` and `N_REPLACE`. The packet ends with a u16 length prefixed block that holds the vslot of each `Variant`, in field order.").unwrap();
    writeln!(out, "- `Index(u16)` — the texture value itself").unwrap();
    writeln!(
        out,
        "- `Variant {{ slot: u16, vslot: VSlotProps }}` — sent as `0x10000 + slot`\n"
    )
    .unwrap();

    writeln!(out, "### `data::ClipboardPayload`").unwrap();
    writeln!(out, "A copied block (`N_CLIPBOARD`). Wire: `unpacked_len, packed_len`, then zlib data holding the `block3` header (o, s, grid, orient as LE i32), the cubes, and `VSlotData` entries ended by 4 zero bytes. In `N_CLIPBOARD` it is `Option<ClipboardPayload>`: `None` (both lengths 0) clears the sender's clipboard.").unwrap();
    writeln!(
        out,
        "- `sel: SelInfo` — only `o`, `s`, `grid`, `orient` are sent"
    )
    .unwrap();
    writeln!(out, "- `nodes: Vec<Node>`").unwrap();
    writeln!(
        out,
        "- `vslots: Vec<VSlotData>` — `{{ index: u16, slot: u16, vslot: VSlotProps }}`\n"
    )
    .unwrap();

    writeln!(out, "### `data::UndoPayload`").unwrap();
    writeln!(out, "The block of `N_UNDO`/`N_REDO`, zlib packed like `ClipboardPayload`. Starts with a u16 entity count; 0 means cubes.").unwrap();
    writeln!(
        out,
        "- `Entities(Vec<UndoEntity>)` — `{{ index: u16, ent: Entity }}` each"
    )
    .unwrap();
    writeln!(
        out,
        "- `Cubes {{ sel, nodes, gridmap: Vec<u8>, vslots }}` — one gridmap byte per node\n"
    )
    .unwrap();

    // ------------------------------------------------------------------
    // SECTION: Shared packets
    // ------------------------------------------------------------------
    writeln!(out, "---\n").unwrap();
    writeln!(out, "## Any Channel (`sauer_proto::messages`)\n").unwrap();
    writeln!(
        out,
        "Packets every channel can carry. Each channel module re-exports them.\n"
    )
    .unwrap();
    for s in &parse_make_structs(&shared) {
        write_struct(&mut out, s, &routing);
    }

    // ------------------------------------------------------------------
    // SECTION: Channel 0
    // ------------------------------------------------------------------
    writeln!(out, "---\n").unwrap();
    writeln!(out, "## Channel 0 Packets (`sauer_proto::channel0`)\n").unwrap();
    writeln!(
        out,
        "High-frequency physics / movement updates. Most packets are *unreliable* (sequenced).  "
    )
    .unwrap();
    writeln!(
        out,
        "Enum types: `Chan0Server` for `S2C<Chan0>`, `Chan0Client` for `C2S<Chan0>`.\n"
    )
    .unwrap();

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
    writeln!(
        out,
        "Reliable game-state messages, chat, events (default reliable unless noted).  "
    )
    .unwrap();
    writeln!(
        out,
        "Enum types: `Chan1Server` for `S2C<Chan1>`, `Chan1Client` for `C2S<Chan1>`.  "
    )
    .unwrap();
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
    writeln!(
        out,
        "Bulk file transfers (demo downloads, map sends). Server → Client only.  "
    )
    .unwrap();
    writeln!(out, "Enum type: `Chan2Server` for `S2C<Chan2>`.  ").unwrap();
    writeln!(
        out,
        "Data field uses `read_all_remaining` — consumes all bytes to end of packet.\n"
    )
    .unwrap();

    let chan2_all = parse_make_structs(&chan2);
    for s in &chan2_all {
        write_struct(&mut out, s, &routing);
    }

    // ------------------------------------------------------------------
    // SECTION: ExtInfo (brief mention)
    // ------------------------------------------------------------------
    writeln!(out, "---\n").unwrap();
    writeln!(out, "## ExtInfo (`sauer_proto::server_query`)\n").unwrap();
    writeln!(out, "Out-of-band UDP protocol for server browser queries. Not covered in detail here — see the `sauer_proto::server_query` module.\n").unwrap();

    fs::write(dest, &out).unwrap_or_else(|e| panic!("cannot write {}: {e}", dest.display()));
    eprintln!("Written {} bytes to {}", out.len(), dest.display());
}
