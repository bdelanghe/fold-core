//! Real paths (BRIEF-fold-render-anywhere-2026-10-30, items 2 and 3; slice 2): every fold of a document as a whole page of its
//! own at `<base><segment>/…/`, written ahead (a host such as fold-serve writes each to its file), that works with JS off.
//!
//! - The page's `<title>` names the fold and its path.
//! - Every move between folds is a plain `<a href>` to a real path, in the same tab; only links off the fold open a new one.
//! - Focus moves only when you move: every such link ends in `#<id>`, and a browser focuses a fragment's target as it loads,
//!   with no script. Going in, the target is the fold's `<h1>` (focusable, `tabindex=-1`), so a screen reader starts at where
//!   you are; going back up the path, it is the row you came from. A page opened from nowhere moves no focus (lone's
//!   LONE_COGA_FOCUS_THEFT: `autofocus` would, without the reader asking).
//! - The `<h1>` and each child row have the HTML `id` of their path under the document (id.rs `html_id`, so `#<id>` needs no
//!   escaping), an element id derived from the path for convenience and never read as the data's identity. A row is the RDFa
//!   subject, a `schema:WebPageElement` whose `schema:about` is the IRI of the thing it shows (id.rs `haecceity`, brief item 20).
//! - A fold that presents as slides has a second page, `<base><segment>/…/@slides/`: `@` is always escaped in a segment
//!   (id.rs `segment_of`), so that step can never be a fold's.
//!
//! The tree is ax.rs's, given real-path links; the HTML is html.rs's, so a page's `<main>` is the fold's tree as toDom derives it.

use serde_json::{json, Value};

use crate::ax::{at, ax_of};
use crate::html::{attr, html, text};
use crate::id::{haecceity, html_id, segment_of};
use crate::js::Clock;

/// Where the pages are served from, when the host does not say.
pub const BASE: &str = "/fold/";

/// The last step of a fold's slides page.
pub const SLIDES: &str = "@slides";

/// The real path of a fold (`mode` "folds" or "slides"): each id one segment, as id.rs encodes it.
pub fn href(base: &str, path: &[String], mode: &str) -> String {
    let mut s = base.to_string();
    for p in path {
        s.push_str(&segment_of(p));
        s.push('/');
    }
    if mode == "slides" {
        s.push_str(SLIDES);
        s.push('/');
    }
    s
}

/// The page's file under the site's directory: the href's segments as a static server decodes them, so each is the id itself.
/// An id no file name can hold (empty, `.`, `..`, or holding `/` or NUL) is refused, never written.
pub fn file(path: &[String], mode: &str) -> Result<String, String> {
    let mut f = String::new();
    for p in path {
        if p.is_empty() || p == "." || p == ".." || p.contains(['/', '\0']) {
            return Err(format!("page: no file can hold the step {p:?}"));
        }
        f.push_str(p);
        f.push('/');
    }
    if mode == "slides" {
        f.push_str(SLIDES);
        f.push('/');
    }
    f.push_str("index.html");
    Ok(f)
}

/// The path under the document, as it stands in a haecceity.
fn under(path: &[String]) -> String {
    path.iter().map(|p| segment_of(p)).collect::<Vec<_>>().join("/")
}

/// The tree as a page shows it: links into the fold are `local` (same tab) and `ping` `<base>_sink/v` (fold sink, brief items 24
/// and 27: the browser itself says which row was followed), the heading has the fold's id, and each child row carries its id and
/// haecceity.
fn mark(ax: &mut Value, base: &str, own: &str, rows: &[(String, String)]) {
    let Some(o) = ax.as_object_mut() else { return };
    let role = o.get("role").and_then(Value::as_str).unwrap_or("").to_string();
    if role == "link" && o.get("href").and_then(Value::as_str).is_some_and(|h| h.starts_with(base)) {
        o.insert("local".into(), true.into());
        o.insert("ping".into(), format!("{base}_sink/v").into());
    }
    if role == "heading" {
        o.insert("id".into(), own.into());
    }
    if role == "listitem" {
        if let Some((id, iri)) = o.get("index").and_then(Value::as_u64).and_then(|i| rows.get(i as usize)) {
            o.insert("id".into(), id.clone().into());
            o.insert("haecceity".into(), iri.clone().into());
        }
    }
    if let Some(Value::Array(cs)) = o.get_mut("children") {
        for c in cs {
            mark(c, base, own, rows);
        }
    }
}

/// The names in a tree's path (its navigation's links), root first.
fn trail(ax: &Value) -> Vec<String> {
    fn links(v: &Value, out: &mut Vec<String>) {
        if v["role"] == "link" {
            out.extend(v["name"].as_str().map(str::to_string));
        }
        for c in v["children"].as_array().into_iter().flatten() {
            links(c, out);
        }
    }
    let mut out = vec![];
    for c in ax["children"].as_array().into_iter().flatten().filter(|c| c["role"] == "navigation") {
        links(c, &mut out);
    }
    out
}

/// The page's title: the fold's name, then the path it is in.
pub fn title(ax: &Value) -> String {
    let h = ax["children"].as_array().into_iter().flatten().find(|c| c["role"] == "heading");
    let name = h.and_then(|h| h["name"].as_str()).unwrap_or("").to_string();
    let path = trail(ax);
    if path.is_empty() {
        name
    } else {
        format!("{name} — {}", path.join(" / "))
    }
}

/// One fold's whole page, from its (marked) tree: or why toDom would refuse the tree. `base` is the viewer's, whose stylesheet
/// and Home Screen manifest (viewer/manifest.webmanifest) the page links.
pub fn page(tree: &Value, base: &str) -> Result<String, String> {
    let main = html(tree)?;
    Ok(format!(
        "<!doctype html>\n<html lang=\"en\">\n<head>\n<meta charset=\"utf-8\">\n\
         <meta name=\"viewport\" content=\"width=device-width, initial-scale=1\">\n<meta name=\"color-scheme\" content=\"light dark\">\n\
         <title>{}</title>\n<link rel=\"stylesheet\" href=\"{}fold.css\">\n<link rel=\"manifest\" href=\"{}manifest.webmanifest\">\n\
         </head>\n<body>\n{main}\n</body>\n</html>\n",
        text(&title(tree)),
        attr(base),
        attr(base)
    ))
}

/// Every page of a folded document (`nodes`, fold's `[[id, node], …]`), in its order: a fold's page, then its slides page if it
/// presents. Each is `{id, mode, href, file, html}`, or `{id, mode, href, error}` for one that cannot be written.
pub fn site(nodes: &[(String, Value)], source: &str, base: &str, clock: &Clock) -> Vec<Value> {
    let presents = |n: &Value| n.get("present").is_some_and(|p| !p.is_null());
    let folds: std::collections::HashMap<Vec<String>, &Value> = nodes.iter().map(|(id, n)| (at(id), n)).collect();
    let decks: std::collections::HashSet<Vec<String>> = nodes.iter().filter(|(_, n)| presents(n)).map(|(id, _)| at(id)).collect();
    // As slides, a link goes to a fold's slides page where it has one (it presents), else to the fold's page; at its heading.
    let page_of = |p: &[String], m: &str| href(base, p, if decks.contains(p) { m } else { "folds" });
    let link = |p: &[String], m: &str| format!("{}#{}", page_of(p, m), html_id(source, &under(p)));
    let mut out = vec![];
    for (id, node) in nodes {
        let path = at(id);
        let rows: Vec<(String, String)> = node["kids"]
            .as_array()
            .into_iter()
            .flatten()
            .map(|k| {
                let mut p = path.clone();
                p.push(k["id"].as_str().unwrap_or("").to_string());
                let u = under(&p);
                (html_id(source, &u), haecceity(source, &u))
            })
            .collect();
        for mode in ["folds", "slides"].into_iter().filter(|m| *m == "folds" || presents(node)) {
            let mut tree = ax_of(Some(node), &path, &link, mode, clock);
            // Back up the path, each step lands on the row it was left by, where that fold lists it; else on its heading.
            let nav = tree.pointer("/children/0/role").is_some_and(|r| r == "navigation");
            if let Some(Value::Array(steps)) = tree.pointer_mut("/children/0/children/0/children").filter(|_| nav) {
                for (i, li) in steps.iter_mut().enumerate() {
                    if let Some(a) = li.pointer_mut("/children/0").filter(|a| a["role"] == "link") {
                        let up = &path[..i];
                        let lists = folds.get(up).is_some_and(|n| n["kids"].as_array().into_iter().flatten().any(|k| k["id"] == path[i]));
                        let at = if lists { under(&path[..=i]) } else { html_id(source, &under(up)) };
                        a["href"] = format!("{}#{at}", page_of(up, mode)).into();
                    }
                }
            }
            mark(&mut tree, base, &html_id(source, &under(&path)), &rows);
            let made = file(&path, mode).and_then(|f| Ok((f, page(&tree, base)?)));
            out.push(match made {
                Ok((f, h)) => json!({ "id": id, "mode": mode, "href": href(base, &path, mode), "file": f, "html": h }),
                Err(e) => json!({ "id": id, "mode": mode, "href": href(base, &path, mode), "error": e }),
            });
        }
    }
    out
}
