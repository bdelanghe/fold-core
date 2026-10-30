//! One id per thing (DESIGN-one-id-path-version-component-2026-10-30): a fold-data node's haecceity, the path IRI that says
//! which thing it is, and its quiddity, the kind of thing it is.
//!
//! A path is `<base><source>/<step>/<step>…`, one step per node below the document's root, each `kind[name]`:
//! - a node with its own `id` steps as `kind[slug(id)]`: the source's id wins;
//! - one without steps as `kind[slug(name)]~h`, where h is the first 8 hex of
//!   sha256(parent path, kind, raw name, first distinguishing property), joined by `\n`: never its title or its place alone.
//!
//! Two siblings that come out the same are one canonical path twice, and `unique` refuses them.

use serde_json::Value;
use sha2::{Digest, Sha256};

use super::{get, s, slug, LABEL_KEYS};

/// The one base every haecceity sits under (Bobby, 2026-10-30): reference only; sameness is idem.bounded.tools's.
pub const BASE: &str = "https://ipso.bounded.tools/";

/// The one slugging: lowercase, `[a-z0-9_.-]` kept, every other run one `-`, `-` trimmed from the ends, at most 60, `x` when empty.
pub fn slug_of(x: &str) -> String {
    slug(x)
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

/// The name a step slugs and hashes, whole: the first of the label keys, else `text`; blank when none.
pub fn raw_name(o: &Value) -> String {
    LABEL_KEYS.iter().chain(&["text"]).find_map(|k| s(get(o, k))).unwrap_or_default()
}

/// What tells two same-named siblings apart, in this order: where it links (`link.href`, else `href`), then its words.
pub const DISTINGUISHING: [&str; 6] = ["href", "text", "summary", "purpose", "why", "action"];

pub fn distinguishing(o: &Value) -> String {
    let link = get(o, "link").and_then(|l| s(get(l, "href")));
    link.or_else(|| DISTINGUISHING.iter().find_map(|k| s(get(o, k)))).unwrap_or_default()
}

/// The `~h` a step without a source id carries.
pub fn suffix(parent: &str, kind: &str, name: &str, dist: &str) -> String {
    let h = Sha256::digest([parent, kind, name, dist].join("\n").as_bytes());
    h[..4].iter().map(|b| format!("{b:02x}")).collect()
}

/// One node's step under `parent` (the path so far, "" at the root).
pub fn step(parent: &str, o: &Value, kind: &str) -> String {
    if let Some(id) = s(get(o, "id")) {
        return format!("{kind}[{}]", slug(&id));
    }
    let name = raw_name(o);
    format!("{kind}[{}]~{}", slug(&name), suffix(parent, kind, &name, &distinguishing(o)))
}

pub fn child(parent: &str, step: &str) -> String {
    if parent.is_empty() {
        step.to_string()
    } else {
        format!("{parent}/{step}")
    }
}

/// `[` and `]` may not stand in an IRI's path (RFC 3987): they go as %5B and %5D. Nothing else a step holds needs it.
pub fn encode(path: &str) -> String {
    path.replace('[', "%5B").replace(']', "%5D")
}

/// The haecceity: the document's own IRI for its root, the path under it for the rest.
pub fn haecceity(source: &str, path: &str) -> String {
    let doc = format!("{BASE}{}", slug(source));
    if path.is_empty() {
        doc
    } else {
        format!("{doc}/{}", encode(path))
    }
}

/// The element's HTML `id`: the path as it stands in the IRI, so `#<id>` needs no further escaping; the root's is the source.
pub fn html_id(source: &str, path: &str) -> String {
    if path.is_empty() {
        slug(source)
    } else {
        encode(path)
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
