//! One id per thing (DESIGN-one-id-path-version-component-2026-10-30, "Naming discipline"): a fold-data node's haecceity,
//! the IRI that says which thing it is, and its quiddity, the kind of thing it is.
//!
//! A haecceity names; it never describes. It is `<base><source>/<segment>/<segment>…`, one segment per node below the
//! document's root, and no segment holds a kind, a title or a slug:
//! - a node with its own `id` is that id, UTF-8 percent-encoded and never slugged: the source's id wins;
//! - one without is an opaque token `~h`, h the first 16 hex of sha256(parent path, kind, raw name, first distinguishing
//!   property) joined by `\n`. It is PROVISIONAL: a label fix changes it, so it is never published or signed, and the
//!   fix is a source id in the data. Never its place alone.
//!
//! `role[name]` (Allie's address, the rotor's) is a selector that resolves to one of these, not part of one.
//! Two siblings that come out the same are one canonical path twice, and `unique` refuses them.

use serde_json::Value;
use sha2::{Digest, Sha256};

use super::{get, s, slug, LABEL_KEYS};

/// The one base every haecceity sits under (Bobby, 2026-10-30): reference only; sameness is idem.bounded.tools's.
pub const BASE: &str = "https://ipso.bounded.tools/";

/// A source id as one segment: every byte but `[A-Za-z0-9._-]` as `%XX` (uppercase), so `/`, `~` and `#` in an id stay
/// inside its segment and a leading `~` only ever marks a token. Never slugged, never case-folded.
pub fn segment_of(id: &str) -> String {
    id.bytes()
        .map(|b| match b {
            b'A'..=b'Z' | b'a'..=b'z' | b'0'..=b'9' | b'.' | b'_' | b'-' => (b as char).to_string(),
            _ => format!("%{b:02X}"),
        })
        .collect()
}

/// The root's quiddity: a fold document.
pub const ROOT_KIND: &str = "page";

/// What a node listed under each kids key is when it doesn't say: `sections` a section, `groups` a group, the rest items.
pub fn kind_under(key: &str) -> &'static str {
    match key {
        "sections" => "section",
        "groups" => "group",
        _ => "item",
    }
}

/// A node's quiddity: its own `kind`, slugged, else what its key says it is.
pub fn quiddity(o: &Value, key: &str) -> String {
    s(get(o, "kind")).map(|k| slug(&k)).unwrap_or_else(|| kind_under(key).to_string())
}

/// The name a token hashes, whole (a label, so a token is provisional): the first of the label keys, else `text`; blank when none.
pub fn raw_name(o: &Value) -> String {
    LABEL_KEYS.iter().chain(&["text"]).find_map(|k| s(get(o, k))).unwrap_or_default()
}

/// What tells two same-named siblings apart, in this order: where it links (`link.href`, else `href`), then its words.
pub const DISTINGUISHING: [&str; 6] = ["href", "text", "summary", "purpose", "why", "action"];

pub fn distinguishing(o: &Value) -> String {
    let link = get(o, "link").and_then(|l| s(get(l, "href")));
    link.or_else(|| DISTINGUISHING.iter().find_map(|k| s(get(o, k)))).unwrap_or_default()
}

/// The `h` of a token: the first 16 hex of sha256(parent, kind, name, dist joined by `\n`).
pub fn token(parent: &str, kind: &str, name: &str, dist: &str) -> String {
    let h = Sha256::digest([parent, kind, name, dist].join("\n").as_bytes());
    h[..8].iter().map(|b| format!("{b:02x}")).collect()
}

/// Whether a node names itself: a non-blank `id` (trimmed), or a numeric one (a PR number) as JSON writes it.
pub fn source_id(o: &Value) -> Option<String> {
    match get(o, "id") {
        Some(Value::Number(n)) => Some(n.to_string()),
        v => s(v),
    }
}

/// One node's segment under `parent` (the path so far, "" at the root): its source id, else a provisional token.
pub fn step(parent: &str, o: &Value, kind: &str) -> String {
    match source_id(o) {
        Some(id) => segment_of(&id),
        None => format!("~{}", token(parent, kind, &raw_name(o), &distinguishing(o))),
    }
}

pub fn child(parent: &str, step: &str) -> String {
    if parent.is_empty() {
        step.to_string()
    } else {
        format!("{parent}/{step}")
    }
}

/// The haecceity: the document's own IRI for its root, the path under it for the rest. Segments are already encoded.
pub fn haecceity(source: &str, path: &str) -> String {
    let doc = format!("{BASE}{}", segment_of(source));
    if path.is_empty() {
        doc
    } else {
        format!("{doc}/{path}")
    }
}

/// The element's HTML `id`: the path as it stands in the IRI, so `#<id>` needs no further escaping; the root's is the source.
pub fn html_id(source: &str, path: &str) -> String {
    if path.is_empty() {
        segment_of(source)
    } else {
        path.to_string()
    }
}

/// One canonical path per thing: the first path seen twice.
pub fn unique<'a>(paths: impl IntoIterator<Item = &'a str>) -> Result<(), String> {
    let mut seen = std::collections::HashSet::new();
    for p in paths {
        if !seen.insert(p) {
            return Err(p.to_string());
        }
    }
    Ok(())
}

/// Every node of a fold-data document, depth first: (path, quiddity, haecceity).
pub fn paths(root: &Value, source: &str) -> Vec<(String, String, String)> {
    fn go(o: &Value, path: &str, source: &str, out: &mut Vec<(String, String, String)>) {
        for key in super::KID_KEYS {
            let Some(Value::Array(cs)) = get(o, key) else { continue };
            for c in cs.iter().filter(|c| c.is_object()) {
                // A group with no title is layout only (as fold() reads it): its items step under the parent.
                if key == "groups" && s(get(c, "title")).is_none() {
                    go(c, path, source, out);
                    continue;
                }
                let kind = quiddity(c, key);
                let p = child(path, &step(path, c, &kind));
                out.push((p.clone(), kind, haecceity(source, &p)));
                go(c, &p, source, out);
            }
        }
    }
    let mut out = vec![(String::new(), ROOT_KIND.to_string(), haecceity(source, ""))];
    go(root, "", source, &mut out);
    out
}
