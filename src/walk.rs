//! walk.ts: the artificial viewer's walk over a fold viewer's folds. It reads every fold the way a screen reader gets it, through
//! its accessibility tree (`ax_of`), never the DOM, and asks of each fold what a cold reader asks: what is this, what does it
//! want, what next. What it finds is a finding with a ranked reduction (remove a choice > rename > restructure), never a fix.
//! Byte-identical to walk.ts at the rev in `goldens/REV`; the budgets come in as budgets.json holds them, so the core reads no file.

use std::collections::{HashMap, HashSet};
use std::sync::OnceLock;

use regex::Regex;
use serde::{Deserialize, Serialize};
use serde_json::{json, Number, Value};
use sha2::{Digest, Sha256};

use crate::ax::{ax_of, concat, spoken};
use crate::js::{self, js_trim, Clock};
use crate::MAX_KIDS;

/// What a fold costs to open: one fold doc over this many bytes blocks, on a kept page's folds too.
pub const FOLD_BYTES: usize = 8 * 1024;

/// A finding's reduction, ranked: remove a choice before renaming, rename before restructuring.
#[derive(Serialize, Deserialize, Clone, Copy, Debug, PartialEq, Eq)]
#[serde(rename_all = "lowercase")]
pub enum Kind {
    Remove,
    Rename,
    Restructure,
}

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, Eq)]
pub struct Finding {
    pub fold: String,
    pub check: String,
    pub kind: Kind,
    pub blocks: bool,
    pub said: String,
    pub reduction: String,
}

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, Eq)]
pub struct Walk {
    pub folds: usize,
    pub reached: usize,
    pub findings: Vec<Finding>,
}

/// One entry of budgets.json's `budgets` (only the fields the walk reads).
#[derive(Serialize, Deserialize, Clone, Debug, PartialEq)]
pub struct Budget {
    pub id: String,
    pub value: Number,
}

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, Default)]
pub struct WalkOpts {
    /// budgets.json's `budgets`: the walk holds each fold to `view.words`.
    pub budgets: Vec<Budget>,
    /// The machine's zone, where a fold's "Data from …" time is said (IANA name; UTC when absent).
    #[serde(rename = "localTimeZone", skip_serializing_if = "Option::is_none", default)]
    pub local_time_zone: Option<String>,
}

fn get<'a>(o: &'a Value, k: &str) -> Option<&'a Value> {
    o.as_object().and_then(|m| m.get(k))
}

fn truthy(v: Option<&Value>) -> bool {
    match v {
        None | Some(Value::Null) => false,
        Some(Value::Bool(b)) => *b,
        Some(Value::Number(n)) => n.as_f64().is_some_and(|f| f != 0.0 && !f.is_nan()),
        Some(Value::String(x)) => !x.is_empty(),
        Some(_) => true,
    }
}

fn re(cell: &'static OnceLock<Regex>, src: &str) -> &'static Regex {
    cell.get_or_init(|| Regex::new(src).expect("valid regex"))
}

/// Words Bobby ruled off the page (10-27): `[/\bwork in progress\b/i, /\bbeing worked on\b/i]`, the first one that matches.
fn banned(line: &str) -> Option<&str> {
    static A: OnceLock<Regex> = OnceLock::new();
    static B: OnceLock<Regex> = OnceLock::new();
    [re(&A, r"(?i-u)\bwork in progress\b"), re(&B, r"(?i-u)\bbeing worked on\b")].into_iter().find_map(|r| r.find(line)).map(|m| m.as_str())
}

/// A line that names a state and nothing more ("Stuck.", "Down"): `/^\W*(stuck|…|waiting)\W*$/i`.
fn bare_state(t: &str) -> bool {
    static R: OnceLock<Regex> = OnceLock::new();
    re(&R, r"^[^A-Za-z0-9_]*(?i-u:stuck|blocked|down|failing|failed|broken|stale|error|offline|degraded|unknown|waiting)[^A-Za-z0-9_]*$")
        .is_match(t)
}

/// `/(^|~)part-\d+~part-\d+$/`: a named run holding named runs.
fn nested_runs(id: &str) -> bool {
    static R: OnceLock<Regex> = OnceLock::new();
    re(&R, r"(^|~)part-[0-9]+~part-[0-9]+$").is_match(id)
}

/// `/(^|~)artifacts~./`: a kept page read inline, whose findings are reported and never block.
fn inline_read(id: &str) -> bool {
    id.match_indices("artifacts~").any(|(i, m)| {
        (i == 0 || id[..i].ends_with('~'))
            && id[i + m.len()..].chars().next().is_some_and(|c| !matches!(c, '\n' | '\r' | '\u{2028}' | '\u{2029}'))
    })
}

/// Words, as `.split(/\s+/).filter(Boolean).length` counts them.
fn count(xs: &[String]) -> usize {
    xs.iter().map(|x| js::split_ws(x).filter(|w| !w.is_empty()).count()).sum()
}

/// `s.split(/\s+/).length`: one more than the whitespace runs.
fn split_len(s: &str) -> usize {
    let mut runs = 0;
    let mut gap = false;
    for c in s.chars() {
        let w = js::is_ws_char(c);
        if w && !gap {
            runs += 1;
        }
        gap = w;
    }
    runs + 1
}

/// `(v ?? []).length` for a fold's `text`: an array's items or a string's code units.
fn len(v: Option<&Value>) -> usize {
    match v {
        Some(Value::Array(a)) => a.len(),
        Some(Value::String(s)) => js::len16(s),
        _ => 0,
    }
}

fn path_href(p: &[String], m: &str) -> String {
    format!("{}{}", if m == "slides" { "#slides/" } else { "#/" }, p.join("/"))
}

fn child_id(id: &str, k: &str) -> String {
    if id == "root" {
        k.into()
    } else {
        format!("{id}~{k}")
    }
}

struct Walker<'a> {
    nodes: HashMap<&'a str, &'a Value>,
    order: Vec<&'a str>,
    words: f64,
    clock: Clock,
    seen: HashSet<String>,
    findings: Vec<Finding>,
}

fn finding(fold: &str, check: &str, kind: Kind, blocks: bool, said: String, reduction: String) -> Finding {
    Finding { fold: fold.into(), check: check.into(), kind, blocks, said, reduction }
}

impl Walker<'_> {
    fn add(&mut self, mut f: Finding) {
        if inline_read(&f.fold) {
            f.blocks = false;
        }
        self.findings.push(f);
    }

    /// The budget as JS prints a number (220, not 220.0).
    fn budget(&self) -> String {
        if self.words.fract() == 0.0 && self.words.abs() < 1e21 {
            format!("{}", self.words as i64)
        } else {
            self.words.to_string()
        }
    }

    fn visit(&mut self, id: &str, path: &[String]) {
        if !self.seen.insert(id.to_string()) {
            return;
        }
        let n = self.nodes[id];
        let words = count(&spoken(&ax_of(Some(n), path, &|p, _| path_href(p, "folds"), "folds", &self.clock)));
        let kids = concat(get(n, "kids"));
        let bytes = serde_json::to_string(n).map_or(0, |s| s.len());
        if bytes > FOLD_BYTES {
            self.findings.push(finding(
                id,
                "fold-bytes",
                Kind::Restructure,
                true,
                format!("{bytes} bytes to open"),
                format!("fold it further, below {FOLD_BYTES} bytes per fold"),
            ));
        }
        let text = concat(get(n, "text"));
        for t in &text {
            let Value::String(t) = t else { continue };
            if bare_state(js_trim(t)) {
                let r = format!("say what \"{}\" is about and what to do, or drop the line", js_trim(t));
                self.add(finding(id, "unexplained-state", Kind::Rename, true, t.clone(), r));
            }
        }
        let title = js::string(get(n, "title"));
        if kids.is_empty() && len(get(n, "text")) == 0 && !truthy(get(n, "href")) && split_len(&title) <= 3 {
            self.add(finding(id, "no-intent", Kind::Remove, false, title.clone(), "an empty fold: remove it from its parent".into()));
        }
        if kids.len() > MAX_KIDS {
            self.add(finding(
                id,
                "choices",
                Kind::Restructure,
                true,
                format!("{} links", kids.len()),
                format!("group into at most {MAX_KIDS}"),
            ));
        }
        if words as f64 > self.words {
            let b = self.budget();
            self.add(finding(id, "words", Kind::Restructure, true, format!("{words} words"), format!("split below {b} words")));
        }
        if nested_runs(id) {
            let r = "group the list by what its items share, so one level of runs holds it".into();
            self.add(finding(id, "nested-runs", Kind::Restructure, true, title.clone(), r));
        }
        if truthy(get(n, "present")) {
            let slide = count(&spoken(&ax_of(Some(n), path, &path_href, "slides", &self.clock)));
            if slide as f64 > self.words {
                let b = self.budget();
                let r = format!("split the slide below {b} words");
                self.add(finding(id, "slide-words", Kind::Restructure, true, format!("{slide} words on the slide"), r));
            }
            for k in ["prev", "next"] {
                let Some(step) = get(n, k).filter(|v| truthy(Some(v))) else { continue };
                if !self.nodes.contains_key(js::string(get(step, "id")).as_str()) {
                    let said = format!("{k}: {}", js::string(get(step, "title")));
                    self.add(finding(id, "dangling", Kind::Remove, true, said, format!("the slide's {k} leads nowhere: refold the page")));
                }
            }
        }
        for k in &kids {
            let kt = js::string(get(k, "title"));
            let hint = get(k, "hint").filter(|h| truthy(Some(h)));
            let line = match hint {
                Some(h) => format!("{kt}: {}", js::string(Some(h))),
                None => kt.clone(),
            };
            if let Some(Value::String(h)) = hint {
                let h = js_trim(h);
                if h.ends_with('…') || h.ends_with("...") {
                    self.add(finding(
                        id,
                        "cut-hint",
                        Kind::Rename,
                        false,
                        line.clone(),
                        format!("a hint that is a whole sentence for \"{kt}\""),
                    ));
                }
            }
            if !inline_read(id) {
                if let Some(w) = banned(&line) {
                    let r = format!("drop \"{w}\" (Bobby 10-27)");
                    self.add(finding(id, "ruled-words", Kind::Rename, false, line.clone(), r));
                }
            }
            // Every row goes into its own fold; a row that is only a link, with no fold, is not dangling.
            let kid = js::string(get(k, "id"));
            let cid = child_id(id, &kid);
            if !self.nodes.contains_key(cid.as_str()) {
                if !truthy(get(k, "href")) {
                    self.add(finding(
                        id,
                        "dangling",
                        Kind::Remove,
                        true,
                        kt.clone(),
                        format!("\"{kt}\" leads nowhere: remove it or write its fold"),
                    ));
                }
            } else {
                let mut p = path.to_vec();
                p.push(kid);
                self.visit(&cid, &p);
            }
        }
        for t in &text {
            if !inline_read(id) && banned(&js::string(Some(t))).is_some() {
                let r = "say which goal session, not that work is in progress (Bobby 10-27)".into();
                self.add(finding(id, "ruled-words", Kind::Rename, false, js::string(Some(t)), r));
            }
        }
    }
}

/// walk.ts `walk`: every fold from "root", judged; `nodes` is the fold collection in its order (a repeated id keeps its first
/// place and takes the last node, as a `Map` does). Findings come blocking first, then by kind, then by fold id.
pub fn walk(nodes: &[(String, Value)], opts: &WalkOpts) -> Result<Walk, String> {
    let words = opts.budgets.iter().find(|b| b.id == "view.words").ok_or("budgets: no view.words")?.value.as_f64().ok_or("view.words")?;
    let mut w = Walker {
        nodes: HashMap::new(),
        order: vec![],
        words,
        clock: Clock::new(None, opts.local_time_zone.as_deref()),
        seen: HashSet::new(),
        findings: vec![],
    };
    for (id, n) in nodes {
        if w.nodes.insert(id, n).is_none() {
            w.order.push(id);
        }
    }
    if w.nodes.contains_key("root") {
        w.visit("root", &[]);
    } else {
        w.add(finding("root", "dangling", Kind::Restructure, true, "no root".into(), "the viewer has nothing to open on".into()));
    }
    for id in w.order.clone() {
        if !w.seen.contains(id) {
            let said = js::string(get(w.nodes[id], "title"));
            w.add(finding(id, "unreachable", Kind::Remove, false, said, "no path from the top reaches it: remove it or link it".into()));
        }
    }
    let rank = |k: Kind| k as u8;
    w.findings
        .sort_by(|a, b| b.blocks.cmp(&a.blocks).then(rank(a.kind).cmp(&rank(b.kind))).then_with(|| js::locale_compare(&a.fold, &b.fold)));
    Ok(Walk { folds: w.order.len(), reached: w.seen.len(), findings: w.findings })
}

/// walk.ts `verdict`: the walk in one line, for the page: nothing blocking, or the first blocking finding and its reduction.
pub fn verdict(r: &Walk) -> String {
    let Some(b) = r.findings.iter().find(|f| f.blocks) else {
        return "Checked by the artificial viewer: every fold reached, nothing blocking.".into();
    };
    let at = if b.fold == "root" { "this fold".into() } else { b.fold.replace('~', " › ") };
    format!("The artificial viewer is stuck on \"{}\" ({at}): {}.", js::slice16(&b.said, 60), b.reduction)
}

fn vid(k: &[&str]) -> String {
    let h = Sha256::digest(k.join("\u{0}").as_bytes());
    h.iter().map(|b| format!("{b:02x}")).collect::<String>()[..32].to_string()
}

/// walk.ts `factoBody`: one Observation for the walk, one per finding, each joined to the walk by a `found_in` edge.
pub fn facto_body(artifact: &str, at: &str, rev: &str, r: &Walk) -> Value {
    let walk_id = vid(&["artificial-viewer-walk", artifact, at]);
    let blocking = r.findings.iter().filter(|f| f.blocks).count();
    let mut vertices = vec![json!({ "id": walk_id, "labels": ["Observation", "ArtificialViewerWalk"], "props": {
        "name": format!("artificial viewer walk {artifact} {at}"), "artifact": artifact, "observedAt": at, "rev": rev,
        "folds": r.folds, "reached": r.reached, "findings": r.findings.len(), "blocking": blocking, "source": "fold-viewer/walk.ts",
        "evidence": "a static walk of the fold data's accessibility trees (fold-ax.js), proxies only; not a person and not a cold read",
    } })];
    let mut edges = vec![];
    for (i, f) in r.findings.iter().enumerate() {
        let id = vid(&["artificial-viewer-finding", &walk_id, &i.to_string()]);
        let mut props = serde_json::Map::new();
        props.insert("name".into(), format!("{}: {}", f.check, js::slice16(&f.said, 60)).into());
        props.insert("rank".into(), (i + 1).into());
        if let Value::Object(m) = json!(f) {
            props.extend(m);
        }
        props.insert("artifact".into(), artifact.into());
        props.insert("observedAt".into(), at.into());
        props.insert("source".into(), "fold-viewer/walk.ts".into());
        vertices.push(json!({ "id": id, "labels": ["Observation", "ArtificialViewerFinding"], "props": props }));
        edges.push(json!({ "id": vid(&["found_in", &id, &walk_id]), "src_id": id, "dst_id": walk_id, "label": "found_in" }));
    }
    json!({ "vertices": vertices, "edges": edges })
}
