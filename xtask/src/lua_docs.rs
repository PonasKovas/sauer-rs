//! Writes HTML API docs and EmmyLua LSP stubs for the sauer Lua plugin API:
//! `lua-api/index.html` and `lua-api/sauer.lua`.

use regex::Regex;
use std::fmt::Write as _;
use std::fs;
use std::path::Path;

// ---------------------------------------------------------------------------
// Data model
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, PartialEq)]
enum EntryKind {
    Function, // global function
    Const,    // global constant
    Method,   // :method() on a type
    Field,    // .field on a type  (read/write)
    Callback, // plugin callback handler
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
    let field_re = Regex::new(r#"fields\.add_field_method_get\(\s*"([^"]+)""#).unwrap();
    let send_re = Regex::new(r#"add_send!\("([^"]+)",\s*([\w:<>]+)"#).unwrap();
    let bcast_re = Regex::new(r#"add_broadcast!\("([^"]+)",\s*([\w:<>]+)"#).unwrap();

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
            entries.push(Entry {
                name,
                kind: EntryKind::Method,
                doc,
                sig,
                tags,
            });
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
            let auto_doc = format!(
                "Broadcast a `{}` packet to all connected clients.",
                type_name
            );
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

/// Fields of a userdata type. `field_type` gives each field's Lua type.
fn parse_fields(src: &str, prefix: &str, field_type: fn(&str) -> &'static str) -> Vec<Entry> {
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
                    sig: format!("{prefix}.{name}: {}", field_type(&name)),
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
// Callbacks: hand-written for the special hooks (signatures come from
// sauer-plugin-lua/src/plugin.rs), parsed from `for_each_packet_hook!` for
// the packet ones.
// ---------------------------------------------------------------------------

struct CallbackDef {
    name: &'static str,
    sig: &'static str,
    doc: &'static str,
    cancellable: bool,
}

const SPECIAL_CALLBACKS: &[CallbackDef] = &[
    CallbackDef {
        name: "start",
        sig: "start()",
        doc: "Called once when the server starts and the `server` global is ready.",
        cancellable: false,
    },
    CallbackDef {
        name: "connected",
        sig: "connected(id: number, event: Event)",
        doc: "Called when a new peer connects (before it sends N_CONNECT). Cancel to drop it.",
        cancellable: true,
    },
    CallbackDef {
        name: "disconnected",
        sig: "disconnected(id: number)",
        doc: "Called when a peer disconnects.",
        cancellable: false,
    },
    CallbackDef {
        name: "on_client_welcomed",
        sig: "on_client_welcomed(id: number)",
        doc: "Called after a client completes the welcome handshake and is fully joined.",
        cancellable: false,
    },
    CallbackDef {
        name: "map_changed",
        sig: "map_changed(id: number, map_name: string, mode: string)",
        doc: "Called after the map changes. `id` is the client who caused it.",
        cancellable: false,
    },
    CallbackDef {
        name: "on_spawn",
        sig: "on_spawn(id: number, state: SpawnState, event: Event)",
        doc: "Called when a player spawns. Change `state` fields to change what they spawn with.",
        cancellable: true,
    },
    CallbackDef {
        name: "on_chat",
        sig: "on_chat(id: number, chat: Chat, event: Event)",
        doc: "Called for each chat message. Change `chat.text` to rewrite it, cancel to drop it.",
        cancellable: true,
    },
    CallbackDef {
        name: "on_damage",
        sig: "on_damage(target_id: number, actor_id: number, damage: Damage, event: Event)",
        doc: "Called before damage is dealt. Change `damage.amount` to change it, cancel to prevent it.",
        cancellable: true,
    },
    CallbackDef {
        name: "on_player_died",
        sig: "on_player_died(victim_id: number, actor_id: number)",
        doc: "Called after a player dies. `actor_id` is the victim on a suicide.",
        cancellable: false,
    },
    CallbackDef {
        name: "on_command",
        sig: "on_command(id: number, args: table, event: Event)",
        doc: "Called when a client sends a registered server command (see `register_command`). `args[1]` is the command name.",
        cancellable: true,
    },
    CallbackDef {
        name: "map",
        sig: "map(id: number, data: string, event: Event)",
        doc: "Called when a client uploads map data (binary string).",
        cancellable: true,
    },
    CallbackDef {
        name: "server_query",
        sig: "server_query(addr: string, request: table, event: Event)",
        doc: "Called for each ExtInfo/server-browser UDP query. `request.type` identifies the query kind.",
        cancellable: true,
    },
];

/// `plugin_src` is sauer-server/src/plugin.rs, which holds `for_each_packet_hook!`.
fn callbacks(plugin_src: &str) -> Vec<Entry> {
    let mut entries: Vec<Entry> = SPECIAL_CALLBACKS
        .iter()
        .map(|d| Entry {
            name: d.name.to_string(),
            kind: EntryKind::Callback,
            doc: d.doc.to_string(),
            sig: d.sig.to_string(),
            tags: if d.cancellable {
                vec!["cancellable"]
            } else {
                vec![]
            },
        })
        .collect();

    let hook_re =
        Regex::new(r"\[(N_\w+)\]\s+(\w+)\(\$crate::sauer_proto::(channel\d)::(\w+)\)").unwrap();
    let packet_hooks: Vec<Entry> = hook_re
        .captures_iter(plugin_src)
        .map(|c| Entry {
            name: c[2].to_string(),
            kind: EntryKind::Callback,
            doc: format!("Called when a client sends `{}` ({}).", &c[1], &c[3]),
            sig: format!("{}(id: number, packet: {}, event: Event)", &c[2], &c[4]),
            tags: vec!["cancellable", "packet"],
        })
        .collect();
    assert!(
        !packet_hooks.is_empty(),
        "no packet hooks found; did for_each_packet_hook! change shape?"
    );
    entries.extend(packet_hooks);
    entries
}

// ---------------------------------------------------------------------------
// Helpers
// ---------------------------------------------------------------------------

fn packet_display_name(rust_type: &str) -> String {
    // e.g. "channel1::Client::<channel1::Sound>" → "Client<Sound>"
    // e.g. "channel1::ServMsg" → "ServMsg"
    let s = rust_type
        .replace("channel1::", "")
        .replace("channel0::", "");
    // strip turbofish style "::<" → "<"
    s.replace("::<", "<")
}

fn hand_written_sig(name: &str) -> String {
    match name {
        "disconnect" => "server:disconnect(id: number, reason?: number)".into(),
        "get_id" => "server:get_id(cn: number): number|nil".into(),
        "client_ids" => "server:client_ids(): number[]".into(),
        "player" => "server:player(id: number): table|nil".into(),
        "map_name" => "server:map_name(): string".into(),
        "game_mode" => "server:game_mode(): string".into(),
        "master_mode" => "server:master_mode(): string".into(),
        "set_master_mode" => "server:set_master_mode(mode: string)".into(),
        "set_name" => "server:set_name(id: number, name: string)".into(),
        "set_team" => "server:set_team(id: number, team: string)".into(),
        "set_spectator" => "server:set_spectator(id: number, spectate: boolean)".into(),
        "set_privilege" => "server:set_privilege(id: number, privilege: string)".into(),
        "kill" => "server:kill(id: number): boolean".into(),
        "ban" => "server:ban(id: number)".into(),
        "peer_address" => "server:peer_address(id: number): string".into(),
        "get_cn" => "server:get_cn(id: number): number|nil".into(),
        "get_session_id" => "server:get_session_id(id: number): number|nil".into(),
        "register_command" => {
            "server:register_command(name: string, desc: string, usage?: string)".into()
        }
        "send_server_query" => "server:send_server_query(addr: string, response: table)".into(),
        "set_interval" => {
            "server:set_interval(interval_ms: number, func: function): TaskHandle".into()
        }
        "set_timeout" => {
            "server:set_timeout(timeout_ms: number, func: function): TaskHandle".into()
        }
        _ => format!("server:{}(...)", name),
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

const HIGHLIGHT_CDN: &str = "https://cdnjs.cloudflare.com/ajax/libs/highlight.js/11.9.0";

fn html_escape(s: &str) -> String {
    s.replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
        .replace('"', "&quot;")
}

fn render_tag(tag: &str) -> String {
    let (cls, label) = match tag {
        "cancellable" => ("tag-cancel", "cancellable"),
        "packet" => ("tag-packet", "packet"),
        "async" => ("tag-async", "async"),
        _ => ("tag-default", tag),
    };
    format!(r#"<span class="tag {cls}">{label}</span>"#)
}

fn render_entry_html(e: &Entry) -> String {
    let kind_cls = match e.kind {
        EntryKind::Function => "kind-fn",
        EntryKind::Const => "kind-const",
        EntryKind::Method => "kind-method",
        EntryKind::Field => "kind-field",
        EntryKind::Callback => "kind-callback",
    };
    let kind_label = match e.kind {
        EntryKind::Function => "fn",
        EntryKind::Const => "const",
        EntryKind::Method => "method",
        EntryKind::Field => "field",
        EntryKind::Callback => "callback",
    };
    let tags_html: String = e.tags.iter().map(|t| render_tag(t)).collect();
    let doc_html = if e.doc.is_empty() {
        String::new()
    } else {
        // Inline `backtick` → <code>
        let escaped = html_escape(&e.doc);
        let linked = Regex::new(r"`([^`]+)`")
            .unwrap()
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
        id = html_escape(&e.name.replace('.', "-")),
        sig = html_escape(&e.sig),
    )
}

fn render_section_nav(s: &Section) -> String {
    let items: String = s
        .entries
        .iter()
        .map(|e| {
            let id = html_escape(&e.name.replace('.', "-"));
            let lbl = html_escape(&e.name);
            format!("<a class=\"nav-item\" href=\"#entry-{id}\">{lbl}</a>")
        })
        .collect::<Vec<_>>()
        .join("\n");

    let id = s.id;
    let title = s.title;
    format!(
        "<details class=\"nav-section\" open>\n\
         <summary><a href=\"#{id}\">{title}</a></summary>\n\
         <div class=\"nav-items\">{items}</div>\n\
         </details>"
    )
}

fn render_section_html(s: &Section) -> String {
    let entries_html: String = s
        .entries
        .iter()
        .map(render_entry_html)
        .collect::<Vec<_>>()
        .join("\n");
    format!(
        r#"<section id="{id}">
  <h2>{title}</h2>
  <p class="section-desc">{desc}</p>
  <div class="entries">{entries_html}</div>
</section>"#,
        id = s.id,
        title = s.title,
        desc = s.desc,
    )
}

fn render_html(sections: &[Section]) -> String {
    let nav: String = sections
        .iter()
        .map(render_section_nav)
        .collect::<Vec<_>>()
        .join("\n");
    let body: String = sections
        .iter()
        .map(render_section_html)
        .collect::<Vec<_>>()
        .join("\n");

    format!(
        r#"<!DOCTYPE html>
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
        cdn = HIGHLIGHT_CDN,
        nav = nav,
        body = body,
    )
}

// ---------------------------------------------------------------------------
// EmmyLua stubs
// ---------------------------------------------------------------------------

fn render_stubs(sections: &[Section]) -> String {
    let mut out = String::new();
    writeln!(out, "---@meta").unwrap();
    writeln!(
        out,
        "-- Auto-generated EmmyLua stubs for the sauer Lua plugin API."
    )
    .unwrap();
    writeln!(
        out,
        "-- Drop this file into your project root and point lua-language-server at it."
    )
    .unwrap();
    writeln!(out).unwrap();

    // Globals
    if let Some(g) = sections.iter().find(|s| s.id == "globals") {
        writeln!(
            out,
            "-- ── Globals ─────────────────────────────────────────────────────────────────"
        )
        .unwrap();
        writeln!(out).unwrap();
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
            writeln!(out).unwrap();
        }
    }

    // Server class
    if let Some(s) = sections.iter().find(|s| s.id == "server") {
        writeln!(
            out,
            "-- ── Server ──────────────────────────────────────────────────────────────────"
        )
        .unwrap();
        writeln!(out).unwrap();
        writeln!(out, "---@class Server").unwrap();
        writeln!(
            out,
            "---@field extra table Plugin-local key/value storage shared across the server."
        )
        .unwrap();
        writeln!(out, "local Server = {{}}").unwrap();
        writeln!(out).unwrap();
        writeln!(out, "---@type Server").unwrap();
        writeln!(out, "server = {{}}").unwrap();
        writeln!(out).unwrap();

        for e in &s.entries {
            if e.kind != EntryKind::Method {
                continue;
            }
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
            writeln!(
                out,
                "function Server:{}({}) end",
                e.name,
                param_names.join(", ")
            )
            .unwrap();
            writeln!(out).unwrap();
        }
    }

    // Field-only classes; each field's type is after the `:` in its sig.
    for s in sections
        .iter()
        .filter(|s| matches!(s.id, "spawn-state" | "chat" | "damage"))
    {
        writeln!(out, "-- ── {} ──", s.title).unwrap();
        writeln!(out).unwrap();
        writeln!(out, "---@class {}", s.title).unwrap();
        for e in s.entries.iter().filter(|e| e.kind == EntryKind::Field) {
            let t = e.sig.rsplit_once(": ").map_or("any", |(_, t)| t);
            let doc = if e.doc.is_empty() {
                String::new()
            } else {
                format!(" {}", e.doc)
            };
            writeln!(out, "---@field {} {}{}", e.name, t, doc).unwrap();
        }
        writeln!(out, "local {} = {{}}", s.title).unwrap();
        writeln!(out).unwrap();
    }

    // Event class
    writeln!(
        out,
        "-- ── Event ───────────────────────────────────────────────────────────────────"
    )
    .unwrap();
    writeln!(out).unwrap();
    writeln!(out, "---@class Event").unwrap();
    writeln!(out, "local Event = {{}}").unwrap();
    writeln!(out).unwrap();
    writeln!(out, "--- Cancel this event, preventing default server processing and stopping further plugin handlers.").unwrap();
    writeln!(out, "function Event:cancel() end").unwrap();
    writeln!(out).unwrap();

    // TaskHandle class
    writeln!(
        out,
        "-- ── TaskHandle ──────────────────────────────────────────────────────────────"
    )
    .unwrap();
    writeln!(out).unwrap();
    writeln!(out, "---@class TaskHandle").unwrap();
    writeln!(out, "local TaskHandle = {{}}").unwrap();
    writeln!(out).unwrap();
    writeln!(out, "--- Abort the background task immediately.").unwrap();
    writeln!(out, "function TaskHandle:cancel() end").unwrap();
    writeln!(out).unwrap();

    // Callbacks
    if let Some(s) = sections.iter().find(|s| s.id == "callbacks") {
        writeln!(
            out,
            "-- ── Callbacks ───────────────────────────────────────────────────────────────"
        )
        .unwrap();
        writeln!(
            out,
            "-- Define these as globals in your plugin file to receive events."
        )
        .unwrap();
        writeln!(out).unwrap();
        for e in &s.entries {
            if !e.doc.is_empty() {
                writeln!(out, "--- {}", e.doc).unwrap();
            }
            if e.tags.contains(&"cancellable") {
                writeln!(
                    out,
                    "--- Call `event:cancel()` to prevent default processing."
                )
                .unwrap();
            }
            let params = stub_params(&e.sig);
            for (n, t) in &params {
                writeln!(out, "---@param {} {}", n, t).unwrap();
            }
            let param_names: Vec<&str> = params.iter().map(|(n, _)| n.as_str()).collect();
            writeln!(out, "function {}({}) end", e.name, param_names.join(", ")).unwrap();
            writeln!(out).unwrap();
        }
    }

    out
}

// Very naive param extractor: `foo(a: T, b: U): R` → vec![("a","T"),("b","U")]
fn stub_params(sig: &str) -> Vec<(String, String)> {
    let inner = sig
        .split('(')
        .nth(1)
        .and_then(|s| s.split(')').next())
        .unwrap_or("");
    inner
        .split(',')
        .filter_map(|part| {
            let part = part.trim();
            if part.is_empty() {
                return None;
            }
            if let Some((n, t)) = part.split_once(':') {
                let name = n.trim().trim_start_matches('?').to_string();
                let t = t.trim().to_string();
                if name.is_empty() || name == "self" {
                    return None;
                }
                Some((name, t))
            } else {
                let name = part.trim_end_matches('?').to_string();
                if name.is_empty() || name == "self" {
                    return None;
                }
                Some((name, "any".to_string()))
            }
        })
        .collect()
}

fn stub_return(sig: &str) -> Option<String> {
    sig.split(')').nth(1).and_then(|s| {
        let r = s.trim().trim_start_matches(':').trim();
        if r.is_empty() {
            None
        } else {
            Some(r.to_string())
        }
    })
}

// ---------------------------------------------------------------------------
// Main
// ---------------------------------------------------------------------------

/// Reads the sources under the workspace `root` and writes the docs to `out_dir`.
pub fn run(root: &Path, out_dir: &Path) {
    let read = |p: &str| {
        fs::read_to_string(root.join(p)).unwrap_or_else(|e| panic!("cannot read {p}: {e}"))
    };

    let globals_src = read("sauer-plugin-lua/src/setup_globals.rs");
    let server_src = read("sauer-plugin-lua/src/userdata/server.rs");
    let spawn_src = read("sauer-plugin-lua/src/userdata/spawn_state.rs");
    let hook_value_src = read("sauer-plugin-lua/src/userdata/hook_value.rs");
    let event_src = read("sauer-plugin-lua/src/userdata/event.rs");
    let taskhandle_src = read("sauer-plugin-lua/src/userdata/task_handle.rs");
    let plugin_src = read("sauer-server/src/plugin.rs");
    let (damage_src, chat_src) = hook_value_src
        .split_once("pub struct ChatUD")
        .expect("hook_value.rs: ChatUD not found");

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
            entries: parse_fields(&spawn_src, "spawn_state", |name| match name {
                "armour_type" | "gun_select" => "string",
                _ => "number",
            }),
        },
        Section {
            id: "chat",
            title: "Chat",
            desc: "Passed to `on_chat`. Change `text` to change what everyone sees.",
            entries: parse_fields(chat_src, "chat", |_| "string"),
        },
        Section {
            id: "damage",
            title: "Damage",
            desc: "Passed to `on_damage`. Change `amount` to change the damage dealt.",
            entries: parse_fields(damage_src, "damage", |_| "number"),
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
            entries: callbacks(&plugin_src),
        },
    ];

    fs::create_dir_all(out_dir)
        .unwrap_or_else(|e| panic!("cannot create {}: {e}", out_dir.display()));
    for (name, contents) in [
        ("index.html", render_html(&sections)),
        ("sauer.lua", render_stubs(&sections)),
    ] {
        let path = out_dir.join(name);
        fs::write(&path, &contents)
            .unwrap_or_else(|e| panic!("cannot write {}: {e}", path.display()));
        eprintln!("Written {} bytes → {}", contents.len(), path.display());
    }
}
