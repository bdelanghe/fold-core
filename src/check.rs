//! The fold contract, checked (Bobby 2026-10-31: "figure out how it was possible to create a derivation that is incorrect").
//!
//! Two producers had each made something that looked like a fold without fold-core: a page that borrows the viewer's chrome
//! (`fold.css`, the "Basis" manifest, the "Everything ›" path) but writes its own `<main>` (trmnl-basis's html.rs), and documents
//! whose shape nothing checked. Nothing refused either, because fold-core's walk is lenient by design and nothing marked a page as
//! fold-core's. So:
//!
//! - `page.rs` stamps every page it renders with `<meta name="generator" content="fold-core">` ([`GENERATOR`]).
//! - [`page`] refuses an HTML page that claims the fold chrome without that stamp: only fold-core renders a fold page.
//! - [`doc`] checks a fold document (`page.json`): a title and children; every node labelled; every `items` entry has an id;
//!   ids unique among siblings.
//!
//! `fold check <file>…` runs these (`.html` → [`page`], `.json` → [`doc`]); producers run it in their tests and flake checks.

use std::collections::HashSet;

use serde_json::Value;

/// The stamp page.rs puts in every page's `<head>`.
pub const GENERATOR: &str = r#"<meta name="generator" content="fold-core">"#;

/// An HTML page: `Err` lists why it isn't a fold page when it presents as one.
pub fn page(html: &str) -> Result<(), Vec<String>> {
    let claims = html.contains("fold.css") || html.contains(r#"apple-mobile-web-app-title" content="Basis""#);
    if claims && !html.contains(GENERATOR) {
        return Err(vec![
            "presents as a fold (the viewer's fold.css / Basis manifest) but fold-core didn't render it: no generator stamp. \
             Render it with fold_core::page::page from a document, or drop the fold chrome."
                .into(),
        ]);
    }
    Ok(())
}

fn str_of<'a>(v: &'a Value, k: &str) -> Option<&'a str> {
    v.get(k).and_then(Value::as_str).filter(|s| !s.trim().is_empty())
}

/// The keys a node's children sit under, and the keys that can label one: lib.rs's KID_KEYS and LABEL_KEYS, as the walk reads them.
const KIDS: [&str; 5] = ["sections", "groups", "items", "kids", "children"];
const LABELS: [&str; 5] = ["title", "nav", "name", "action", "label"];

/// A fold document (`page.json`): `Err` lists every problem with its JSON path. The walk itself is lenient (any object folds),
/// so this is the contract a producer is held to: the root has a title and children; every node but an untitled group is
/// labelled; an `items` entry
/// has an id; ids are unique among siblings (a fold's path is its ids, so a repeat makes two folds one path).
pub fn doc(d: &Value) -> Result<(), Vec<String>> {
    let mut errs = vec![];
    if str_of(d, "title").is_none() {
        errs.push("/title: missing or empty".into());
    }
    if !KIDS.iter().any(|k| d.get(*k).and_then(Value::as_array).is_some_and(|a| !a.is_empty())) {
        errs.push("/: no children (a fold document has sections)".into());
    }
    walk(d, "", &mut errs);
    if errs.is_empty() { Ok(()) } else { Err(errs) }
}

fn walk(node: &Value, path: &str, errs: &mut Vec<String>) {
    for k in KIDS {
        let Some(kids) = node.get(k).and_then(Value::as_array) else { continue };
        let mut ids = HashSet::new();
        for (i, kid) in kids.iter().enumerate() {
            let p = format!("{path}/{k}/{i}");
            if !kid.is_object() {
                errs.push(format!("{p}: not an object"));
                continue;
            }
            match str_of(kid, "id") {
                Some(id) if !ids.insert(id.to_string()) => errs.push(format!("{p}/id: {id:?} repeats a sibling's")),
                None if k == "items" => errs.push(format!("{p}/id: missing (an item is addressed by its id)")),
                _ => {}
            }
            // An untitled group is a fold idiom: the walk shows its items in place, with no fold of its own.
            if k != "groups" && !LABELS.iter().any(|l| str_of(kid, l).is_some()) && str_of(kid, "text").is_none() {
                errs.push(format!("{p}: no label (title, name, nav, action, label) and no text"));
            }
            walk(kid, &p, errs);
        }
    }
}
