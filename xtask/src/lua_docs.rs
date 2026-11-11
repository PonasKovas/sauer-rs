//! Writes HTML API docs and EmmyLua LSP stubs for the sauer Lua plugin API:
//! `lua-api/index.html` and `lua-api/sauer.lua`.
//!
//! Packets and the types inside them are read from the sauer-proto sources,
//! and each Rust type is mapped to what Lua sees. A type with no mapping, a
//! server method with no signature or a method the parser missed stops the
//! generator, so the docs can't quietly fall behind the code.

use crate::rust_src::*;
use regex::Regex;
use std::collections::{BTreeMap, BTreeSet, HashMap};
use std::fmt::Write as _;
use std::fs;
use std::path::Path;

// ---------------------------------------------------------------------------
// Data model
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, Copy, PartialEq, Default)]
enum EntryKind {
    #[default]
    Function,
    Const,
    Method,
    Field,
    Callback,
    Packet,
    Type,
    Enum,
}

#[derive(Debug, Clone, Default)]
struct Entry {
    name: String,
    kind: EntryKind,
    doc: String,
    /// Lua signature. Links are written `[[anchor|Label]]`.
    sig: String,
    /// The signature the EmmyLua stubs use, when it differs from `sig`.
    stub_sig: Option<String>,
    tags: Vec<String>,
    /// Fields of a table or userdata, or values of an enum.
    fields: Vec<FieldDoc>,
    /// Extra lines under the doc, with links.
    notes: Vec<String>,
}

#[derive(Debug, Clone, Default)]
struct FieldDoc {
    name: String,
    /// Lua type with links; empty for enum values.
    ty: String,
    doc: String,
    /// A method (`obj:name()`) rather than a field.
    method: bool,
}

struct Section {
    id: &'static str,
    title: &'static str,
    desc: &'static str,
    entries: Vec<Entry>,
}

// ---------------------------------------------------------------------------
// Links
// ---------------------------------------------------------------------------

fn link(anchor: &str, label: &str) -> String {
    format!("[[{anchor}|{label}]]")
}

fn anchor(section: &str, name: &str) -> String {
    format!("{section}-{}", name.replace(['.', ':'], "-"))
}

fn link_re() -> Regex {
    Regex::new(r"\[\[([^|\]]+)\|([^\]]+)\]\]").unwrap()
}

/// The text of `s` with its links turned into their labels.
fn plain(s: &str) -> String {
    link_re().replace_all(s, "$2").into_owned()
}

/// Names that `@Name` in a hand-written signature, and a backticked name in
/// a doc, link to.
#[derive(Default)]
struct Names {
    anchors: HashMap<String, String>,
    ambiguous: BTreeSet<String>,
}

impl Names {
    fn add(&mut self, name: &str, anchor: String) {
        if self.anchors.insert(name.to_string(), anchor).is_some() {
            self.ambiguous.insert(name.to_string());
        }
    }

    fn get(&self, name: &str) -> Option<&String> {
        if self.ambiguous.contains(name) {
            return None;
        }
        self.anchors.get(name)
    }

    /// Replaces each `@Name` in `s` with a link to it.
    fn resolve(&self, s: &str) -> String {
        Regex::new(r"@([A-Za-z_][\w.]*)")
            .unwrap()
            .replace_all(s, |c: &regex::Captures| {
                let name = &c[1];
                let a = self.get(name).unwrap_or_else(|| {
                    panic!("`@{name}` in `{s}` names nothing documented, or more than one thing")
                });
                link(a, name)
            })
            .into_owned()
    }
}

// ---------------------------------------------------------------------------
// Rust types as Lua sees them
// ---------------------------------------------------------------------------

/// A table or userdata documented by hand, with field types in Rust syntax.
struct Custom {
    name: &'static str,
    doc: &'static str,
    fields: &'static [(&'static str, &'static str, &'static str)],
}

/// Types whose Lua shape isn't their Rust fields, written out by hand. The
/// field types still go through [`TypeMap::lua`].
const CUSTOM_TYPES: &[Custom] = &[
    Custom {
        name: "Vec3",
        doc: "A position or vector. Packets give it as userdata; to send one, pass a table with the same fields. World positions are numbers, grid positions (in `SelInfo`) integers.",
        fields: &[("x", "f32", ""), ("y", "f32", ""), ("z", "f32", "")],
    },
    Custom {
        name: "SelInfo",
        doc: "An edit selection, the engine's `selinfo`.",
        fields: &[
            ("o", "Vec3<i32>", "Corner the selection starts at."),
            ("s", "Vec3<i32>", "Size, in cubes of `grid`."),
            ("grid", "i32", "Cube size."),
            ("orient", "Orient", "Selected face."),
            ("cx", "i32", "Corner selection."),
            ("cxs", "i32", ""),
            ("cy", "i32", ""),
            ("cys", "i32", ""),
            ("corner", "i32", ""),
        ],
    },
    Custom {
        name: "VSlotProps",
        doc: "What a vslot changes on top of its texture. Unset fields keep the texture's value.",
        fields: &[
            ("params", "Vec<VSlotParam>", "Shader params."),
            ("scale", "Option<f32>", ""),
            ("rotation", "Option<i32>", ""),
            ("offset", "Option<[i32; 2]>", "`{x, y}`"),
            ("scroll", "Option<[f32; 2]>", "`{x, y}`"),
            ("layer", "Option<i32>", ""),
            ("alpha", "Option<[f32; 2]>", "`{front, back}`"),
            ("color", "Option<[f32; 3]>", "`{r, g, b}`"),
        ],
    },
    Custom {
        name: "FlagState",
        doc: "A flag in `N_INITFLAGS`. Where it is follows from which of `owner` and `drop_loc` is set: neither means it's at home.",
        fields: &[
            ("version", "i32", ""),
            ("spawn_index", "i32", ""),
            ("invis", "bool", "Defaults to false when sending."),
            ("owner", "Option<Cn>", "Who carries it."),
            ("drop_loc", "Option<Vec3<f32>>", "Where it was dropped."),
            (
                "hold",
                "Option<HoldTeam>",
                "Hold modes only, and then on every flag.",
            ),
        ],
    },
    Custom {
        name: "HoldTeam",
        doc: "Which team holds a flag in hold modes. An empty table means no team.",
        fields: &[
            ("team", "Option<i32>", ""),
            ("hold_time", "Option<i32>", ""),
        ],
    },
    Custom {
        name: "TexVariant",
        doc: "A texture with changes on top, the table form of `TexRef`.",
        fields: &[
            ("slot", "u16", "The texture slot."),
            ("vslot", "VSlotProps", "What changes."),
        ],
    },
    Custom {
        name: "ClipboardPayload",
        doc: "A copied block of cubes. Lua can't read inside it or build one, so packets that carry it can't be sent from Lua.",
        fields: &[],
    },
    Custom {
        name: "UndoPayload",
        doc: "An undo or redo block. Lua can't read inside it or build one, so packets that carry it can't be sent from Lua.",
        fields: &[],
    },
];

/// Types that are another Lua type: (name, doc, the Rust types it can be).
const ALIASES: &[(&str, &str, &[&str])] = &[
    (
        "Cn",
        "A client number, 0 to 255: the number players see and type. Not the same as a peer `id`; see `server:get_id` and `server:get_cn`.",
        &["i32"],
    ),
    (
        "EditVarValue",
        "The value of an edit var: an integer, a float or a string.",
        &["i32", "f32", "CubeString"],
    ),
    (
        "TexRef",
        "A texture in an edit packet: a vslot index, or a `TexVariant` table for a texture with changes.",
        &["u16", "TexVariant"],
    ),
];

struct TypeMap {
    /// Data types by Rust name. Hand-written ones replace parsed ones.
    structs: HashMap<String, (String, Vec<FieldDef>)>,
    enums: HashMap<String, EnumDef>,
    aliases: HashSet<&'static str>,
    /// Objects (hand-written tables) that fields can name.
    objects: HashSet<String>,
    /// Shown names, for data types that share a name with a packet.
    display: HashMap<String, String>,
    used_structs: BTreeSet<String>,
    used_enums: BTreeSet<String>,
    used_aliases: BTreeSet<String>,
    unknown: BTreeSet<String>,
}

use std::collections::HashSet;

fn generic<'a>(ty: &'a str, outer: &str) -> Option<&'a str> {
    ty.strip_prefix(outer)?
        .strip_prefix('<')?
        .strip_suffix('>')
        .map(str::trim)
}

impl TypeMap {
    fn shown(&self, name: &str) -> String {
        self.display
            .get(name)
            .cloned()
            .unwrap_or_else(|| name.to_string())
    }

    /// The Lua type of the Rust type `ty`, with links.
    fn lua(&mut self, ty: &str) -> String {
        let ty = ty.trim();
        let ty = ty.strip_prefix("data::").unwrap_or(ty);
        if let Some(inner) = generic(ty, "Option") {
            return format!("{}|nil", self.lua(inner));
        }
        if let Some(inner) = generic(ty, "Vec") {
            return format!("{}[]", self.elem(inner));
        }
        if let Some(inner) = generic(ty, "Box") {
            return self.lua(inner);
        }
        if let Some((inner, _)) = ty
            .strip_prefix('[')
            .and_then(|s| s.strip_suffix(']'))
            .and_then(|s| s.rsplit_once(';'))
        {
            return format!("{}[]", self.elem(inner));
        }
        match ty {
            "i8" | "i16" | "i32" | "i64" | "u8" | "u16" | "u32" | "u64" | "usize" => {
                "integer".into()
            }
            "f32" | "f64" => "number".into(),
            "bool" => "boolean".into(),
            "CubeString" | "String" => "string".into(),
            // Lua gets the flag list itself.
            "InitFlags" => self.lua("Vec<FlagState>"),
            _ if ty.starts_with("Vec3<") => self.struct_link("Vec3"),
            _ if self.objects.contains(ty) => link(&anchor("objects", ty), ty),
            _ if self.aliases.contains(ty) => {
                self.used_aliases.insert(ty.to_string());
                link(&anchor("types", ty), ty)
            }
            _ if self.enums.contains_key(ty) => {
                self.used_enums.insert(ty.to_string());
                link(&anchor("enums", ty), ty)
            }
            _ if self.structs.contains_key(ty) => self.struct_link(ty),
            _ => {
                self.unknown.insert(ty.to_string());
                "any".into()
            }
        }
    }

    /// Like [`lua`](Self::lua), in brackets when it's a union.
    fn elem(&mut self, ty: &str) -> String {
        let t = self.lua(ty);
        if plain(&t).contains('|') {
            format!("({t})")
        } else {
            t
        }
    }

    fn struct_link(&mut self, name: &str) -> String {
        self.used_structs.insert(name.to_string());
        let shown = self.shown(name);
        link(&anchor("types", &shown), &shown)
    }

    fn fields(&mut self, fields: &[FieldDef]) -> Vec<FieldDoc> {
        fields
            .iter()
            .map(|f| FieldDoc {
                name: f.name.clone(),
                ty: self.lua(&f.ty),
                doc: f.doc.clone(),
                method: false,
            })
            .collect()
    }
}

fn field_defs(fields: &[(&str, &str, &str)]) -> Vec<FieldDef> {
    fields
        .iter()
        .map(|&(name, ty, doc)| FieldDef {
            name: name.into(),
            ty: ty.into(),
            doc: doc.into(),
        })
        .collect()
}

// ---------------------------------------------------------------------------
// Parsers for the Lua glue
// ---------------------------------------------------------------------------

/// Each match of `re` in `src`, with the doc comment above it.
fn documented<'a>(src: &'a str, re: &Regex) -> Vec<(regex::Captures<'a>, String)> {
    re.captures_iter(src)
        .map(|c| {
            let start = c.get(0).unwrap().start();
            let line_start = src[..start].rfind('\n').map_or(0, |i| i + 1);
            let doc = extract_block_doc(&src[..line_start]);
            (c, doc)
        })
        .collect()
}

fn parse_globals(src: &str) -> Vec<Entry> {
    let re = Regex::new(r#"(env|ui_table)\.set\(\s*"([^"]+)""#).unwrap();
    let mut entries: Vec<Entry> = Vec::new();
    for (c, doc) in documented(src, &re) {
        let name = &c[2];
        let entry = if &c[1] == "ui_table" {
            let sig = match name {
                "tip" => "ui.tip(cmd: string, desc: string): string".to_string(),
                "sep" => "ui.sep(): string".to_string(),
                _ => format!("ui.{name}(text: string): string"),
            };
            Entry {
                name: format!("ui.{name}"),
                sig,
                doc,
                ..Default::default()
            }
        } else if name.chars().all(|c| c.is_ascii_uppercase() || c == '_') {
            Entry {
                name: name.into(),
                kind: EntryKind::Const,
                sig: format!("{name}: integer"),
                doc,
                ..Default::default()
            }
        } else if name == "ui" {
            continue;
        } else {
            Entry {
                name: name.into(),
                sig: format!("{name}(...: any)"),
                doc,
                ..Default::default()
            }
        };
        if !entries.iter().any(|e| e.name == entry.name) {
            entries.push(entry);
        }
    }
    entries
}

/// Signatures of the server methods that aren't `send_*`/`broadcast_*`.
fn server_sig(name: &str) -> Option<&'static str> {
    Some(match name {
        "disconnect" => "server:disconnect(id: integer, reason?: integer)",
        "get_id" => "server:get_id(cn: @Cn): integer|nil",
        "client_ids" => "server:client_ids(): integer[]",
        "player" => "server:player(id: integer): @Player|nil",
        "map_name" => "server:map_name(): string",
        "game_mode" => "server:game_mode(): @GameMode",
        "master_mode" => "server:master_mode(): @MasterMode",
        "set_master_mode" => "server:set_master_mode(mode: @MasterMode)",
        "set_name" => "server:set_name(id: integer, name: string)",
        "set_team" => "server:set_team(id: integer, team: string)",
        "set_spectator" => "server:set_spectator(id: integer, spectate: boolean)",
        "set_privilege" => "server:set_privilege(id: integer, privilege: @Privilege)",
        "kill" => "server:kill(id: integer): boolean",
        "ban" => "server:ban(id: integer)",
        "peer_address" => "server:peer_address(id: integer): string",
        "get_cn" => "server:get_cn(id: integer): @Cn|nil",
        "get_session_id" => "server:get_session_id(id: integer): integer|nil",
        "register_command" => "server:register_command(name: string, desc: string, usage?: string)",
        "send_server_query" => {
            "server:send_server_query(addr: string, response: @ServerQueryResponse)"
        }
        "set_interval" => "server:set_interval(interval_ms: integer, func: function): @TaskHandle",
        "set_timeout" => "server:set_timeout(timeout_ms: integer, func: function): @TaskHandle",
        _ => return None,
    })
}

struct SendMethod {
    name: String,
    doc: String,
    packet: String,
    /// `Client<T>`: the table also needs `client_num`.
    wrapped: bool,
    broadcast: bool,
}

fn parse_server(src: &str) -> (Vec<(String, String)>, Vec<SendMethod>) {
    let method_re = Regex::new(r#"methods\.add_method(?:_mut)?\(\s*"([^"]+)""#).unwrap();
    let methods = documented(src, &method_re)
        .into_iter()
        .map(|(c, doc)| (c[1].to_string(), doc))
        .collect();

    let send_re =
        Regex::new(r#"add_(send|broadcast)!\(\s*"([^"]+)",\s*([\w:<>]+?)\s*,?\s*\)"#).unwrap();
    let client_re = Regex::new(r"Client::<\w+::(\w+)>").unwrap();
    let sends: Vec<SendMethod> = documented(src, &send_re)
        .into_iter()
        .map(|(c, doc)| {
            let ty = &c[3];
            let (packet, wrapped) = match client_re.captures(ty) {
                Some(inner) => (inner[1].to_string(), true),
                None => (ty.rsplit("::").next().unwrap().to_string(), false),
            };
            SendMethod {
                name: c[2].to_string(),
                doc,
                packet,
                wrapped,
                broadcast: &c[1] == "broadcast",
            }
        })
        .collect();
    let calls = src.matches("add_send!(").count() + src.matches("add_broadcast!(").count();
    assert_eq!(
        sends.len(),
        calls,
        "parsed {} of {calls} add_send!/add_broadcast! calls",
        sends.len()
    );
    (methods, sends)
}

/// Getters (and so fields) of a userdata, with their docs.
fn parse_getters(src: &str) -> Vec<(String, String)> {
    let re = Regex::new(r#"fields\.add_field_method_get\(\s*"([^"]+)""#).unwrap();
    documented(src, &re)
        .into_iter()
        .map(|(c, doc)| (c[1].to_string(), doc))
        .collect()
}

fn parse_methods(src: &str) -> Vec<(String, String)> {
    let re = Regex::new(r#"methods\.add_method(?:_mut)?\(\s*"([^"]+)""#).unwrap();
    documented(src, &re)
        .into_iter()
        .map(|(c, doc)| (c[1].to_string(), doc))
        .collect()
}

// ---------------------------------------------------------------------------
// Callbacks
// ---------------------------------------------------------------------------

struct CallbackDef {
    name: &'static str,
    sig: &'static str,
    doc: &'static str,
    cancellable: bool,
}

/// The hooks that aren't packets. Signatures follow
/// sauer-plugin-lua/src/plugin.rs.
const SPECIAL_CALLBACKS: &[CallbackDef] = &[
    CallbackDef {
        name: "start",
        sig: "start()",
        doc: "Called once when the server starts and the `server` global is ready.",
        cancellable: false,
    },
    CallbackDef {
        name: "connected",
        sig: "connected(id: integer, event: @Event)",
        doc: "Called when a new peer connects, before it sends `N_CONNECT`. Cancel to drop it.",
        cancellable: true,
    },
    CallbackDef {
        name: "disconnected",
        sig: "disconnected(id: integer)",
        doc: "Called when a peer disconnects.",
        cancellable: false,
    },
    CallbackDef {
        name: "on_client_welcomed",
        sig: "on_client_welcomed(id: integer)",
        doc: "Called after a client finishes joining.",
        cancellable: false,
    },
    CallbackDef {
        name: "map_changed",
        sig: "map_changed(id: integer, map_name: string, mode: @GameMode)",
        doc: "Called after the map changes. `id` is the client who caused it.",
        cancellable: false,
    },
    CallbackDef {
        name: "on_spawn",
        sig: "on_spawn(id: integer, state: @Loadout, event: @Event)",
        doc: "Called when a player spawns. Change `state` to change what they spawn with, cancel to keep them dead.",
        cancellable: true,
    },
    CallbackDef {
        name: "on_chat",
        sig: "on_chat(id: integer, chat: @Chat, event: @Event)",
        doc: "Called for each chat message. Change `chat.text` to rewrite it, cancel to drop it.",
        cancellable: true,
    },
    CallbackDef {
        name: "on_team_chat",
        sig: "on_team_chat(id: integer, chat: @Chat, event: @Event)",
        doc: "Called for each team chat message that will reach teammates. Change `chat.text` to rewrite it, cancel to drop it.",
        cancellable: true,
    },
    CallbackDef {
        name: "on_damage",
        sig: "on_damage(target_id: integer, actor_id: integer, damage: @DamageInfo, event: @Event)",
        doc: "Called before damage is dealt. Change `damage.amount` to change it, cancel to prevent it.",
        cancellable: true,
    },
    CallbackDef {
        name: "on_player_died",
        sig: "on_player_died(victim_id: integer, actor_id: integer)",
        doc: "Called after a player dies. `actor_id` is the victim on a suicide.",
        cancellable: false,
    },
    CallbackDef {
        name: "on_command",
        sig: "on_command(id: integer, args: string[], event: @Event)",
        doc: "Called when a client sends a registered server command (see `register_command`). `args[1]` is the command name.",
        cancellable: true,
    },
    CallbackDef {
        name: "map",
        sig: "map(id: integer, data: string, event: @Event)",
        doc: "Called when a client uploads map data, as a binary string.",
        cancellable: true,
    },
    CallbackDef {
        name: "server_query",
        sig: "server_query(addr: string, request: @ServerQueryRequest, event: @Event)",
        doc: "Called for each server browser (ExtInfo) UDP query. Cancel and answer with `send_server_query` to reply yourself.",
        cancellable: true,
    },
];

struct PacketHook {
    msg_id: String,
    hook: String,
    packet: String,
}

/// `plugin_src` is sauer-server/src/plugin.rs, which holds `for_each_packet_hook!`.
fn parse_packet_hooks(plugin_src: &str) -> Vec<PacketHook> {
    let re = Regex::new(r"\[(N_\w+)\]\s+(\w+)\(\$crate::sauer_proto::channel\d::(\w+)\)").unwrap();
    let hooks: Vec<PacketHook> = re
        .captures_iter(plugin_src)
        .map(|c| PacketHook {
            msg_id: c[1].to_string(),
            hook: c[2].to_string(),
            packet: c[3].to_string(),
        })
        .collect();
    assert!(
        !hooks.is_empty(),
        "no packet hooks found; did for_each_packet_hook! change shape?"
    );
    hooks
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

/// Markup (links) to HTML.
fn markup_html(s: &str) -> String {
    link_re()
        .replace_all(&html_escape(s), r##"<a href="#$1">$2</a>"##)
        .into_owned()
}

/// Doc text to HTML: backticks become code, and link when they name
/// something documented.
fn doc_html(s: &str, names: &Names) -> String {
    let code_re = Regex::new(r"`([^`]+)`").unwrap();
    code_re
        .replace_all(&markup_html(s), |c: &regex::Captures| {
            let text = &c[1];
            let key = text
                .trim_start_matches("server:")
                .trim_end_matches("()")
                .replace("&quot;", "\"");
            match names.get(&key) {
                Some(a) => format!(r##"<a href="#{a}"><code>{text}</code></a>"##),
                None => format!("<code>{text}</code>"),
            }
        })
        .into_owned()
}

fn render_tag(tag: &str) -> String {
    let cls = match tag {
        "cancellable" => "tag-cancel",
        "packet" | "received" | "sendable" => "tag-packet",
        "async" => "tag-async",
        "read/write" => "tag-rw",
        "needs client_num" => "tag-warn",
        _ if tag.starts_with("N_") => "tag-id",
        _ => "tag-default",
    };
    format!(r#"<span class="tag {cls}">{}</span>"#, html_escape(tag))
}

fn render_entry_html(section: &str, e: &Entry, names: &Names) -> String {
    let (kind_cls, kind_label) = match e.kind {
        EntryKind::Function => ("kind-fn", "fn"),
        EntryKind::Const => ("kind-const", "const"),
        EntryKind::Method => ("kind-method", "method"),
        EntryKind::Field => ("kind-field", "field"),
        EntryKind::Callback => ("kind-callback", "callback"),
        EntryKind::Packet => ("kind-packet", "packet"),
        EntryKind::Type => ("kind-type", "type"),
        EntryKind::Enum => ("kind-enum", "enum"),
    };
    let id = anchor(section, &e.name);
    let tags: String = e.tags.iter().map(|t| render_tag(t)).collect();
    let mut body = String::new();
    if !e.doc.is_empty() {
        writeln!(
            body,
            r#"<p class="entry-doc">{}</p>"#,
            doc_html(&e.doc, names)
        )
        .unwrap();
    }
    if !e.fields.is_empty() {
        body.push_str(r#"<table class="fields"><tbody>"#);
        for f in &e.fields {
            let name = if f.method {
                format!(":{}()", f.name)
            } else if e.kind == EntryKind::Enum && f.ty.is_empty() {
                format!("\"{}\"", f.name)
            } else {
                f.name.clone()
            };
            let ty = if f.ty.is_empty() {
                String::new()
            } else {
                format!("<code>{}</code>", markup_html(&f.ty))
            };
            write!(
                body,
                r#"<tr><td class="f-name"><code>{}</code></td><td class="f-type">{ty}</td><td class="f-doc">{}</td></tr>"#,
                html_escape(&name),
                doc_html(&f.doc, names),
            )
            .unwrap();
        }
        body.push_str("</tbody></table>");
    }
    for n in &e.notes {
        writeln!(body, r#"<p class="entry-note">{}</p>"#, doc_html(n, names)).unwrap();
    }
    format!(
        r##"<div class="entry" id="{id}">
  <div class="entry-header">
    <span class="entry-kind {kind_cls}">{kind_label}</span>
    <code class="entry-sig">{sig}</code>
    <span class="entry-tags">{tags}</span>
    <a class="permalink" href="#{id}" title="Link to this">#</a>
  </div>
  {body}
</div>"##,
        sig = markup_html(&e.sig),
    )
}

fn render_section_nav(s: &Section) -> String {
    let items: String = s
        .entries
        .iter()
        .map(|e| {
            format!(
                "<a class=\"nav-item\" href=\"#{}\">{}</a>",
                anchor(s.id, &e.name),
                html_escape(&e.name)
            )
        })
        .collect::<Vec<_>>()
        .join("\n");
    // Long lists start closed; searching opens them.
    let open = if s.entries.len() <= 25 { " open" } else { "" };
    format!(
        "<details class=\"nav-section\"{open}>\n\
         <summary><a href=\"#{id}\">{title}</a><span class=\"count\">{n}</span></summary>\n\
         <div class=\"nav-items\">{items}</div>\n\
         </details>",
        id = s.id,
        title = s.title,
        n = s.entries.len(),
    )
}

fn render_section_html(s: &Section, names: &Names) -> String {
    let entries: String = s
        .entries
        .iter()
        .map(|e| render_entry_html(s.id, e, names))
        .collect::<Vec<_>>()
        .join("\n");
    format!(
        r#"<section id="{id}">
  <h2>{title}</h2>
  <p class="section-desc">{desc}</p>
  <div class="entries">{entries}</div>
</section>"#,
        id = s.id,
        title = s.title,
        desc = doc_html(s.desc, names),
    )
}

const OVERVIEW: &str = r##"<section id="overview">
  <h2>Overview</h2>
  <p>A plugin is a Lua file. Define <a href="#callbacks">callbacks</a> as global functions and the server calls them; use the <a href="#server"><code>server</code></a> global to act. The <a href="sauer.lua">sauer.lua</a> stubs give your editor the same types.</p>
<pre><code class="language-lua">function on_client_welcomed(id)
  local p = server:player(id)
  server:send_serv_msg(id, { text = ui.header("Welcome, " .. p.name) })
end

-- Packet hooks get the packet and can stop the server from handling it.
function edit_mode(id, packet, event)
  if not packet.is_editing then return end
  if server:player(id).privilege == "None" then event:cancel() end
end</code></pre>
  <h3>Things to know</h3>
  <ul>
    <li><b>Players have two numbers.</b> Callbacks and <code>server</code> methods take <code>id</code>, the peer id the server uses. Packets carry a <a href="#types-Cn"><code>Cn</code></a>, the client number players see. Convert with <a href="#server-get_cn"><code>server:get_cn</code></a> and <a href="#server-get_id"><code>server:get_id</code></a>.</li>
    <li><b>Packets in hooks are read-only.</b> Read their fields; cancel the <a href="#objects-Event"><code>Event</code></a> to skip the server's own handling. Only the <code>Chat</code>, <code>DamageInfo</code> and <code>Loadout</code> objects can be changed.</li>
    <li><b>Packets you send are tables</b> with the fields listed for the packet. Some <code>send_*</code> methods relay a packet as if a player sent it; their table also needs <code>client_num</code>, that player's <code>Cn</code>.</li>
    <li><b>Enums are strings</b> of the value names, like <code>"Pistol"</code> for a <a href="#enums-Gun"><code>Gun</code></a>.</li>
    <li><b>Types:</b> <code>T|nil</code> may be nil, <code>T[]</code> is a list (a table indexed from 1), <a href="#types-Vec3"><code>Vec3</code></a> and other types link to their fields.</li>
  </ul>
</section>"##;

fn render_html(sections: &[Section], names: &Names) -> String {
    let nav: String = sections
        .iter()
        .map(render_section_nav)
        .collect::<Vec<_>>()
        .join("\n");
    let body: String = sections
        .iter()
        .map(|s| render_section_html(s, names))
        .collect::<Vec<_>>()
        .join("\n");

    format!(
        r##"<!DOCTYPE html>
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
  --purple:       #bc8cff;
  --sidebar-w:    280px;
  --font-mono:    'SFMono-Regular', Consolas, 'Liberation Mono', Menlo, monospace;
}}

*, *::before, *::after {{ box-sizing: border-box; margin: 0; padding: 0; }}
html {{ scroll-behavior: smooth; scrollbar-color: #30363d transparent; }}
#sidebar {{ scrollbar-width: thin; scrollbar-color: #30363d transparent; }}

body {{
  background: var(--bg);
  color: var(--text);
  font-family: -apple-system, BlinkMacSystemFont, 'Segoe UI', Helvetica, Arial, sans-serif;
  font-size: 14px;
  line-height: 1.6;
  display: flex;
  min-height: 100vh;
}}

a {{ color: var(--accent); text-decoration: none; }}
a:hover {{ text-decoration: underline; }}
code {{ font-family: var(--font-mono); }}

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
#sidebar-header {{ padding: 20px 16px 12px; border-bottom: 1px solid var(--border); }}
#sidebar-header h1 {{ font-size: 15px; font-weight: 600; margin-bottom: 4px; }}
#sidebar-header .subtitle {{ font-size: 11px; color: var(--text-muted); }}

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
}}
#search:focus {{ border-color: var(--accent); }}
#search::placeholder {{ color: var(--text-muted); }}

#nav {{ padding: 4px 0 16px; }}
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
.nav-section > summary::before {{ content: '▸'; font-size: 9px; transition: transform .15s; }}
.nav-section[open] > summary::before {{ transform: rotate(90deg); }}
.nav-section > summary a {{ color: var(--text-muted); }}
.nav-section > summary:hover, .nav-section > summary:hover a {{ color: var(--text); }}
.nav-section > summary::-webkit-details-marker {{ display: none; }}
.nav-section .count {{ margin-left: auto; font-weight: 400; }}
.nav-overview {{ display: block; padding: 6px 16px; font-size: 11px; font-weight: 600; text-transform: uppercase; letter-spacing: .06em; color: var(--text-muted); }}

.nav-item {{
  display: block;
  padding: 3px 16px 3px 28px;
  color: var(--text-muted);
  font-size: 12.5px;
  font-family: var(--font-mono);
  white-space: nowrap;
  overflow: hidden;
  text-overflow: ellipsis;
}}
.nav-item:hover {{ background: var(--bg-card-hover); color: var(--accent); text-decoration: none; }}
.nav-item.current {{ color: var(--accent); }}
.nav-item.hidden {{ display: none; }}

/* ── Main content ── */
#main {{
  margin-left: var(--sidebar-w);
  flex: 1;
  min-width: 0;
  padding: 40px 48px 80px;
  max-width: 1000px;
}}
#page-title {{ font-size: 28px; font-weight: 700; margin-bottom: 8px; }}
#page-subtitle {{ color: var(--text-muted); margin-bottom: 40px; }}

section {{ margin-bottom: 56px; }}
section h2 {{
  font-size: 20px;
  font-weight: 600;
  padding-bottom: 10px;
  border-bottom: 1px solid var(--border);
  margin-bottom: 8px;
  scroll-margin-top: 24px;
}}
section h3 {{ font-size: 15px; margin: 18px 0 6px; }}
#overview p, #overview li {{ color: #c9d1d9; margin-bottom: 8px; }}
#overview ul {{ padding-left: 20px; }}
#overview pre {{ background: var(--bg-card); border: 1px solid var(--border); border-radius: 8px; padding: 12px 16px; margin: 12px 0; overflow-x: auto; font-size: 13px; }}
.section-desc {{ color: var(--text-muted); font-size: 13px; margin-bottom: 20px; }}

/* ── Entry cards ── */
.entries {{ display: flex; flex-direction: column; gap: 2px; }}
.entry {{
  background: var(--bg-card);
  border: 1px solid var(--border);
  border-radius: 8px;
  padding: 14px 18px;
  scroll-margin-top: 24px;
}}
.entry:hover {{ border-color: #444c56; background: var(--bg-card-hover); }}
.entry:target {{ border-color: var(--accent); }}
.entry.hidden {{ display: none; }}

.entry-header {{ display: flex; align-items: center; gap: 10px; flex-wrap: wrap; }}
.entry-kind {{
  font-size: 10px;
  font-weight: 700;
  text-transform: uppercase;
  letter-spacing: .08em;
  padding: 2px 7px;
  border-radius: 4px;
  flex-shrink: 0;
}}
.kind-fn, .kind-method {{ background: #1a3a5c; color: #58a6ff; }}
.kind-const    {{ background: #2d1c4e; color: #bc8cff; }}
.kind-field    {{ background: #162a1e; color: #3fb950; }}
.kind-callback {{ background: #3b2000; color: #ffa657; }}
.kind-packet   {{ background: #1c2940; color: #79c0ff; }}
.kind-type     {{ background: #162a1e; color: #3fb950; }}
.kind-enum     {{ background: #2d1c4e; color: #bc8cff; }}

.entry-sig {{ font-size: 13.5px; flex: 1; min-width: 0; overflow-wrap: anywhere; }}
.entry-tags {{ display: flex; gap: 5px; flex-wrap: wrap; }}
.permalink {{ color: var(--text-muted); opacity: 0; font-size: 13px; }}
.entry:hover .permalink {{ opacity: 1; }}

.tag {{ font-size: 10px; font-weight: 600; padding: 2px 7px; border-radius: 10px; letter-spacing: .04em; }}
.tag-cancel  {{ background: #3b1e1e; color: #f85149; }}
.tag-packet  {{ background: #1c2940; color: #79c0ff; }}
.tag-async   {{ background: #1a2d1a; color: #56d364; }}
.tag-rw      {{ background: #1a2d1a; color: #56d364; }}
.tag-warn    {{ background: #3b2e00; color: #d29922; }}
.tag-id      {{ background: var(--bg); color: var(--text-muted); font-family: var(--font-mono); font-weight: 400; }}
.tag-default {{ background: var(--bg); color: var(--text-muted); }}

.entry-doc, .entry-note {{ margin-top: 8px; color: #b0bec5; font-size: 13px; line-height: 1.55; }}
.entry-note {{ color: var(--text-muted); }}
.entry-doc code, .entry-note code, .f-doc code, .section-desc code, #overview code {{
  font-size: 12px;
  background: rgba(255,255,255,.07);
  padding: 1px 5px;
  border-radius: 4px;
  color: var(--purple);
}}
#overview pre code {{ background: none; padding: 0; color: inherit; font-size: 13px; }}
a code {{ color: var(--accent) !important; }}

.fields {{ margin-top: 10px; border-collapse: collapse; width: 100%; font-size: 13px; }}
.fields td {{ padding: 4px 10px 4px 0; border-top: 1px solid var(--border); vertical-align: top; }}
.fields .f-name {{ white-space: nowrap; color: var(--text); }}
.fields .f-type {{ white-space: nowrap; color: #79c0ff; }}
.fields .f-doc {{ color: #b0bec5; width: 100%; }}

.hljs {{ background: transparent !important; padding: 0 !important; }}
::-webkit-scrollbar {{ width: 6px; height: 6px; }}
::-webkit-scrollbar-thumb {{ background: #30363d; border-radius: 3px; }}

@media (max-width: 720px) {{
  #sidebar {{ display: none; }}
  #main {{ margin-left: 0; padding: 24px 16px 60px; }}
  .fields .f-type {{ white-space: normal; }}
}}
</style>
</head>
<body>

<aside id="sidebar">
  <div id="sidebar-header">
    <h1>sauer lua api</h1>
    <div class="subtitle">Plugin scripting reference</div>
  </div>
  <input id="search" type="search" placeholder="Search names, fields, docs…" autocomplete="off" spellcheck="false">
  <nav id="nav">
    <a class="nav-overview" href="#overview">Overview</a>
    {nav}
  </nav>
</aside>

<main id="main">
  <h1 id="page-title">Lua Plugin API</h1>
  <p id="page-subtitle">Generated from the sauer-rs sources by <code>cargo xtask lua-docs</code>.</p>
  {overview}
  {body}
</main>

<script>
hljs.highlightAll();

const search = document.getElementById('search');
const details = [...document.querySelectorAll('.nav-section')];
const wasOpen = details.map(d => d.open);
search.addEventListener('input', () => {{
  const q = search.value.trim().toLowerCase();
  document.querySelectorAll('.entry').forEach(entry => {{
    entry.classList.toggle('hidden', !!q && !entry.textContent.toLowerCase().includes(q));
  }});
  document.querySelectorAll('.nav-item').forEach(a => {{
    const entry = document.getElementById(a.getAttribute('href').slice(1));
    a.classList.toggle('hidden', entry?.classList.contains('hidden') ?? false);
  }});
  details.forEach((d, i) => d.open = q ? true : wasOpen[i]);
}});

// Opening a link into a closed nav group opens it and shows the item.
function showCurrent() {{
  document.querySelectorAll('.nav-item.current').forEach(a => a.classList.remove('current'));
  const a = document.querySelector(`.nav-item[href="${{CSS.escape(location.hash)}}"]`);
  if (!a) return;
  a.classList.add('current');
  a.closest('details').open = true;
  a.scrollIntoView({{ block: 'nearest' }});
}}
window.addEventListener('hashchange', showCurrent);
showCurrent();
</script>
</body>
</html>
"##,
        cdn = HIGHLIGHT_CDN,
        overview = OVERVIEW,
    )
}

// ---------------------------------------------------------------------------
// EmmyLua stubs
// ---------------------------------------------------------------------------

/// Doc text for a stub comment: links become their labels.
fn stub_doc(out: &mut String, doc: &str) {
    if !doc.is_empty() {
        writeln!(out, "--- {}", plain(doc)).unwrap();
    }
}

/// Params and return type of a signature (links already stripped).
fn sig_parts(sig: &str) -> (Vec<(String, String)>, Option<String>) {
    let (Some(open), Some(close)) = (sig.find('('), sig.rfind(')')) else {
        return (Vec::new(), None);
    };
    let params = sig[open + 1..close]
        .split(", ")
        .filter_map(|p| {
            let (n, t) = p.split_once(": ")?;
            Some((n.trim().to_string(), t.trim().to_string()))
        })
        .collect();
    let ret = sig[close + 1..]
        .trim()
        .strip_prefix(':')
        .map(|r| r.trim().to_string());
    (params, ret)
}

fn stub_function(out: &mut String, e: &Entry, lua_name: &str) {
    stub_doc(out, &e.doc);
    for n in &e.notes {
        stub_doc(out, n);
    }
    let sig = plain(e.stub_sig.as_deref().unwrap_or(&e.sig));
    let (params, ret) = sig_parts(&sig);
    for (n, t) in &params {
        writeln!(out, "---@param {n} {t}").unwrap();
    }
    if let Some(r) = ret {
        writeln!(out, "---@return {r}").unwrap();
    }
    let names: Vec<&str> = params
        .iter()
        .map(|(n, _)| n.trim_end_matches('?'))
        .collect();
    writeln!(out, "function {lua_name}({}) end\n", names.join(", ")).unwrap();
}

fn stub_class(out: &mut String, e: &Entry) {
    stub_doc(out, &e.doc);
    writeln!(out, "---@class {}", e.name).unwrap();
    for f in e.fields.iter().filter(|f| !f.method) {
        let doc = if f.doc.is_empty() {
            String::new()
        } else {
            format!(" {}", plain(&f.doc))
        };
        writeln!(out, "---@field {} {}{doc}", f.name, plain(&f.ty)).unwrap();
    }
    writeln!(out, "local {} = {{}}\n", e.name.replace('.', "_")).unwrap();
    for f in e.fields.iter().filter(|f| f.method) {
        stub_doc(out, &f.doc);
        writeln!(
            out,
            "function {}:{}() end\n",
            e.name.replace('.', "_"),
            f.name
        )
        .unwrap();
    }
}

fn render_stubs(sections: &[Section], wrapped: &BTreeSet<String>) -> String {
    let mut out = String::new();
    writeln!(out, "---@meta").unwrap();
    writeln!(
        out,
        "-- EmmyLua stubs for the sauer Lua plugin API, made by `cargo xtask lua-docs`."
    )
    .unwrap();
    writeln!(
        out,
        "-- Put this file in your project and point lua-language-server at it.\n"
    )
    .unwrap();
    let section = |id: &str| sections.iter().find(|s| s.id == id).unwrap();

    writeln!(out, "-- ── Types ──\n").unwrap();
    for e in &section("enums").entries {
        stub_doc(&mut out, &e.doc);
        let values: Vec<String> = e
            .fields
            .iter()
            .map(|f| {
                if f.ty.is_empty() {
                    format!("\"{}\"", f.name)
                } else {
                    plain(&f.ty)
                }
            })
            .collect();
        writeln!(out, "---@alias {} {}\n", e.name, values.join("|")).unwrap();
    }
    for e in &section("types").entries {
        if e.fields.is_empty() && e.sig.contains(" = ") {
            stub_doc(&mut out, &e.doc);
            let (_, ty) = e.sig.split_once(" = ").unwrap();
            writeln!(out, "---@alias {} {}\n", e.name, plain(ty)).unwrap();
        } else {
            stub_class(&mut out, e);
        }
    }

    writeln!(out, "-- ── Objects ──\n").unwrap();
    for e in &section("objects").entries {
        if e.name == "ServerQueryResponse" {
            stub_doc(&mut out, &e.doc);
            writeln!(
                out,
                "---@alias ServerQueryResponse ServerInfoReply|UptimeReply|PlayerStatsReply|TeamScoreReply\n"
            )
            .unwrap();
        } else {
            stub_class(&mut out, e);
        }
    }

    writeln!(out, "-- ── Packets ──\n").unwrap();
    for e in &section("packets").entries {
        stub_class(&mut out, e);
        if wrapped.contains(&e.name) {
            writeln!(
                out,
                "--- `{0}` as a table to send in another player's name.\n---@class {0}.WithCn: {0}\n---@field client_num Cn The player it comes from.\n",
                e.name
            )
            .unwrap();
        }
    }

    writeln!(out, "-- ── Globals ──\n").unwrap();
    for e in &section("globals").entries {
        match e.kind {
            EntryKind::Const => {
                stub_doc(&mut out, &e.doc);
                writeln!(out, "---@type integer\n{} = 0\n", e.name).unwrap();
            }
            _ if e.name.starts_with("ui.") => {}
            _ => stub_function(&mut out, e, &e.name),
        }
    }
    writeln!(out, "ui = {{}}\n").unwrap();
    for e in section("globals")
        .entries
        .iter()
        .filter(|e| e.name.starts_with("ui."))
    {
        stub_function(&mut out, e, &e.name);
    }

    writeln!(out, "-- ── Server ──\n").unwrap();
    writeln!(out, "---@class Server").unwrap();
    writeln!(
        out,
        "---@field extra table Plugin-local key/value storage shared across the server."
    )
    .unwrap();
    writeln!(
        out,
        "local Server = {{}}\n\n---@type Server\nserver = {{}}\n"
    )
    .unwrap();
    for id in ["server", "server-send", "server-broadcast"] {
        for e in section(id)
            .entries
            .iter()
            .filter(|e| e.kind == EntryKind::Method)
        {
            stub_function(&mut out, e, &format!("Server:{}", e.name));
        }
    }

    writeln!(out, "-- ── Callbacks ──").unwrap();
    writeln!(
        out,
        "-- Define these as globals in your plugin file to receive events.\n"
    )
    .unwrap();
    for e in &section("callbacks").entries {
        stub_function(&mut out, e, &e.name);
    }
    out
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
    let task_src = read("sauer-plugin-lua/src/userdata/task_handle.rs");
    let plugin_src = read("sauer-server/src/plugin.rs");
    let (damage_src, chat_src) = hook_value_src
        .split_once("pub struct ChatUD")
        .expect("hook_value.rs: ChatUD not found");

    // ── Rust types ──
    let proto = |p: &str| read(&format!("sauer-proto/src/{p}"));
    let mut packets: HashMap<String, StructDef> = HashMap::new();
    // Message ids by packet name, for packets without a tag of their own.
    let mut msg_ids: HashMap<String, String> = HashMap::new();
    for p in [
        "messages/mod.rs",
        "messages/channel0.rs",
        "messages/channel1.rs",
        "messages/channel2.rs",
    ] {
        let src = proto(p);
        for e in parse_packet_enums(&src).into_iter().flat_map(|e| e.entries) {
            msg_ids.entry(e.name).or_insert(e.msg_id);
        }
        for s in parse_make_structs(&src)
            .into_iter()
            .chain(parse_manual_structs(&src))
        {
            if s.name != "Client" {
                packets.entry(s.name.clone()).or_insert(s);
            }
        }
    }
    let mut data_srcs = vec![proto("data.rs"), read("sauer-core/src/lib.rs")];
    let mut data_files: Vec<_> = fs::read_dir(root.join("sauer-proto/src/data"))
        .expect("read sauer-proto/src/data")
        .map(|e| e.unwrap().path())
        .collect();
    data_files.sort();
    for f in data_files {
        data_srcs.push(fs::read_to_string(f).unwrap());
    }

    let mut types = TypeMap {
        structs: HashMap::new(),
        enums: HashMap::new(),
        aliases: ALIASES.iter().map(|a| a.0).collect(),
        objects: ["TeamScore".to_string()].into(),
        display: HashMap::new(),
        used_structs: BTreeSet::new(),
        used_enums: BTreeSet::new(),
        used_aliases: BTreeSet::new(),
        unknown: BTreeSet::new(),
    };
    let mut struct_docs: HashMap<String, String> = HashMap::new();
    for src in &data_srcs {
        for s in parse_make_structs(src)
            .into_iter()
            .chain(parse_manual_structs(src))
        {
            struct_docs.entry(s.name.clone()).or_insert(s.doc.clone());
            types
                .structs
                .entry(s.name.clone())
                .or_insert((s.doc, s.fields));
        }
        for e in parse_make_enums(src)
            .into_iter()
            .chain(parse_manual_enums(src))
        {
            types.enums.entry(e.name.clone()).or_insert(e);
        }
    }
    for c in CUSTOM_TYPES {
        types
            .structs
            .insert(c.name.into(), (c.doc.into(), field_defs(c.fields)));
        // `UndoPayload` is a Rust enum, but opaque to Lua.
        types.enums.remove(c.name);
    }
    // A data type a packet shares its name with is shown as `data.Name`.
    for name in types.structs.keys() {
        if packets.contains_key(name)
            && packets[name].tag.is_some()
            && !struct_has_same_def(&packets[name], &types.structs[name].1)
        {
            types.display.insert(name.clone(), format!("data.{name}"));
        }
    }
    // Packets that are data types re-exported into a channel.
    for (name, (doc, fields)) in &types.structs {
        packets.entry(name.clone()).or_insert_with(|| StructDef {
            name: name.clone(),
            tag: None,
            doc: doc.clone(),
            fields: fields.clone(),
        });
    }

    // ── Packets ──
    let hooks = parse_packet_hooks(&plugin_src);
    let (methods, sends) = parse_server(&server_src);
    let mut packet_names: BTreeSet<String> = BTreeSet::new();
    packet_names.extend(hooks.iter().map(|h| h.packet.clone()));
    packet_names.extend(sends.iter().map(|s| s.packet.clone()));
    let wrapped: BTreeSet<String> = sends
        .iter()
        .filter(|s| s.wrapped)
        .map(|s| s.packet.clone())
        .collect();

    let id_prefix = Regex::new(r"^`N_\w+`\s*").unwrap();
    let mut packet_entries = Vec::new();
    for name in &packet_names {
        let def = packets
            .get(name)
            .unwrap_or_else(|| panic!("packet `{name}` not found in sauer-proto"));
        let msg_id = def.tag.clone().or_else(|| msg_ids.get(name).cloned());
        let doc = id_prefix.replace(&def.doc, "").into_owned();
        let fields = types.fields(&def.fields);
        let mut tags: Vec<String> = msg_id.into_iter().collect();
        let mut notes = Vec::new();
        let received: Vec<String> = hooks
            .iter()
            .filter(|h| &h.packet == name)
            .map(|h| link(&anchor("callbacks", &h.hook), &h.hook))
            .collect();
        if !received.is_empty() {
            tags.push("received".into());
            notes.push(format!("Received in: {}", received.join(", ")));
        }
        let sent: Vec<String> = sends
            .iter()
            .filter(|s| &s.packet == name)
            .map(|s| {
                let section = if s.broadcast {
                    "server-broadcast"
                } else {
                    "server-send"
                };
                link(&anchor(section, &s.name), &s.name)
            })
            .collect();
        if !sent.is_empty() {
            tags.push("sendable".into());
            notes.push(format!("Sent with: {}", sent.join(", ")));
        }
        if wrapped.contains(name) {
            notes.push(
                "When sending, the table also needs `client_num`: the `Cn` of the player it comes from."
                    .into(),
            );
        }
        if fields
            .iter()
            .any(|f| f.ty.contains("ClipboardPayload") || f.ty.contains("UndoPayload"))
            && !sent.is_empty()
        {
            notes
                .push("Sending this from Lua fails: its block can't be built from a table.".into());
        }
        packet_entries.push(Entry {
            name: name.clone(),
            kind: EntryKind::Packet,
            sig: name.clone(),
            doc,
            tags,
            fields,
            notes,
            ..Default::default()
        });
    }

    // ── Objects (hook values and tables the API hands out) ──
    let rust_ty_of_spawn = |name: &str| match name {
        "armour_type" => "ArmourType",
        "gun_select" => "Gun",
        _ => "i32",
    };
    let object = |types: &mut TypeMap, name: &str, doc: &str, rw: bool, fields: Vec<FieldDef>| {
        let fields = types.fields(&fields);
        Entry {
            name: name.into(),
            kind: EntryKind::Type,
            sig: name.into(),
            doc: doc.into(),
            tags: if rw {
                vec!["read/write".into()]
            } else {
                vec![]
            },
            fields,
            ..Default::default()
        }
    };
    let getters = |src: &str, ty: &dyn Fn(&str) -> &'static str| -> Vec<FieldDef> {
        parse_getters(src)
            .into_iter()
            .map(|(name, doc)| FieldDef {
                ty: ty(&name).into(),
                name,
                doc: doc.trim_end_matches("(read/write)").trim().to_string(),
            })
            .collect()
    };
    let method_fields = |src: &str| -> Vec<FieldDoc> {
        parse_methods(src)
            .into_iter()
            .map(|(name, doc)| FieldDoc {
                name,
                doc,
                method: true,
                ..Default::default()
            })
            .collect()
    };
    let objects = vec![
        object(
            &mut types,
            "Loadout",
            "Passed to `on_spawn`: what the player spawns with. Change fields to change it.",
            true,
            getters(&spawn_src, &rust_ty_of_spawn),
        ),
        object(
            &mut types,
            "Chat",
            "Passed to `on_chat` and `on_team_chat`. Change `text` to change what is sent.",
            true,
            getters(chat_src, &|_| "CubeString"),
        ),
        object(
            &mut types,
            "DamageInfo",
            "Passed to `on_damage`. Change `amount` to change the damage dealt.",
            true,
            getters(damage_src, &|_| "i32"),
        ),
        Entry {
            name: "Event".into(),
            kind: EntryKind::Type,
            sig: "Event".into(),
            doc: "Passed as the last argument to cancellable callbacks.".into(),
            fields: method_fields(&event_src),
            ..Default::default()
        },
        Entry {
            name: "TaskHandle".into(),
            kind: EntryKind::Type,
            sig: "TaskHandle".into(),
            doc: "Returned by `set_interval` and `set_timeout`.".into(),
            fields: method_fields(&task_src),
            ..Default::default()
        },
        object(
            &mut types,
            "Player",
            "Returned by `server:player`: a snapshot of the player when it was called.",
            false,
            field_defs(&[
                ("name", "CubeString", ""),
                ("team", "CubeString", ""),
                ("model", "PlayerModel", ""),
                ("privilege", "Privilege", ""),
                ("state", "ClientState", ""),
                ("frags", "i32", ""),
                ("deaths", "i32", ""),
                ("flags", "i32", "Flags scored."),
                ("ping", "i32", ""),
            ]),
        ),
        object(
            &mut types,
            "ServerQueryRequest",
            "Passed to `server_query`: what a server browser asked for.",
            false,
            field_defs(&[
                (
                    "type",
                    "CubeString",
                    "`\"ServerInfo\"`, `\"Uptime\"`, `\"PlayerStats\"` or `\"TeamScore\"`.",
                ),
                (
                    "ping_id",
                    "Option<i32>",
                    "`ServerInfo` only. Send it back in the reply.",
                ),
                (
                    "client_number",
                    "Option<i32>",
                    "`PlayerStats` only: the player asked about, nil for all.",
                ),
            ]),
        ),
        Entry {
            name: "ServerQueryResponse".into(),
            kind: EntryKind::Type,
            sig: "ServerQueryResponse".into(),
            doc: "A reply for `send_server_query`: one of the tables below, picked by its `type`."
                .into(),
            notes: vec![format!(
                "One of: {}",
                [
                    "ServerInfoReply",
                    "UptimeReply",
                    "PlayerStatsReply",
                    "TeamScoreReply"
                ]
                .map(|n| link(&anchor("objects", n), n))
                .join(", ")
            )],
            ..Default::default()
        },
        object(
            &mut types,
            "ServerInfoReply",
            "Answers a `ServerInfo` query. `type` is `\"ServerInfo\"`. Modes are numbers here, as the server browser expects.",
            false,
            field_defs(&[
                ("type", "CubeString", "`\"ServerInfo\"`"),
                ("ping_id", "i32", "From the request."),
                ("num_clients", "i32", ""),
                ("protocol_version", "i32", "Usually `PROTOCOL_VERSION`."),
                ("game_mode", "i32", ""),
                ("remaining_time", "i32", "Seconds."),
                ("max_clients", "i32", ""),
                ("master_mode", "i32", ""),
                ("is_paused", "bool", ""),
                ("game_speed", "i32", "Percent."),
                ("map_name", "CubeString", ""),
                ("server_desc", "CubeString", ""),
            ]),
        ),
        object(
            &mut types,
            "UptimeReply",
            "Answers an `Uptime` query.",
            false,
            field_defs(&[
                ("type", "CubeString", "`\"Uptime\"`"),
                ("seconds", "i32", ""),
            ]),
        ),
        object(
            &mut types,
            "PlayerStatsReply",
            "One packet of a `PlayerStats` reply. A full reply is a `PlayerStatsIds` packet and then one `PlayerStatsStats` per player, or a lone `PlayerStatsError` when the asked player isn't there.",
            false,
            field_defs(&[
                (
                    "type",
                    "CubeString",
                    "`\"PlayerStatsIds\"`, `\"PlayerStatsStats\"` or `\"PlayerStatsError\"`.",
                ),
                (
                    "client_number",
                    "Option<i32>",
                    "From the request, every packet.",
                ),
                (
                    "ids",
                    "Option<Vec<i32>>",
                    "`PlayerStatsIds`: the cns that follow.",
                ),
                (
                    "client_num",
                    "Option<i32>",
                    "`PlayerStatsStats` from here on.",
                ),
                ("ping", "Option<i32>", ""),
                ("name", "Option<CubeString>", ""),
                ("team", "Option<CubeString>", ""),
                ("frags", "Option<i32>", ""),
                ("flags", "Option<i32>", ""),
                ("deaths", "Option<i32>", ""),
                ("teamkills", "Option<i32>", ""),
                ("accuracy", "Option<i32>", "Percent."),
                ("health", "Option<i32>", ""),
                ("armour", "Option<i32>", ""),
                ("gunselect", "Option<Gun>", ""),
                ("privilege", "Option<Privilege>", ""),
                ("state", "Option<ClientState>", ""),
                (
                    "ip",
                    "Option<[u8; 3]>",
                    "First three bytes of the IPv4 address; zeros if nil.",
                ),
            ]),
        ),
        object(
            &mut types,
            "TeamScoreReply",
            "Answers a `TeamScore` query.",
            false,
            field_defs(&[
                ("type", "CubeString", "`\"TeamScore\"`"),
                ("game_mode", "i32", ""),
                ("remaining_time", "i32", "Seconds."),
                (
                    "scores",
                    "Option<Vec<TeamScore>>",
                    "Nil when the mode has no teams.",
                ),
            ]),
        ),
        object(
            &mut types,
            "TeamScore",
            "A team in a `TeamScoreReply`.",
            false,
            field_defs(&[
                ("team_name", "CubeString", ""),
                ("score", "i32", ""),
                (
                    "bases",
                    "Option<Vec<i32>>",
                    "Bases held, capture modes only.",
                ),
            ]),
        ),
    ];
    // ── Data types, walking outward from what packets and objects use ──
    // `Cn` is always listed: the overview and the server methods refer to it.
    types.used_aliases.insert("Cn".into());
    let mut type_entries: BTreeMap<String, Entry> = BTreeMap::new();
    loop {
        let structs: Vec<String> = types
            .used_structs
            .iter()
            .filter(|n| !type_entries.contains_key(&types.shown(n)))
            .cloned()
            .collect();
        let aliases: Vec<String> = types
            .used_aliases
            .iter()
            .filter(|n| !type_entries.contains_key(*n))
            .cloned()
            .collect();
        if structs.is_empty() && aliases.is_empty() {
            break;
        }
        for name in structs {
            let (doc, fields) = types.structs[&name].clone();
            let fields = types.fields(&fields);
            let shown = types.shown(&name);
            type_entries.insert(
                shown.clone(),
                Entry {
                    name: shown.clone(),
                    kind: EntryKind::Type,
                    sig: shown,
                    doc,
                    fields,
                    ..Default::default()
                },
            );
        }
        for name in aliases {
            let &(_, doc, variants) = ALIASES.iter().find(|a| a.0 == name).unwrap();
            let ty = variants
                .iter()
                .map(|v| types.lua(v))
                .collect::<Vec<_>>()
                .join("|");
            type_entries.insert(
                name.clone(),
                Entry {
                    sig: format!("{name} = {ty}"),
                    name,
                    kind: EntryKind::Type,
                    doc: doc.into(),
                    ..Default::default()
                },
            );
        }
    }

    // ── Enums ──
    let mut enum_names = types.used_enums.clone();
    // The ones the server API uses outside packets.
    enum_names.extend(
        [
            "GameMode",
            "MasterMode",
            "Privilege",
            "PlayerModel",
            "ClientState",
            "Gun",
            "ArmourType",
        ]
        .map(String::from),
    );
    let enum_entries: Vec<Entry> = enum_names
        .iter()
        .map(|name| {
            let e = types
                .enums
                .get(name)
                .unwrap_or_else(|| panic!("enum `{name}` not found"));
            let fields = e
                .variants
                .iter()
                .map(|(v, val, doc)| {
                    // `GameSpecific(i32)`: any other value, as its number.
                    let numeric = !val.is_empty() && val.parse::<i64>().is_err();
                    FieldDoc {
                        name: v.clone(),
                        ty: if numeric {
                            "integer".into()
                        } else {
                            String::new()
                        },
                        doc: if numeric {
                            "Any other value, as its number.".to_string()
                        } else {
                            doc.clone()
                        },
                        method: false,
                    }
                })
                .collect();
            Entry {
                name: name.clone(),
                kind: EntryKind::Enum,
                sig: name.clone(),
                doc: e.doc.clone(),
                fields,
                ..Default::default()
            }
        })
        .collect();

    assert!(
        types.unknown.is_empty(),
        "no Lua type for {:?}; add it to lua_docs.rs",
        types.unknown
    );

    // ── Names for `@Name` and doc links ──
    let mut names = Names::default();
    for o in &objects {
        names.add(&o.name, anchor("objects", &o.name));
    }
    for e in &packet_entries {
        names.add(&e.name, anchor("packets", &e.name));
    }
    for e in type_entries.values() {
        names.add(&e.name, anchor("types", &e.name));
    }
    for e in &enum_entries {
        names.add(&e.name, anchor("enums", &e.name));
    }
    for c in SPECIAL_CALLBACKS {
        names.add(c.name, anchor("callbacks", c.name));
    }
    for h in &hooks {
        names.add(&h.hook, anchor("callbacks", &h.hook));
    }
    for (m, _) in &methods {
        names.add(m, anchor("server", m));
    }
    for s in &sends {
        let section = if s.broadcast {
            "server-broadcast"
        } else {
            "server-send"
        };
        names.add(&s.name, anchor(section, &s.name));
    }

    // ── Callbacks ──
    let mut callbacks: Vec<Entry> = SPECIAL_CALLBACKS
        .iter()
        .map(|d| Entry {
            name: d.name.into(),
            kind: EntryKind::Callback,
            doc: d.doc.into(),
            sig: names.resolve(d.sig),
            tags: if d.cancellable {
                vec!["cancellable".into()]
            } else {
                vec![]
            },
            ..Default::default()
        })
        .collect();
    callbacks.extend(hooks.iter().map(|h| Entry {
        name: h.hook.clone(),
        kind: EntryKind::Callback,
        doc: format!(
            "Called when a client sends `{}`. Cancel to skip the server's own handling.",
            h.msg_id
        ),
        sig: names.resolve(&format!(
            "{}(id: integer, packet: {}, event: @Event)",
            h.hook,
            link(&anchor("packets", &h.packet), &h.packet)
        )),
        tags: vec!["cancellable".into(), "packet".into()],
        ..Default::default()
    }));

    // ── Server ──
    let mut server_entries = vec![Entry {
        name: "extra".into(),
        kind: EntryKind::Field,
        doc: "Plugin-local key/value storage table, shared across the server instance.".into(),
        sig: "server.extra: table".into(),
        ..Default::default()
    }];
    let mut missing = Vec::new();
    for (name, doc) in &methods {
        let Some(sig) = server_sig(name) else {
            missing.push(name.clone());
            continue;
        };
        server_entries.push(Entry {
            name: name.clone(),
            kind: EntryKind::Method,
            doc: doc.clone(),
            sig: names.resolve(sig),
            tags: if name.starts_with("set_interval") || name.starts_with("set_timeout") {
                vec!["async".into()]
            } else {
                vec![]
            },
            ..Default::default()
        });
    }
    assert!(
        missing.is_empty(),
        "no signature for server methods {missing:?}"
    );

    let send_entry = |s: &SendMethod| {
        let sig = |packet: &str| {
            if s.broadcast {
                format!("server:{}(packet: {packet})", s.name)
            } else {
                format!("server:{}(id: integer, packet: {packet})", s.name)
            }
        };
        let stub_sig = s.wrapped.then(|| sig(&format!("{}.WithCn", s.packet)));
        let sig = sig(&link(&anchor("packets", &s.packet), &s.packet));
        let doc = if !s.doc.is_empty() {
            s.doc.clone()
        } else if s.broadcast {
            format!("Sends `{}` to every joined client.", s.packet)
        } else {
            format!("Sends `{}` to one client.", s.packet)
        };
        Entry {
            name: s.name.clone(),
            kind: EntryKind::Method,
            doc,
            sig,
            stub_sig,
            tags: if s.wrapped {
                vec!["needs client_num".into()]
            } else {
                vec![]
            },
            ..Default::default()
        }
    };
    let send_entries: Vec<Entry> = sends
        .iter()
        .filter(|s| !s.broadcast)
        .map(send_entry)
        .collect();
    let broadcast_entries: Vec<Entry> = sends
        .iter()
        .filter(|s| s.broadcast)
        .map(send_entry)
        .collect();

    let sections = vec![
        Section {
            id: "globals",
            title: "Globals",
            desc: "Functions and constants in every plugin's environment.",
            entries: parse_globals(&globals_src),
        },
        Section {
            id: "callbacks",
            title: "Callbacks",
            desc: "Define these as global functions in your plugin; the server calls them on the matching event. Cancellable ones get an `Event` last.",
            entries: callbacks,
        },
        Section {
            id: "server",
            title: "Server",
            desc: "The `server` global, ready from `start` on.",
            entries: server_entries,
        },
        Section {
            id: "server-send",
            title: "Server: send",
            desc: "Send a packet to one client. The packet is a table with the packet's fields. The ones tagged needs client_num relay a packet in another player's name.",
            entries: send_entries,
        },
        Section {
            id: "server-broadcast",
            title: "Server: broadcast",
            desc: "Send a packet to every client that finished joining.",
            entries: broadcast_entries,
        },
        Section {
            id: "objects",
            title: "Objects",
            desc: "Values the API hands to callbacks or returns from methods.",
            entries: objects,
        },
        Section {
            id: "packets",
            title: "Packets",
            desc: "Every packet a hook receives or a `send_*` method takes. In hooks they're read-only; to send one, pass a table with the same fields.",
            entries: packet_entries,
        },
        Section {
            id: "types",
            title: "Types",
            desc: "Types used inside packets.",
            entries: type_entries.into_values().collect(),
        },
        Section {
            id: "enums",
            title: "Enums",
            desc: "Enum values are strings of the value names.",
            entries: enum_entries,
        },
    ];

    fs::create_dir_all(out_dir)
        .unwrap_or_else(|e| panic!("cannot create {}: {e}", out_dir.display()));
    for (name, contents) in [
        ("index.html", render_html(&sections, &names)),
        ("sauer.lua", render_stubs(&sections, &wrapped)),
    ] {
        let path = out_dir.join(name);
        fs::write(&path, &contents)
            .unwrap_or_else(|e| panic!("cannot write {}: {e}", path.display()));
        eprintln!("Written {} bytes → {}", contents.len(), path.display());
    }
}

/// Whether a packet and a data type of the same name have the same fields,
/// as when the data type is re-exported as the packet.
fn struct_has_same_def(packet: &StructDef, fields: &[FieldDef]) -> bool {
    packet.fields.len() == fields.len()
        && packet
            .fields
            .iter()
            .zip(fields)
            .all(|(a, b)| a.name == b.name && a.ty == b.ty)
}
