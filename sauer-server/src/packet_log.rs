use std::collections::HashSet;
use std::fmt;
use std::sync::OnceLock;

use sauer_proto::PacketDescriptor;

const DEFAULT_FILTER: &str = "*,-N_POS,-N_PING,-N_CLIENTPING";
const ENV_VAR: &str = "SAUER_PACKET_LOG";

// ── Truncated debug output ────────────────────────────────────────────────────

// First N and last M bytes of a Debug representation; middle is replaced with
// "…[K chars omitted]…" if the output exceeds HEAD + TAIL bytes.
const MAX_HEAD: usize = 512;
const MAX_TAIL: usize = 128;

struct LimitedWriter {
    head: String,
    tail: String,
    overflow: usize,
}

impl LimitedWriter {
    fn new() -> Self {
        Self {
            head: String::with_capacity(MAX_HEAD),
            tail: String::new(),
            overflow: 0,
        }
    }

    fn into_string(self) -> String {
        if self.tail.is_empty() {
            self.head
        } else if self.overflow == 0 {
            self.head + &self.tail
        } else {
            format!(
                "{}…[{} chars omitted]…{}",
                self.head, self.overflow, self.tail
            )
        }
    }
}

// Largest char boundary index that is <= max.
fn char_boundary_le(s: &str, max: usize) -> usize {
    if max >= s.len() {
        return s.len();
    }
    let mut i = max;
    while !s.is_char_boundary(i) {
        i -= 1;
    }
    i
}

// Smallest char boundary index that is >= min.
fn char_boundary_ge(s: &str, min: usize) -> usize {
    let mut i = min;
    while i < s.len() && !s.is_char_boundary(i) {
        i += 1;
    }
    i
}

impl fmt::Write for LimitedWriter {
    fn write_str(&mut self, s: &str) -> fmt::Result {
        let remaining_head = MAX_HEAD.saturating_sub(self.head.len());
        let split = char_boundary_le(s, remaining_head);
        self.head.push_str(&s[..split]);

        let rest = &s[split..];
        if rest.is_empty() {
            return Ok(());
        }

        self.tail.push_str(rest);
        if self.tail.len() > MAX_TAIL {
            let excess = self.tail.len() - MAX_TAIL;
            let trim = char_boundary_ge(&self.tail, excess);
            self.overflow += trim;
            self.tail.drain(..trim);
        }
        Ok(())
    }
}

struct TruncatedDebug<'a, T: fmt::Debug + ?Sized>(&'a T);

impl<T: fmt::Debug + ?Sized> fmt::Debug for TruncatedDebug<'_, T> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let mut w = LimitedWriter::new();
        let _ = fmt::write(&mut w, format_args!("{:?}", self.0));
        f.write_str(&w.into_string())
    }
}

// ── Filter ────────────────────────────────────────────────────────────────────

enum FilterMode {
    AllExcept(HashSet<&'static str>),
    OnlyThese(HashSet<&'static str>),
}

struct PacketFilter(FilterMode);

impl PacketFilter {
    fn allows(&self, name: &str) -> bool {
        match &self.0 {
            FilterMode::AllExcept(ex) => !ex.contains(name),
            FilterMode::OnlyThese(inc) => inc.contains(name),
        }
    }
}

struct Filters {
    incoming: PacketFilter,
    outgoing: PacketFilter,
}

/// Parse `SAUER_PACKET_LOG` (or the default) into a pair of directional filters.
///
/// Token syntax (comma-separated):
/// - `*`         → match all packets, both directions
/// - `NAME`      → match named packet, both directions
/// - `in:NAME`   → match named packet, incoming only
/// - `out:NAME`  → match named packet, outgoing only
/// - `-NAME`     → exclude named packet, both directions
/// - `-in:NAME`  → exclude named packet, incoming only
/// - `-out:NAME` → exclude named packet, outgoing only
///
/// If `*` is seen for a direction: include all, then apply that direction's exclusions.
/// If `*` is absent for a direction: include nothing, then add explicit inclusions.
fn parse_filters(expr: &str) -> Filters {
    let mut in_all = false;
    let mut out_all = false;
    let mut in_excl: HashSet<&'static str> = HashSet::new();
    let mut out_excl: HashSet<&'static str> = HashSet::new();
    let mut in_incl: HashSet<&'static str> = HashSet::new();
    let mut out_incl: HashSet<&'static str> = HashSet::new();

    for raw in expr.split(',').map(str::trim) {
        if raw.is_empty() {
            continue;
        }

        let (exclude, token) = match raw.strip_prefix('-') {
            Some(rest) => (true, rest),
            None => (false, raw),
        };

        enum Dir {
            Both,
            In,
            Out,
        }
        let (dir, name) = if let Some(n) = token.strip_prefix("in:") {
            (Dir::In, n)
        } else if let Some(n) = token.strip_prefix("out:") {
            (Dir::Out, n)
        } else {
            (Dir::Both, token)
        };

        if name == "*" && !exclude {
            match dir {
                Dir::Both => {
                    in_all = true;
                    out_all = true;
                }
                Dir::In => {
                    in_all = true;
                }
                Dir::Out => {
                    out_all = true;
                }
            }
            continue;
        }

        let s = intern(name);
        if exclude {
            match dir {
                Dir::Both => {
                    in_excl.insert(s);
                    out_excl.insert(s);
                }
                Dir::In => {
                    in_excl.insert(s);
                }
                Dir::Out => {
                    out_excl.insert(s);
                }
            }
        } else {
            match dir {
                Dir::Both => {
                    in_incl.insert(s);
                    out_incl.insert(s);
                }
                Dir::In => {
                    in_incl.insert(s);
                }
                Dir::Out => {
                    out_incl.insert(s);
                }
            }
        }
    }

    Filters {
        incoming: if in_all {
            PacketFilter(FilterMode::AllExcept(in_excl))
        } else {
            PacketFilter(FilterMode::OnlyThese(in_incl))
        },
        outgoing: if out_all {
            PacketFilter(FilterMode::AllExcept(out_excl))
        } else {
            PacketFilter(FilterMode::OnlyThese(out_incl))
        },
    }
}

fn intern(s: &str) -> &'static str {
    Box::leak(s.to_string().into_boxed_str())
}

fn filters() -> &'static Filters {
    static FILTERS: OnceLock<Filters> = OnceLock::new();
    FILTERS.get_or_init(|| {
        let expr = std::env::var(ENV_VAR).unwrap_or_else(|_| DEFAULT_FILTER.to_string());
        parse_filters(&expr)
    })
}

// ── Public logging fns ────────────────────────────────────────────────────────

pub fn trace_in(cn: u32, name: &'static str, pkt: &dyn fmt::Debug) {
    if filters().incoming.allows(name) {
        tracing::trace!(target: "sauer_packet", cn, direction = "in", packet = name, data = ?TruncatedDebug(pkt));
    }
}

pub fn trace_out<T: PacketDescriptor + fmt::Debug>(cn: u32, pkt: &T) {
    log_out(pkt, |names, data| {
        tracing::trace!(target: "sauer_packet", cn, direction = "out", packets = ?names, data = ?TruncatedDebug(data));
    });
}

pub fn trace_out_broadcast<T: PacketDescriptor + fmt::Debug>(pkt: &T) {
    log_out(pkt, |names, data| {
        tracing::trace!(target: "sauer_packet", broadcast = true, direction = "out", packets = ?names, data = ?TruncatedDebug(data));
    });
}

pub fn trace_out_broadcast_except<T: PacketDescriptor + fmt::Debug>(except: u32, pkt: &T) {
    log_out(pkt, |names, data| {
        tracing::trace!(target: "sauer_packet", broadcast = true, except_cn = except, direction = "out", packets = ?names, data = ?TruncatedDebug(data));
    });
}

fn log_out<T: PacketDescriptor + fmt::Debug>(pkt: &T, emit: impl FnOnce(&[&'static str], &T)) {
    let all_names = pkt.packet_names();
    let matched: Vec<&'static str> = all_names
        .into_iter()
        .filter(|n| filters().outgoing.allows(n))
        .collect();
    if !matched.is_empty() {
        emit(&matched, pkt);
    }
}
