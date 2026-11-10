#!/usr/bin/env rust-script
//! ```cargo
//! [dependencies]
//! regex = "1"
//! ```
//!
//! Generates HTML API docs and EmmyLua LSP stubs for the sauer Lua plugin API.
//! Usage: ./gen-lua-docs.rs [path/to/sauer-plugin-lua/src]
//! Outputs: lua-api/index.html  and  lua-api/sauer.lua

use regex::Regex;
use std::fmt::Write as _;
use std::fs;
use std::path::Path;

// ---------------------------------------------------------------------------
// Data model
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, PartialEq)]
enum EntryKind {
    Function,  // global function
    Const,     // global constant
    Method,    // :method() on a type
    Field,     // .field on a type  (read/write)
    Callback,  // plugin callback handler
}

#[derive(Debug, Clone)]
struct Entry {
    name: String,
    kind: EntryKind,
    doc: String,
    /// Human-readable Lua signature, e.g. `disconnect(id: number)`
    sig: String,
    /// Extra badge text shown in the UI, e.g. "packet", "cancellable"
    tags: Vec<&'static str>,
}

#[derive(Debug)]
struct Section {
    id: &'static str,
    title: &'static str,
    desc: &'static str,
    entries: Vec<Entry>,
}

// ---------------------------------------------------------------------------
// Parsers per source file
// ---------------------------------------------------------------------------

fn parse_globals(src: &str) -> Vec<Entry> {
    let log_re = Regex::new(r#"env\.set\("(print|trace|debug|info|warn|error)"#).unwrap();
    let const_re = Regex::new(r#"env\.set\("([A-Z_]+)"#).unwrap();
    let ui_re = Regex::new(r#"ui_table\.set\(\s*"([^"]+)""#).unwrap();

    let mut entries = Vec::new();
    let mut pending_doc: Vec<String> = Vec::new();

    for line in src.lines() {
        let trimmed = line.trim();
        if let Some(rest) = trimmed.strip_prefix("///") {
            pending_doc.push(rest.trim().to_string());
            continue;
        }

        let doc = pending_doc.join(" ");

        if let Some(caps) = log_re.captures(trimmed) {
            let name = caps[1].to_string();
            entries.push(Entry {
                sig: format!("{}(...)", name),
                kind: EntryKind::Function,
                doc,
                tags: vec![],
                name,
            });
            pending_doc.clear();
        } else if let Some(caps) = const_re.captures(trimmed) {
            let name = caps[1].to_string();
            entries.push(Entry {
                sig: name.clone(),
                kind: EntryKind::Const,
                doc,
                tags: vec![],
                name,
            });
            pending_doc.clear();
        } else if let Some(caps) = ui_re.captures(trimmed) {
            let name = caps[1].to_string();
            let sig = match name.as_str() {
                "header" | "body" | "warn_msg" | "section" => format!("ui.{}(text: string)", name),
                "tip" => "ui.tip(cmd: string, desc: string)".to_string(),
                "sep" => "ui.sep()".to_string(),
                _ => format!("ui.{}(...)", name),
            };
            entries.push(Entry {
                name: format!("ui.{}", name),
                kind: EntryKind::Function,
                doc,
                sig,
                tags: vec![],
            });
            pending_doc.clear();
        } else if !trimmed.is_empty() && !trimmed.starts_with("//") {
            pending_doc.clear();
        }
    }
    entries
}

fn parse_server_methods(src: &str) -> Vec<Entry> {
    let method_re = Regex::new(r#"methods\.add_method(?:_mut)?\(\s*"([^"]+)""#).unwrap();
    let field_re  = Regex::new(r#"fields\.add_field_method_get\(\s*"([^"]+)""#).unwrap();
    let send_re   = Regex::new(r#"add_send!\("([^"]+)",\s*([\w:<>]+)"#).unwrap();
    let bcast_re  = Regex::new(r#"add_broadcast!\("([^"]+)",\s*([\w:<>]+)"#).unwrap();

    let mut entries = Vec::new();
    let mut pending_doc: Vec<String> = Vec::new();

    for line in src.lines() {
        let trimmed = line.trim();

        if let Some(rest) = trimmed.strip_prefix("///") {
            pending_doc.push(rest.trim().to_string());
            continue;
        }

        let doc = pending_doc.join(" ");

        if let Some(caps) = method_re.captures(trimmed) {
            let name = caps[1].to_string();
            let sig = hand_written_sig(&name);
            let tags = hand_written_tags(&name);
            entries.push(Entry { name, kind: EntryKind::Method, doc, sig, tags });
            pending_doc.clear();
        } else if let Some(caps) = field_re.captures(trimmed) {
            let name = caps[1].to_string();
            entries.push(Entry {
                sig: format!("server.{}", name),
                kind: EntryKind::Field,
                doc,
                tags: vec![],
                name,
            });
            pending_doc.clear();
        } else if let Some(caps) = send_re.captures(trimmed) {
            let name = caps[1].to_string();
            let type_name = packet_display_name(&caps[2]);
            let auto_doc = format!("Send a `{}` packet to one client by peer ID.", type_name);
            entries.push(Entry {
                sig: format!("server:{}(id: number, packet: table)", name),
                kind: EntryKind::Method,
                doc: if doc.is_empty() { auto_doc } else { doc },
                tags: vec!["packet"],
                name,
            });
            pending_doc.clear();
        } else if let Some(caps) = bcast_re.captures(trimmed) {
            let name = caps[1].to_string();
            let type_name = packet_display_name(&caps[2]);
            let auto_doc = format!("Broadcast a `{}` packet to all connected clients.", type_name);
            entries.push(Entry {
                sig: format!("server:{}(packet: table)", name),
                kind: EntryKind::Method,
                doc: if doc.is_empty() { auto_doc } else { doc },
                tags: vec!["packet"],
                name,
            });
            pending_doc.clear();
        } else if !trimmed.is_empty() && !trimmed.starts_with("//") {
            pending_doc.clear();
        }
    }
    entries
}

fn parse_spawn_state_fields(src: &str) -> Vec<Entry> {
    let get_re = Regex::new(r#"fields\.add_field_method_get\(\s*"([^"]+)""#).unwrap();
    let set_re = Regex::new(r#"fields\.add_field_method_set\(\s*"([^"]+)""#).unwrap();
    let mut entries = Vec::new();
    let mut pending_doc: Vec<String> = Vec::new();
    let mut seen_setters: std::collections::HashSet<String> = Default::default();

    for line in src.lines() {
        let trimmed = line.trim();
        if let Some(rest) = trimmed.strip_prefix("///") {
            pending_doc.push(rest.trim().to_string());
            continue;
        }
        if set_re.captures(trimmed).is_some() {
            pending_doc.clear();
            continue;
        }
        if let Some(caps) = get_re.captures(trimmed) {
            let name = caps[1].to_string();
            if seen_setters.insert(name.clone()) {
                entries.push(Entry {
                    sig: format!("spawn_state.{}: number", name),
                    kind: EntryKind::Field,
                    doc: pending_doc.join(" "),
                    tags: vec![],
                    name,
                });
            }
            pending_doc.clear();
        } else if !trimmed.is_empty() && !trimmed.starts_with("//") {
            pending_doc.clear();
        }
    }
    entries
}

fn parse_simple_methods(src: &str, type_sig_prefix: &str) -> Vec<Entry> {
    let method_re = Regex::new(r#"methods\.add_method(?:_mut)?\(\s*"([^"]+)""#).unwrap();
    let mut entries = Vec::new();
    let mut pending_doc: Vec<String> = Vec::new();

    for line in src.lines() {
        let trimmed = line.trim();
        if let Some(rest) = trimmed.strip_prefix("///") {
            pending_doc.push(rest.trim().to_string());
            continue;
        }
        if let Some(caps) = method_re.captures(trimmed) {
            let name = caps[1].to_string();
            entries.push(Entry {
                sig: format!("{}:{}()", type_sig_prefix, name),
                kind: EntryKind::Method,
                doc: pending_doc.join(" "),
                tags: vec![],
                name,
            });
            pending_doc.clear();
        } else if !trimmed.is_empty() && !trimmed.starts_with("//") {
            pending_doc.clear();
        }
    }
    entries
}

// ---------------------------------------------------------------------------
// Hardcoded callback table (signatures come from plugin.rs)
// ---------------------------------------------------------------------------

struct CallbackDef {
    name: &'static str,
    sig: &'static str,
    doc: &'static str,
    cancellable: bool,
}

fn callbacks() -> Vec<Entry> {
    const DEFS: &[CallbackDef] = &[
        CallbackDef { name: "start",              sig: "start()",                                          doc: "Called once when the server starts and the `server` global is ready.",                         cancellable: false },
        CallbackDef { name: "connected",          sig: "connected(id: number, event: Event)",              doc: "Called when a new peer connects (before authentication).",                                    cancellable: true  },
        CallbackDef { name: "disconnected",       sig: "disconnected(id: number, event: Event)",           doc: "Called when a peer disconnects.",                                                            cancellable: true  },
        CallbackDef { name: "on_client_welcomed", sig: "on_client_welcomed(id: number, event: Event)",     doc: "Called after a client completes the welcome handshake and is fully joined.",                  cancellable: true  },
        CallbackDef { name: "map_changed",        sig: "map_changed(id: number, map_name: string, mode: number, event: Event)", doc: "Called when a client requests a map change.",                          cancellable: true  },
        CallbackDef { name: "on_spawn",           sig: "on_spawn(id: number, state: SpawnState, event: Event)", doc: "Called when a player spawns. Modify `state` fields to override spawn attributes.",    cancellable: true  },
        CallbackDef { name: "on_chat",            sig: "on_chat(id: number, text: string, event: Event)",  doc: "Called for each chat message. Cancel to suppress it.",                                       cancellable: true  },
        CallbackDef { name: "on_command",         sig: "on_command(id: number, args: table, event: Event)", doc: "Called when a client sends a registered server command (see `register_command`). `args[1]` is the command name.", cancellable: true  },
        CallbackDef { name: "map",                sig: "map(id: number, data: string, event: Event)",      doc: "Called when a client uploads map data (binary string).",                                     cancellable: true  },
        CallbackDef { name: "server_query",       sig: "server_query(addr: string, request: table, event: Event)", doc: "Called for each ExtInfo/server-browser UDP query. `request.type` identifies the query kind.", cancellable: true },
        // Channel 0
        CallbackDef { name: "pos",           sig: "pos(id: number, packet: Position, event: Event)",      doc: "Client position update.",                         cancellable: true },
        CallbackDef { name: "teleport",      sig: "teleport(id: number, packet: Teleport, event: Event)", doc: "Client teleport.",                                cancellable: true },
        CallbackDef { name: "jumppad",       sig: "jumppad(id: number, packet: JumpPad, event: Event)",   doc: "Client jumppad use.",                             cancellable: true },
        // Channel 1
        CallbackDef { name: "connect",           sig: "connect(id, packet, event)",          doc: "Initial connection packet from client.",            cancellable: true },
        CallbackDef { name: "auth_answer",       sig: "auth_answer(id, packet, event)",      doc: "Client authentication answer.",                    cancellable: true },
        CallbackDef { name: "ping",              sig: "ping(id, packet, event)",             doc: "Client ping packet.",                              cancellable: true },
        CallbackDef { name: "from_ai",           sig: "from_ai(id, packet, event)",          doc: "Packet forwarded from a bot.",                     cancellable: true },
        CallbackDef { name: "sound",             sig: "sound(id, packet, event)",            doc: "Client plays a sound.",                            cancellable: true },
        CallbackDef { name: "edit_mode",         sig: "edit_mode(id, packet, event)",        doc: "Client toggles edit mode.",                        cancellable: true },
        CallbackDef { name: "map_crc",           sig: "map_crc(id, packet, event)",          doc: "Client sends map CRC.",                            cancellable: true },
        CallbackDef { name: "check_maps",        sig: "check_maps(id, packet, event)",       doc: "Client requests map check.",                       cancellable: true },
        CallbackDef { name: "try_spawn",         sig: "try_spawn(id, packet, event)",        doc: "Client requests to spawn.",                        cancellable: true },
        CallbackDef { name: "gun_select",        sig: "gun_select(id, packet, event)",       doc: "Client switches weapon.",                          cancellable: true },
        CallbackDef { name: "spawn_client",      sig: "spawn_client(id, packet, event)",     doc: "Client confirms spawn.",                           cancellable: true },
        CallbackDef { name: "suicide",           sig: "suicide(id, packet, event)",          doc: "Client commits suicide.",                          cancellable: true },
        CallbackDef { name: "shoot",             sig: "shoot(id, packet, event)",            doc: "Client fires a shot.",                             cancellable: true },
        CallbackDef { name: "explode",           sig: "explode(id, packet, event)",          doc: "Client detonates an explosive.",                   cancellable: true },
        CallbackDef { name: "item_pickup",       sig: "item_pickup(id, packet, event)",      doc: "Client picks up an item.",                         cancellable: true },
        CallbackDef { name: "text",              sig: "text(id, packet, event)",             doc: "Client sends global chat text.",                   cancellable: true },
        CallbackDef { name: "say_team",          sig: "say_team(id, packet, event)",         doc: "Client sends team chat.",                          cancellable: true },
        CallbackDef { name: "switch_name",       sig: "switch_name(id, packet, event)",      doc: "Client changes their name.",                       cancellable: true },
        CallbackDef { name: "switch_model",      sig: "switch_model(id, packet, event)",     doc: "Client changes their player model.",               cancellable: true },
        CallbackDef { name: "switch_team",       sig: "switch_team(id, packet, event)",      doc: "Client requests a team change.",                   cancellable: true },
        CallbackDef { name: "map_vote",          sig: "map_vote(id, packet, event)",         doc: "Client votes for a map.",                          cancellable: true },
        CallbackDef { name: "item_list",         sig: "item_list(id, packet, event)",        doc: "Client sends item list.",                          cancellable: true },
        CallbackDef { name: "edit_ent",          sig: "edit_ent(id, packet, event)",         doc: "Client edits an entity.",                          cancellable: true },
        CallbackDef { name: "edit_var",          sig: "edit_var(id, packet, event)",         doc: "Client edits a map variable.",                     cancellable: true },
        CallbackDef { name: "client_ping",       sig: "client_ping(id, packet, event)",      doc: "Client ping acknowledgement.",                     cancellable: true },
        CallbackDef { name: "master_mode",       sig: "master_mode(id, packet, event)",      doc: "Client requests master mode change.",              cancellable: true },
        CallbackDef { name: "clear_bans",        sig: "clear_bans(id, packet, event)",       doc: "Client requests ban list clear.",                  cancellable: true },
        CallbackDef { name: "kick",              sig: "kick(id, packet, event)",             doc: "Client requests to kick another player.",          cancellable: true },
        CallbackDef { name: "spectator",         sig: "spectator(id, packet, event)",        doc: "Client requests spectator mode.",                  cancellable: true },
        CallbackDef { name: "set_team",          sig: "set_team(id, packet, event)",         doc: "Client requests to set another player's team.",    cancellable: true },
        CallbackDef { name: "force_intermission",sig: "force_intermission(id, packet, event)", doc: "Client forces intermission.",                    cancellable: true },
        CallbackDef { name: "record_demo",       sig: "record_demo(id, packet, event)",      doc: "Client requests demo recording.",                  cancellable: true },
        CallbackDef { name: "stop_demo",         sig: "stop_demo(id, packet, event)",        doc: "Client stops demo recording.",                     cancellable: true },
        CallbackDef { name: "clear_demos",       sig: "clear_demos(id, packet, event)",      doc: "Client clears demo list.",                         cancellable: true },
        CallbackDef { name: "list_demos",        sig: "list_demos(id, packet, event)",       doc: "Client requests demo list.",                       cancellable: true },
        CallbackDef { name: "get_demo",          sig: "get_demo(id, packet, event)",         doc: "Client requests a demo download.",                 cancellable: true },
        CallbackDef { name: "get_map",           sig: "get_map(id, packet, event)",          doc: "Client requests the current map.",                 cancellable: true },
        CallbackDef { name: "new_map",           sig: "new_map(id, packet, event)",          doc: "Client creates a new map.",                        cancellable: true },
        CallbackDef { name: "set_master",        sig: "set_master(id, packet, event)",       doc: "Client requests master status for another.",       cancellable: true },
        CallbackDef { name: "add_bot",           sig: "add_bot(id, packet, event)",          doc: "Client requests adding a bot.",                    cancellable: true },
        CallbackDef { name: "del_bot",           sig: "del_bot(id, packet, event)",          doc: "Client requests removing a bot.",                  cancellable: true },
        CallbackDef { name: "bot_limit",         sig: "bot_limit(id, packet, event)",        doc: "Client sets the bot limit.",                       cancellable: true },
        CallbackDef { name: "bot_balance",       sig: "bot_balance(id, packet, event)",      doc: "Client toggles bot balance.",                      cancellable: true },
        CallbackDef { name: "auth_try",          sig: "auth_try(id, packet, event)",         doc: "Client initiates authentication.",                 cancellable: true },
        CallbackDef { name: "auth_kick",         sig: "auth_kick(id, packet, event)",        doc: "Client requests an auth-kick.",                    cancellable: true },
        CallbackDef { name: "pause_game",        sig: "pause_game(id, packet, event)",       doc: "Client requests game pause.",                      cancellable: true },
        CallbackDef { name: "game_speed",        sig: "game_speed(id, packet, event)",       doc: "Client requests game speed change.",               cancellable: true },
        CallbackDef { name: "copy",              sig: "copy(id, packet, event)",             doc: "Client copies geometry selection.",                cancellable: true },
        CallbackDef { name: "paste",             sig: "paste(id, packet, event)",            doc: "Client pastes geometry.",                          cancellable: true },
        CallbackDef { name: "clipboard",         sig: "clipboard(id, packet, event)",        doc: "Client sends clipboard data.",                     cancellable: true },
        CallbackDef { name: "undo",              sig: "undo(id, packet, event)",             doc: "Client undoes an edit.",                           cancellable: true },
        CallbackDef { name: "redo",              sig: "redo(id, packet, event)",             doc: "Client redoes an edit.",                           cancellable: true },
        CallbackDef { name: "edit_face",         sig: "edit_face(id, packet, event)",        doc: "Client edits a cube face.",                        cancellable: true },
        CallbackDef { name: "edit_tex",          sig: "edit_tex(id, packet, event)",         doc: "Client edits a texture.",                          cancellable: true },
        CallbackDef { name: "edit_mat",          sig: "edit_mat(id, packet, event)",         doc: "Client edits a material.",                         cancellable: true },
        CallbackDef { name: "flip",              sig: "flip(id, packet, event)",             doc: "Client flips geometry.",                           cancellable: true },
        CallbackDef { name: "rotate",            sig: "rotate(id, packet, event)",           doc: "Client rotates geometry.",                         cancellable: true },
        CallbackDef { name: "replace",           sig: "replace(id, packet, event)",          doc: "Client replaces textures.",                        cancellable: true },
        CallbackDef { name: "del_cube",          sig: "del_cube(id, packet, event)",         doc: "Client deletes cubes.",                            cancellable: true },
        CallbackDef { name: "edit_v_slot",       sig: "edit_v_slot(id, packet, event)",      doc: "Client edits a vslot.",                            cancellable: true },
        CallbackDef { name: "remip",             sig: "remip(id, packet, event)",            doc: "Client remips geometry.",                          cancellable: true },
        CallbackDef { name: "serv_cmd",          sig: "serv_cmd(id, packet, event)",         doc: "Client sends a server command string.",            cancellable: true },
        CallbackDef { name: "bases",             sig: "bases(id, packet, event)",            doc: "Client sends base data (Capture mode).",           cancellable: true },
        CallbackDef { name: "rep_ammo",          sig: "rep_ammo(id, packet, event)",         doc: "Client requests ammo replenishment.",              cancellable: true },
        CallbackDef { name: "try_drop_flag",     sig: "try_drop_flag(id, packet, event)",    doc: "Client attempts to drop the flag.",                cancellable: true },
        CallbackDef { name: "take_flag",         sig: "take_flag(id, packet, event)",        doc: "Client takes a flag.",                             cancellable: true },
        CallbackDef { name: "init_flags",        sig: "init_flags(id, packet, event)",       doc: "Client initialises flag state.",                   cancellable: true },
        CallbackDef { name: "init_tokens",       sig: "init_tokens(id, packet, event)",      doc: "Client initialises token state (Collect mode).",  cancellable: true },
        CallbackDef { name: "take_token",        sig: "take_token(id, packet, event)",       doc: "Client takes a token.",                            cancellable: true },
        CallbackDef { name: "deposit_tokens",    sig: "deposit_tokens(id, packet, event)",   doc: "Client deposits tokens.",                          cancellable: true },
        CallbackDef { name: "taunt",             sig: "taunt(id, packet, event)",            doc: "Client taunts.",                                   cancellable: true },
    ];

    DEFS.iter().map(|d| Entry {
        name: d.name.to_string(),
        kind: EntryKind::Callback,
        doc: d.doc.to_string(),
        sig: d.sig.to_string(),
        tags: if d.cancellable { vec!["cancellable"] } else { vec![] },
    }).collect()
}

// ---------------------------------------------------------------------------
// Helpers
// ---------------------------------------------------------------------------

fn packet_display_name(rust_type: &str) -> String {
    // e.g. "channel1::Client::<channel1::Sound>" → "Client<Sound>"
    // e.g. "channel1::ServMsg" → "ServMsg"
    let s = rust_type.replace("channel1::", "").replace("channel0::", "");
    // strip turbofish style "::<" → "<"
    s.replace("::<", "<")
}

fn hand_written_sig(name: &str) -> String {
    match name {
        "disconnect"       => "server:disconnect(id: number)".into(),
        "peer_address"     => "server:peer_address(id: number): string".into(),
        "get_cn"           => "server:get_cn(id: number): number|nil".into(),
        "register_command" => "server:register_command(name: string, desc: string, usage?: string)".into(),
        "send_server_query"=> "server:send_server_query(addr: string, response: table)".into(),
        "set_interval"     => "server:set_interval(interval_ms: number, func: function): TaskHandle".into(),
        "set_timeout"      => "server:set_timeout(timeout_ms: number, func: function): TaskHandle".into(),
        _                  => format!("server:{}(...)", name),
    }
}

fn hand_written_tags(name: &str) -> Vec<&'static str> {
    match name {
        "set_interval" | "set_timeout" => vec!["async"],
        _ => vec![],
    }
}

// ---------------------------------------------------------------------------
// HTML rendering
// ---------------------------------------------------------------------------

const HIGHLIGHT_CDN: &str =
    "https://cdnjs.cloudflare.com/ajax/libs/highlight.js/11.9.0";

fn html_escape(s: &str) -> String {
    s.replace('&', "&amp;")
     .replace('<', "&lt;")
     .replace('>', "&gt;")
     .replace('"', "&quot;")
}

fn render_tag(tag: &str) -> String {
    let (cls, label) = match tag {
        "cancellable" => ("tag-cancel",  "cancellable"),
        "packet"      => ("tag-packet",  "packet"),
        "async"       => ("tag-async",   "async"),
        _             => ("tag-default", tag),
    };
    format!(r#"<span class="tag {cls}">{label}</span>"#)
}

fn render_entry_html(e: &Entry) -> String {
    let kind_cls = match e.kind {
        EntryKind::Function => "kind-fn",
        EntryKind::Const    => "kind-const",
        EntryKind::Method   => "kind-method",
        EntryKind::Field    => "kind-field",
        EntryKind::Callback => "kind-callback",
    };
    let kind_label = match e.kind {
        EntryKind::Function => "fn",
        EntryKind::Const    => "const",
        EntryKind::Method   => "method",
        EntryKind::Field    => "field",
        EntryKind::Callback => "callback",
    };
    let tags_html: String = e.tags.iter().map(|t| render_tag(t)).collect();
    let doc_html = if e.doc.is_empty() {
        String::new()
    } else {
        // Inline `backtick` → <code>
        let escaped = html_escape(&e.doc);
        let linked = Regex::new(r"`([^`]+)`").unwrap()
            .replace_all(&escaped, r#"<code>$1</code>"#)
            .into_owned();
        format!(r#"<p class="entry-doc">{}</p>"#, linked)
    };

    format!(
        r#"<div class="entry" id="entry-{id}">
  <div class="entry-header">
    <span class="entry-kind {kind_cls}">{kind_label}</span>
    <code class="entry-sig language-lua">{sig}</code>
    <span class="entry-tags">{tags_html}</span>
  </div>
  {doc_html}
</div>"#,
        id  = html_escape(&e.name.replace('.', "-")),
        sig = html_escape(&e.sig),
    )
}

fn render_section_nav(s: &Section) -> String {
    let items: String = s.entries.iter().map(|e| {
        let id  = html_escape(&e.name.replace('.', "-"));
        let lbl = html_escape(&e.name);
        format!("<a class=\"nav-item\" href=\"#entry-{id}\">{lbl}</a>")
    }).collect::<Vec<_>>().join("\n");

    let id    = s.id;
    let title = s.title;
    format!(
        "<details class=\"nav-section\" open>\n\
         <summary><a href=\"#{id}\">{title}</a></summary>\n\
         <div class=\"nav-items\">{items}</div>\n\
         </details>"
    )
}

fn render_section_html(s: &Section) -> String {
    let entries_html: String = s.entries.iter().map(|e| render_entry_html(e)).collect::<Vec<_>>().join("\n");
    format!(
        r#"<section id="{id}">
  <h2>{title}</h2>
  <p class="section-desc">{desc}</p>
  <div class="entries">{entries_html}</div>
</section>"#,
        id    = s.id,
        title = s.title,
        desc  = s.desc,
    )
}

fn render_html(sections: &[Section]) -> String {
    let nav: String  = sections.iter().map(render_section_nav).collect::<Vec<_>>().join("\n");
    let body: String = sections.iter().map(render_section_html).collect::<Vec<_>>().join("\n");

    format!(r#"<!DOCTYPE html>
<html lang="en">
<head>
<meta charset="UTF-8">
<meta name="viewport" content="width=device-width, initial-scale=1.0">
<title>sauer Lua Plugin API</title>
<link rel="stylesheet" href="{cdn}/styles/atom-one-dark.min.css">
<script src="{cdn}/highlight.min.js"></script>
<script src="{cdn}/languages/lua.min.js"></script>
<style>
:root {{
  --bg:           #0d1117;
  --bg-sidebar:   #161b22;
  --bg-card:      #1c2128;
  --bg-card-hover:#21262d;
  --border:       #30363d;
  --text:         #e6edf3;
  --text-muted:   #7d8590;
  --accent:       #58a6ff;
  --accent-dim:   #1f4e8a;
  --green:        #3fb950;
  --purple:       #bc8cff;
  --orange:       #ffa657;
  --red:          #f85149;
  --yellow:       #d29922;
  --sidebar-w:    280px;
  --font-mono:    'SFMono-Regular', Consolas, 'Liberation Mono', Menlo, monospace;
}}

*, *::before, *::after {{ box-sizing: border-box; margin: 0; padding: 0; }}

html {{ scroll-behavior: smooth; }}

body {{
  background: var(--bg);
  color: var(--text);
  font-family: -apple-system, BlinkMacSystemFont, 'Segoe UI', Helvetica, Arial, sans-serif;
  font-size: 14px;
  line-height: 1.6;
  display: flex;
  min-height: 100vh;
}}

/* ── Sidebar ── */
#sidebar {{
  width: var(--sidebar-w);
  min-width: var(--sidebar-w);
  background: var(--bg-sidebar);
  border-right: 1px solid var(--border);
  position: fixed;
  top: 0; left: 0; bottom: 0;
  overflow-y: auto;
  display: flex;
  flex-direction: column;
  z-index: 100;
}}

#sidebar-header {{
  padding: 20px 16px 12px;
  border-bottom: 1px solid var(--border);
}}

#sidebar-header h1 {{
  font-size: 15px;
  font-weight: 600;
  color: var(--text);
  margin-bottom: 4px;
}}

#sidebar-header .subtitle {{
  font-size: 11px;
  color: var(--text-muted);
}}

#search {{
  display: block;
  width: calc(100% - 32px);
  margin: 12px 16px;
  padding: 7px 10px;
  background: var(--bg-card);
  border: 1px solid var(--border);
  border-radius: 6px;
  color: var(--text);
  font-size: 13px;
  outline: none;
  transition: border-color .15s;
}}
#search:focus {{ border-color: var(--accent); }}
#search::placeholder {{ color: var(--text-muted); }}

#nav {{ padding: 4px 0 16px; }}

.nav-section {{ border: none; }}
.nav-section > summary {{
  padding: 6px 16px;
  cursor: pointer;
  font-size: 11px;
  font-weight: 600;
  text-transform: uppercase;
  letter-spacing: .06em;
  color: var(--text-muted);
  list-style: none;
  display: flex;
  align-items: center;
  gap: 6px;
  user-select: none;
}}
.nav-section > summary::before {{
  content: '▸';
  font-size: 9px;
  transition: transform .15s;
}}
.nav-section[open] > summary::before {{ transform: rotate(90deg); }}
.nav-section > summary a {{
  color: var(--text-muted);
  text-decoration: none;
}}
.nav-section > summary:hover {{ color: var(--text); }}
.nav-section > summary:hover a {{ color: var(--text); }}
.nav-section > summary::-webkit-details-marker {{ display: none; }}

.nav-items {{
  padding: 2px 0;
}}

.nav-item {{
  display: block;
  padding: 3px 16px 3px 28px;
  color: var(--text-muted);
  text-decoration: none;
  font-size: 12.5px;
  font-family: var(--font-mono);
  white-space: nowrap;
  overflow: hidden;
  text-overflow: ellipsis;
  border-radius: 0;
  transition: background .1s, color .1s;
}}
.nav-item:hover {{
  background: var(--bg-card-hover);
  color: var(--accent);
}}
.nav-item.hidden {{ display: none; }}

/* ── Main content ── */
#main {{
  margin-left: var(--sidebar-w);
  flex: 1;
  min-width: 0;
  padding: 40px 48px 80px;
  max-width: 900px;
}}

#page-title {{
  font-size: 28px;
  font-weight: 700;
  color: var(--text);
  margin-bottom: 8px;
}}
#page-subtitle {{
  color: var(--text-muted);
  font-size: 14px;
  margin-bottom: 48px;
}}

section {{
  margin-bottom: 56px;
}}

section h2 {{
  font-size: 20px;
  font-weight: 600;
  color: var(--text);
  padding-bottom: 10px;
  border-bottom: 1px solid var(--border);
  margin-bottom: 8px;
  scroll-margin-top: 24px;
}}

.section-desc {{
  color: var(--text-muted);
  font-size: 13px;
  margin-bottom: 20px;
}}

/* ── Entry cards ── */
.entries {{ display: flex; flex-direction: column; gap: 2px; }}

.entry {{
  background: var(--bg-card);
  border: 1px solid var(--border);
  border-radius: 8px;
  padding: 14px 18px;
  transition: border-color .15s, background .15s;
  scroll-margin-top: 24px;
}}
.entry:hover {{
  border-color: #444c56;
  background: var(--bg-card-hover);
}}
.entry.hidden {{ display: none; }}

.entry-header {{
  display: flex;
  align-items: center;
  gap: 10px;
  flex-wrap: wrap;
}}

.entry-kind {{
  font-size: 10px;
  font-weight: 700;
  text-transform: uppercase;
  letter-spacing: .08em;
  padding: 2px 7px;
  border-radius: 4px;
  flex-shrink: 0;
}}
.kind-fn       {{ background: #1a3a5c; color: #58a6ff; }}
.kind-const    {{ background: #2d1c4e; color: #bc8cff; }}
.kind-method   {{ background: #1a3a5c; color: #58a6ff; }}
.kind-field    {{ background: #162a1e; color: #3fb950; }}
.kind-callback {{ background: #3b2000; color: #ffa657; }}

.entry-sig {{
  font-family: var(--font-mono);
  font-size: 13.5px;
  color: var(--text);
  flex: 1;
  min-width: 0;
}}

.entry-tags {{ display: flex; gap: 5px; flex-shrink: 0; }}

.tag {{
  font-size: 10px;
  font-weight: 600;
  padding: 2px 7px;
  border-radius: 10px;
  letter-spacing: .04em;
}}
.tag-cancel  {{ background: #3b1e1e; color: #f85149; }}
.tag-packet  {{ background: #1c2940; color: #79c0ff; }}
.tag-async   {{ background: #1a2d1a; color: #56d364; }}
.tag-default {{ background: var(--bg); color: var(--text-muted); }}

.entry-doc {{
  margin-top: 8px;
  color: #b0bec5;
  font-size: 13px;
  line-height: 1.55;
}}
.entry-doc code {{
  font-family: var(--font-mono);
  font-size: 12px;
  background: rgba(255,255,255,.07);
  padding: 1px 5px;
  border-radius: 4px;
  color: var(--purple);
}}

/* ── highlight.js tweaks ── */
.hljs {{ background: transparent !important; padding: 0 !important; }}

/* ── Scrollbar ── */
::-webkit-scrollbar {{ width: 6px; height: 6px; }}
::-webkit-scrollbar-track {{ background: transparent; }}
::-webkit-scrollbar-thumb {{ background: #30363d; border-radius: 3px; }}
::-webkit-scrollbar-thumb:hover {{ background: #444c56; }}

/* ── Responsive ── */
@media (max-width: 720px) {{
  #sidebar {{ display: none; }}
  #main    {{ margin-left: 0; padding: 24px 20px 60px; }}
}}
</style>
</head>
<body>

<aside id="sidebar">
  <div id="sidebar-header">
    <h1>sauer lua api</h1>
    <div class="subtitle">Plugin scripting reference</div>
  </div>
  <input id="search" type="search" placeholder="Search…" autocomplete="off" spellcheck="false">
  <nav id="nav">
    {nav}
  </nav>
</aside>

<main id="main">
  <h1 id="page-title">Lua Plugin API</h1>
  <p id="page-subtitle">
    Auto-generated from <code style="font-family:var(--font-mono);font-size:12px;background:rgba(255,255,255,.07);padding:2px 6px;border-radius:4px">sauer-plugin-lua/src/</code>.
    Write Lua plugins by defining callback functions and using the <code style="font-family:var(--font-mono);font-size:12px;background:rgba(255,255,255,.07);padding:2px 6px;border-radius:4px">server</code> global.
  </p>
  {body}
</main>

<script>
hljs.highlightAll();

// Highlight only .entry-sig elements that look like Lua signatures
document.querySelectorAll('.entry-sig').forEach(el => {{
  el.classList.add('hljs');
  // don't re-run full highlight, just colour it manually via CSS
}});

// Search
const search = document.getElementById('search');
search.addEventListener('input', () => {{
  const q = search.value.trim().toLowerCase();
  document.querySelectorAll('.entry').forEach(entry => {{
    const id   = entry.id.toLowerCase();
    const sig  = entry.querySelector('.entry-sig')?.textContent.toLowerCase() ?? '';
    const doc  = entry.querySelector('.entry-doc')?.textContent.toLowerCase() ?? '';
    const show = !q || id.includes(q) || sig.includes(q) || doc.includes(q);
    entry.classList.toggle('hidden', !show);
  }});
  document.querySelectorAll('.nav-item').forEach(a => {{
    const href  = a.getAttribute('href').slice(1); // e.g. "entry-disconnect"
    const entry = document.getElementById(href);
    a.classList.toggle('hidden', entry?.classList.contains('hidden') ?? false);
  }});
}});
</script>
</body>
</html>
"#,
        cdn  = HIGHLIGHT_CDN,
        nav  = nav,
        body = body,
    )
}

// ---------------------------------------------------------------------------
// EmmyLua stubs
// ---------------------------------------------------------------------------

fn render_stubs(sections: &[Section]) -> String {
    let mut out = String::new();
    writeln!(out, "---@meta").unwrap();
    writeln!(out, "-- Auto-generated EmmyLua stubs for the sauer Lua plugin API.").unwrap();
    writeln!(out, "-- Drop this file into your project root and point lua-language-server at it.").unwrap();
    writeln!(out, "").unwrap();

    // Globals
    if let Some(g) = sections.iter().find(|s| s.id == "globals") {
        writeln!(out, "-- ── Globals ─────────────────────────────────────────────────────────────────").unwrap();
        writeln!(out, "").unwrap();
        for e in &g.entries {
            if !e.doc.is_empty() {
                writeln!(out, "--- {}", e.doc).unwrap();
            }
            match e.kind {
                EntryKind::Const => {
                    writeln!(out, "---@type number").unwrap();
                    writeln!(out, "{} = 0", e.name).unwrap();
                }
                EntryKind::Function => {
                    let params = stub_params(&e.sig);
                    for (n, t) in &params {
                        writeln!(out, "---@param {} {}", n, t).unwrap();
                    }
                    let param_names: Vec<&str> = params.iter().map(|(n, _)| n.as_str()).collect();
                    writeln!(out, "function {}({}) end", e.name, param_names.join(", ")).unwrap();
                }
                _ => {}
            }
            writeln!(out, "").unwrap();
        }
    }

    // Server class
    if let Some(s) = sections.iter().find(|s| s.id == "server") {
        writeln!(out, "-- ── Server ──────────────────────────────────────────────────────────────────").unwrap();
        writeln!(out, "").unwrap();
        writeln!(out, "---@class Server").unwrap();
        writeln!(out, "---@field extra table Plugin-local key/value storage shared across the server.").unwrap();
        writeln!(out, "local Server = {{}}").unwrap();
        writeln!(out, "").unwrap();
        writeln!(out, "---@type Server").unwrap();
        writeln!(out, "server = {{}}").unwrap();
        writeln!(out, "").unwrap();

        for e in &s.entries {
            if e.kind != EntryKind::Method { continue; }
            if !e.doc.is_empty() {
                writeln!(out, "--- {}", e.doc).unwrap();
            }
            let params = stub_params(&e.sig);
            for (n, t) in &params {
                writeln!(out, "---@param {} {}", n, t).unwrap();
            }
            let ret = stub_return(&e.sig);
            if let Some(r) = ret {
                writeln!(out, "---@return {}", r).unwrap();
            }
            let param_names: Vec<&str> = params.iter().map(|(n, _)| n.as_str()).collect();
            writeln!(out, "function Server:{}({}) end", e.name, param_names.join(", ")).unwrap();
            writeln!(out, "").unwrap();
        }
    }

    // SpawnState class
    if let Some(s) = sections.iter().find(|s| s.id == "spawn-state") {
        writeln!(out, "-- ── SpawnState ──────────────────────────────────────────────────────────────").unwrap();
        writeln!(out, "").unwrap();
        writeln!(out, "---@class SpawnState").unwrap();
        for e in &s.entries {
            if e.kind != EntryKind::Field { continue; }
            let t = if e.name == "armour_type" || e.name == "gun_select" { "string" } else { "number" };
            let doc = if e.doc.is_empty() { String::new() } else { format!(" {}", e.doc) };
            writeln!(out, "---@field {} {}{}", e.name, t, doc).unwrap();
        }
        writeln!(out, "local SpawnState = {{}}").unwrap();
        writeln!(out, "").unwrap();
    }

    // Event class
    writeln!(out, "-- ── Event ───────────────────────────────────────────────────────────────────").unwrap();
    writeln!(out, "").unwrap();
    writeln!(out, "---@class Event").unwrap();
    writeln!(out, "local Event = {{}}").unwrap();
    writeln!(out, "").unwrap();
    writeln!(out, "--- Cancel this event, preventing default server processing and stopping further plugin handlers.").unwrap();
    writeln!(out, "function Event:cancel() end").unwrap();
    writeln!(out, "").unwrap();

    // TaskHandle class
    writeln!(out, "-- ── TaskHandle ──────────────────────────────────────────────────────────────").unwrap();
    writeln!(out, "").unwrap();
    writeln!(out, "---@class TaskHandle").unwrap();
    writeln!(out, "local TaskHandle = {{}}").unwrap();
    writeln!(out, "").unwrap();
    writeln!(out, "--- Abort the background task immediately.").unwrap();
    writeln!(out, "function TaskHandle:cancel() end").unwrap();
    writeln!(out, "").unwrap();

    // Callbacks
    if let Some(s) = sections.iter().find(|s| s.id == "callbacks") {
        writeln!(out, "-- ── Callbacks ───────────────────────────────────────────────────────────────").unwrap();
        writeln!(out, "-- Define these as globals in your plugin file to receive events.").unwrap();
        writeln!(out, "").unwrap();
        for e in &s.entries {
            if !e.doc.is_empty() {
                writeln!(out, "--- {}", e.doc).unwrap();
            }
            if e.tags.contains(&"cancellable") {
                writeln!(out, "--- Call `event:cancel()` to prevent default processing.").unwrap();
            }
            let params = stub_params(&e.sig);
            for (n, t) in &params {
                writeln!(out, "---@param {} {}", n, t).unwrap();
            }
            let param_names: Vec<&str> = params.iter().map(|(n, _)| n.as_str()).collect();
            writeln!(out, "function {}({}) end", e.name, param_names.join(", ")).unwrap();
            writeln!(out, "").unwrap();
        }
    }

    out
}

// Very naive param extractor: `foo(a: T, b: U): R` → vec![("a","T"),("b","U")]
fn stub_params(sig: &str) -> Vec<(String, String)> {
    let inner = sig.split('(').nth(1).and_then(|s| s.split(')').next()).unwrap_or("");
    inner.split(',').filter_map(|part| {
        let part = part.trim();
        if part.is_empty() { return None; }
        if let Some((n, t)) = part.split_once(':') {
            let name = n.trim().trim_start_matches('?').to_string();
            let t    = t.trim().to_string();
            if name.is_empty() || name == "self" { return None; }
            Some((name, t))
        } else {
            let name = part.trim_end_matches('?').to_string();
            if name.is_empty() || name == "self" { return None; }
            Some((name, "any".to_string()))
        }
    }).collect()
}

fn stub_return(sig: &str) -> Option<String> {
    sig.split(')').nth(1).and_then(|s| {
        let r = s.trim().trim_start_matches(':').trim();
        if r.is_empty() { None } else { Some(r.to_string()) }
    })
}

// ---------------------------------------------------------------------------
// Main
// ---------------------------------------------------------------------------

fn main() {
    let src_arg = std::env::args()
        .nth(1)
        .unwrap_or_else(|| "sauer-plugin-lua/src".to_string());
    let src = Path::new(&src_arg);

    macro_rules! read {
        ($p:expr) => {
            fs::read_to_string(src.join($p))
                .unwrap_or_else(|e| panic!("cannot read {}: {e}", $p))
        };
    }

    let globals_src     = read!("setup_globals.rs");
    let server_src      = read!("userdata/server.rs");
    let spawn_src       = read!("userdata/spawn_state.rs");
    let event_src       = read!("userdata/event.rs");
    let taskhandle_src  = read!("userdata/task_handle.rs");

    // Split server methods into logical sub-groups for the sidebar
    let all_server = parse_server_methods(&server_src);
    let (send_entries, rest1): (Vec<_>, Vec<_>) = all_server
        .into_iter()
        .partition(|e| e.name.starts_with("send_") && e.tags.contains(&"packet"));
    let (bcast_entries, core_entries): (Vec<_>, Vec<_>) = rest1
        .into_iter()
        .partition(|e| e.name.starts_with("broadcast_"));

    // Merge: fields first, then core methods, send, broadcast
    let server_field = Entry {
        name: "extra".to_string(),
        kind: EntryKind::Field,
        doc: "Plugin-local key/value storage table, shared across the server instance.".to_string(),
        sig: "server.extra: table".to_string(),
        tags: vec![],
    };
    let mut server_entries = vec![server_field];
    server_entries.extend(core_entries);
    server_entries.extend(send_entries);
    server_entries.extend(bcast_entries);
    // Remove duplicate names (keep first occurrence — manual entries have better docs)
    let mut seen = std::collections::HashSet::new();
    server_entries.retain(|e| seen.insert(e.name.clone()));

    let mut globals = parse_globals(&globals_src);
    // Deduplicate log aliases (print = info, keep unique names)
    globals.dedup_by(|a, b| a.name == b.name);

    let sections: Vec<Section> = vec![
        Section {
            id: "globals",
            title: "Globals",
            desc: "Functions and constants available in every plugin's environment.",
            entries: globals,
        },
        Section {
            id: "server",
            title: "Server",
            desc: "The `server` global is a userdata object available after `start()`. Use it to control the game server.",
            entries: server_entries,
        },
        Section {
            id: "spawn-state",
            title: "SpawnState",
            desc: "Passed to `on_spawn`. Modify fields before returning to override the player's spawn attributes.",
            entries: parse_spawn_state_fields(&spawn_src),
        },
        Section {
            id: "event",
            title: "Event",
            desc: "Passed as the last argument to cancellable callbacks. Call `event:cancel()` to suppress default processing.",
            entries: parse_simple_methods(&event_src, "event"),
        },
        Section {
            id: "task-handle",
            title: "TaskHandle",
            desc: "Returned by `server:set_interval` and `server:set_timeout`. Call `cancel()` to stop the task.",
            entries: parse_simple_methods(&taskhandle_src, "task"),
        },
        Section {
            id: "callbacks",
            title: "Callbacks",
            desc: "Define these as global functions in your plugin file. The server calls them on the matching event.",
            entries: callbacks(),
        },
    ];

    // Output
    let out_dir = std::env::args().nth(2).unwrap_or_else(|| "lua-api".to_string());
    fs::create_dir_all(&out_dir).unwrap_or_else(|e| panic!("cannot create {out_dir}: {e}"));

    let html_path = format!("{out_dir}/index.html");
    let stub_path = format!("{out_dir}/sauer.lua");

    let html = render_html(&sections);
    fs::write(&html_path, &html).unwrap_or_else(|e| panic!("cannot write {html_path}: {e}"));
    eprintln!("Written {} bytes → {html_path}", html.len());

    let stubs = render_stubs(&sections);
    fs::write(&stub_path, &stubs).unwrap_or_else(|e| panic!("cannot write {stub_path}: {e}"));
    eprintln!("Written {} bytes → {stub_path}", stubs.len());
}
