//! `basis`, the engine as a command for a seat (RECOMMEND-consultant-2026-11-01-basis-cli-plugin, slice 1; Bobby 089bd610):
//! a fold of a document read as text, the words a screen reader says for it (ax.rs [`spoken`]), one line per row, never a
//! digest. It reads the document the viewer reads and writes nothing.
//!
//! A path is its steps joined by `/`, each step a fold's id (as written, or as its page's anchor writes it, id.rs
//! `segment_of`) or, failing that, its title (case folded), so `needs`, `Needs you` and `needs/<id>` all reach a fold. The
//! text of a fold is:
//!
//! - `@ <path>`: the fold's own path as its page's anchor (`#<path>`), or `@ /` for the root;
//! - `in: A › B`: the way back, when the fold is under the root;
//! - `# <heading>`;
//! - one line per row, its spoken words joined by ` · `, and `  → <path>` when the row goes in to a fold (that fold's path,
//!   what `basis read` takes and the page's row anchor), or `  ↗ <href>` when its link is one the data carries;
//! - `act: <name> [<act id>] · …` for a row of acts (page.rs `acts`, the id `fold serve` takes an act's post at).

use std::collections::HashMap;

use serde_json::Value;

use crate::ax::{ax_of, spoken};
use crate::id::segment_of;
use crate::js::Clock;
use crate::{fold, FoldOpts};

/// The fold path as `basis` writes it: each id as its page's anchor writes it, joined by `/`.
pub fn path_text(path: &[String]) -> String {
    path.iter().map(|p| segment_of(p)).collect::<Vec<_>>().join("/")
}

fn key(path: &[String]) -> String {
    if path.is_empty() {
        "root".into()
    } else {
        path.join("~")
    }
}

/// A document folded once, for reading many paths.
pub struct Doc {
    nodes: HashMap<String, Value>,
    zone: Option<String>,
}

impl Doc {
    /// Folds `root` (a page, deck or Markdown document as fold() takes it); `zone` is the reader's IANA zone.
    pub fn new(root: &Value, zone: Option<String>) -> Result<Doc, String> {
        let opts = FoldOpts { local_time_zone: zone.clone(), ..FoldOpts::default() };
        let nodes = fold(root, &opts)
            .into_iter()
            .map(|(id, n)| serde_json::to_value(n).map(|v| (id, v)).map_err(|e| e.to_string()))
            .collect::<Result<_, _>>()?;
        Ok(Doc { nodes, zone })
    }

    /// A document already folded: fold()'s `[[id, node], …]`.
    pub fn from_nodes(nodes: &[(String, Value)], zone: Option<String>) -> Doc {
        Doc { nodes: nodes.iter().cloned().collect(), zone }
    }

    /// Every fold's path, sorted by its id so the order is stable.
    pub fn paths(&self) -> Vec<Vec<String>> {
        let mut ids: Vec<&String> = self.nodes.keys().collect();
        ids.sort();
        ids.into_iter().map(|id| if id == "root" { vec![] } else { id.split('~').map(str::to_string).collect() }).collect()
    }

    /// The fold at `at` (`/`-joined steps, see the module doc), as its path of ids; an error names the step that matched no fold.
    pub fn resolve(&self, at: &str) -> Result<Vec<String>, String> {
        let mut path: Vec<String> = vec![];
        for step in at.split('/').map(str::trim).filter(|s| !s.is_empty()) {
            let node = self.nodes.get(&key(&path)).ok_or_else(|| format!("{}: no such fold", path_text(&path)))?;
            let kids: Vec<(&str, &str)> = node["kids"]
                .as_array()
                .into_iter()
                .flatten()
                .map(|k| (k["id"].as_str().unwrap_or(""), k["title"].as_str().unwrap_or("")))
                .collect();
            let lower = step.to_lowercase();
            let hit = kids
                .iter()
                .find(|(id, _)| *id == step || segment_of(id) == step)
                .or_else(|| kids.iter().find(|(_, t)| t.to_lowercase() == lower));
            match hit {
                Some((id, _)) => path.push(id.to_string()),
                None => {
                    let under = if path.is_empty() { "the root".to_string() } else { path_text(&path) };
                    let names: Vec<String> = kids.iter().map(|(id, t)| format!("{} ({t})", segment_of(id))).collect();
                    return Err(format!("{step}: no fold by that id or title in {under}; it has: {}", names.join(", ")));
                }
            }
        }
        Ok(path)
    }

    /// The accessibility tree of the fold at `path`, its links being `basis` paths.
    pub fn ax(&self, path: &[String]) -> Value {
        self.ax_with(path, "")
    }

    fn ax_with(&self, path: &[String], mark: &str) -> Value {
        let link = |p: &[String], _: &str| format!("{mark}{}", path_text(p));
        ax_of(self.nodes.get(&key(path)), path, &link, "folds", &Clock::new(None, self.zone.as_deref()))
    }

    /// The fold at `path` as text (the module doc's shape), each line ending in a newline.
    pub fn text(&self, path: &[String]) -> String {
        text_of(&self.ax_with(path, FOLD), path)
    }
}

/// A fold link's href in the tree [`Doc::text`] reads, so it tells a fold's path from a link the data carries.
const FOLD: &str = "basis:";

/// Every link and form under `ax`, in order: `(true, path)` for a fold link, `(false, href)` for any other, and an act's
/// `(name, act id)` (its form's action under `act/`).
fn links_in(ax: &Value, links: &mut Vec<(bool, String)>, acts: &mut Vec<(String, String)>) {
    let s = |k: &str| ax.get(k).and_then(Value::as_str).unwrap_or("").to_string();
    match ax.get("role").and_then(Value::as_str) {
        Some("link") => {
            let h = s("href");
            links.push(h.strip_prefix(FOLD).map_or((false, h.clone()), |p| (true, p.to_string())));
        }
        Some("form") => {
            let a = s("action");
            acts.push((s("name"), a.strip_prefix("act/").unwrap_or(&a).to_string()));
            return;
        }
        _ => {}
    }
    for c in ax.get("children").and_then(Value::as_array).into_iter().flatten() {
        links_in(c, links, acts);
    }
}

/// A fold's accessibility tree (ax.rs `ax_of`, fold links as [`FOLD`] paths) as text.
fn text_of(ax: &Value, path: &[String]) -> String {
    let mut out = format!("@ {}\n", if path.is_empty() { "/".to_string() } else { path_text(path) });
    let kids = |v: &Value| v.get("children").and_then(Value::as_array).cloned().unwrap_or_default();
    for c in kids(ax) {
        match c.get("role").and_then(Value::as_str) {
            Some("navigation") => out += &format!("in: {}\n", spoken(&c).into_iter().skip(2).collect::<Vec<_>>().join(" › ")),
            Some("heading") => out += &format!("# {}\n", spoken(&c).join(" · ")),
            Some("list") => {
                for row in kids(&c) {
                    let (mut links, mut acts) = (vec![], vec![]);
                    links_in(&row, &mut links, &mut acts);
                    if !acts.is_empty() {
                        let each: Vec<String> = acts.iter().map(|(n, id)| format!("{n} [{id}]")).collect();
                        out += &format!("act: {}\n", each.join(" · "));
                        continue;
                    }
                    let mut words = spoken(&row);
                    words.dedup();
                    if words.is_empty() {
                        continue;
                    }
                    out += &words.join(" · ");
                    // A row goes in to the fold its link names (the way back to the root names the empty path, so none).
                    match links.into_iter().find(|(_, h)| !h.is_empty()) {
                        Some((true, p)) => out += &format!("  → {p}\n"),
                        Some((false, h)) => out += &format!("  ↗ {h}\n"),
                        None => out += "\n",
                    }
                }
            }
            _ => {
                for w in spoken(&c) {
                    out += &format!("{w}\n");
                }
            }
        }
    }
    out
}
