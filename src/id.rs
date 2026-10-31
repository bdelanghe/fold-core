//! One id per thing (DESIGN-one-id-path-version-component-2026-10-30, "Naming discipline"): a node's haecceity, the IRI that
//! says which thing it is, and the HTML id that carries the same path. A haecceity names; it never describes. It is
//! `<base><source>/<segment>/<segment>…`, one segment per node below the document's root, and a segment holding a source id
//! is that id UTF-8 percent-encoded, never slugged: the source's id wins. How a node without one gets its segment, and the
//! fixity beside it, come with the rest of the one-id work (PR #12); this is what the pages at their real paths stand on.

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
