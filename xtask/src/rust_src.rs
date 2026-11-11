//! Parsers for the Rust sources the doc generators read: `make_struct!`,
//! `make_enum!` and `make_packet_enum!` calls, and plain `pub struct`/`pub
//! enum` items.

use regex::Regex;

#[derive(Debug, Clone, Default)]
pub struct StructDef {
    pub name: String,
    pub tag: Option<String>,
    pub doc: String,
    pub fields: Vec<FieldDef>,
}

#[derive(Debug, Clone, Default)]
pub struct FieldDef {
    pub name: String,
    pub ty: String,
    pub doc: String,
}

#[derive(Debug, Clone, Default)]
pub struct EnumDef {
    pub name: String,
    pub doc: String,
    pub repr: String,
    pub variants: Vec<(String, String, String)>, // (variant, value, doc)
}

#[derive(Debug, Clone)]
pub struct PacketEntry {
    pub msg_id: String,
    pub name: String,
    pub client_wrapped: bool,
    pub reliable: bool,
}

#[derive(Debug, Clone)]
pub struct PacketEnum {
    pub route: String,
    pub entries: Vec<PacketEntry>,
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
        let Some(rel) = src[pos..].find(&pat) else {
            break;
        };
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

pub fn parse_packet_enums(src: &str) -> Vec<PacketEnum> {
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
                    msg_id: cap[1].to_string(),
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

pub fn parse_make_structs(src: &str) -> Vec<StructDef> {
    let bodies = extract_macro_bodies(src, "make_struct!");
    let tag_re = Regex::new(r"^\[(\w+)\]\s+(\w+)\s*\{([\s\S]*)\}").unwrap();
    let notag_re = Regex::new(r"^(\w+)\s*\{([\s\S]*)\}").unwrap();
    // Field: word: type (stopping at => or , or end of content)
    let field_re =
        Regex::new(r"(?m)^\s+(\w+):\s*([\w\[\]<>, :;*&']+?)(?:\s*=>|\s*,|\s*$)").unwrap();

    bodies
        .iter()
        .filter_map(|body| {
            let (doc, rest) = extract_leading_doc(body);
            let rest = rest.trim().to_string();

            let (tag, name, fields_str) = if let Some(cap) = tag_re.captures(&rest) {
                (
                    Some(cap[1].to_string()),
                    cap[2].to_string(),
                    cap[3].to_string(),
                )
            } else {
                let cap = notag_re.captures(&rest)?;
                (None, cap[1].to_string(), cap[2].to_string())
            };

            let mut fields = Vec::new();
            let mut field_doc = Vec::new();
            for line in fields_str.lines() {
                if let Some(d) = line.trim().strip_prefix("///") {
                    field_doc.push(d.trim().to_string());
                    continue;
                }
                let Some(cap) = field_re.captures(line) else {
                    if !line.trim().starts_with("#[") {
                        field_doc.clear();
                    }
                    continue;
                };
                let fname = cap[1].trim().to_string();
                let ftype = cap[2].trim().trim_end_matches(',').trim().to_string();
                if !fname.is_empty() && fname != "fn" && fname != "pub" {
                    fields.push(FieldDef {
                        name: fname,
                        ty: ftype,
                        doc: field_doc.join(" "),
                    });
                }
                field_doc.clear();
            }

            Some(StructDef {
                name,
                tag,
                doc,
                fields,
            })
        })
        .collect()
}

// ---------------------------------------------------------------------------
// make_enum! parsing
// ---------------------------------------------------------------------------

pub fn parse_make_enums(src: &str) -> Vec<EnumDef> {
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

            Some(EnumDef {
                name,
                doc,
                repr,
                variants,
            })
        })
        .collect()
}

// ---------------------------------------------------------------------------
// Manual struct/enum parsing (pub struct/enum defined without macros)
// ---------------------------------------------------------------------------

pub fn parse_manual_structs(src: &str) -> Vec<StructDef> {
    // Grab outer doc, then `pub struct Name<...> {`
    let block_re = Regex::new(r"(?m)^pub struct (\w+)(?:<[^>]+>)?\s*\{([^}]*)\}").unwrap();
    let field_re = Regex::new(r"(?m)^\s+pub (\w+):\s*([\w\[\]<>, :;*&']+)").unwrap();

    let mut results = Vec::new();

    for cap in block_re.captures_iter(src) {
        let name = cap[1].to_string();
        let body = &cap[2];

        // Extract doc comment that precedes this struct in the source
        let struct_pos = cap.get(0).unwrap().start();
        let prefix = &src[..struct_pos];
        let doc = extract_block_doc(prefix);

        let mut fields = Vec::new();
        let mut field_doc = Vec::new();
        for line in body.lines() {
            if let Some(d) = line.trim().strip_prefix("///") {
                field_doc.push(d.trim().to_string());
            } else if let Some(fc) = field_re.captures(line) {
                let ty = fc[2].trim().trim_end_matches([',', ';']).trim().to_string();
                fields.push(FieldDef {
                    name: fc[1].to_string(),
                    ty,
                    doc: field_doc.join(" "),
                });
                field_doc.clear();
            } else if !line.trim().starts_with("#[") {
                field_doc.clear();
            }
        }

        results.push(StructDef {
            name,
            tag: None,
            doc,
            fields,
        });
    }
    results
}

pub fn parse_manual_enums(src: &str) -> Vec<EnumDef> {
    let block_re = Regex::new(r"(?m)^pub enum (\w+)\s*\{([^}]*)\}").unwrap();
    let tuple_variant_re = Regex::new(r"^(\w+)\(([^)]+)\)").unwrap();
    let value_variant_re = Regex::new(r"^(\w+)\s*=\s*(-?\d+)").unwrap();
    let unit_variant_re = Regex::new(r"^(\w+)\s*,?$").unwrap();
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
        let repr = pre_lines
            .iter()
            .find_map(|l| repr_re.captures(l).map(|c| c[1].to_string()))
            .unwrap_or_default();

        let mut variants = Vec::new();
        let mut pending_doc = String::new();
        for line in body.lines() {
            // Strip #[...] attribute prefixes
            let t = line.trim();
            let t = if t.starts_with("#[") {
                t.split_once(']').map_or("", |(_, r)| r).trim()
            } else {
                t
            };
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

        results.push(EnumDef {
            name,
            doc,
            repr,
            variants,
        });
    }
    results
}

/// Extract the doc comment immediately preceding a position in source.
pub fn extract_block_doc(prefix: &str) -> String {
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
