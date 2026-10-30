//! fold-core: the shared fold core (one core for every fold viewer).
//!
//! Folds one JSON object (a status page, a deck, a page's Markdown) into fold-viewer nodes, byte-identical to
//! the TypeScript fold viewer (`fold.ts`) at the rev in `goldens/REV`. Pure: no clock, no network, no filesystem; the caller
//! passes the time (`FoldOpts::updated`) and the source files (`with_sources`'s reader).
//!
//! JS string semantics are kept where they show: lengths and cuts count UTF-16 code units, `\s` and `trim()` are JS's whitespace
//! set, `.` stops at JS line terminators.

pub mod ax;
pub mod html;
pub mod id;
mod js;
pub mod page;
mod tz;
pub mod walk;
pub mod wasm;

use std::cell::RefCell;
use std::collections::{HashMap, HashSet};
use std::rc::Rc;

use serde::{Deserialize, Serialize};
use serde_json::{Map, Value};

use js::{clip, js_trim, len16, strip_quotes, when, Clock};

/// At most this many links per fold; past it, "More" or named runs.
pub const MAX_KIDS: usize = 5;
/// Words on one screenful (a page of a long section or slide).
pub const PAGE_WORDS: usize = 80;

const LABEL_KEYS: [&str; 5] = ["title", "nav", "name", "action", "label"];
const TEXT_KEYS: [&str; 13] =
    ["purpose", "summary", "text", "state", "last", "at", "waits", "action", "why", "next", "detail", "note", "owner"];
const KID_KEYS: [&str; 5] = ["sections", "groups", "items", "kids", "children"];

/// A link on a fold: one row.
#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, Eq, Default)]
#[cfg_attr(feature = "schema", derive(schemars::JsonSchema))]
pub struct Kid {
    pub id: String,
    pub title: String,
    #[serde(skip_serializing_if = "Option::is_none", default)]
    pub hint: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none", default)]
    pub href: Option<String>,
}

/// The fold before or after one in a presenting subtree's reading order.
#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, Eq, Default)]
#[cfg_attr(feature = "schema", derive(schemars::JsonSchema))]
pub struct Step {
    pub id: String,
    pub title: String,
}

/// One fold. Field order is fold.ts's key order; absent fields are left out, as `JSON.stringify` leaves out `undefined`.
#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, Eq, Default)]
#[cfg_attr(feature = "schema", derive(schemars::JsonSchema))]
#[serde(rename_all = "camelCase")]
pub struct Node {
    pub title: String,
    pub trail: Vec<String>,
    pub kids: Vec<Kid>,
    #[serde(skip_serializing_if = "Option::is_none", default)]
    pub text: Option<Vec<String>>,
    #[serde(skip_serializing_if = "Option::is_none", default)]
    pub href: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none", default)]
    pub href_label: Option<String>,
    /// fold.ts `act`: the fold's one action, `{cap, call, args, label}` (the viewer calls `claude.use(cap)[call](args)`).
    #[serde(skip_serializing_if = "Option::is_none", default)]
    pub act: Option<Value>,
    #[serde(skip_serializing_if = "Option::is_none", default)]
    pub updated: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none", default)]
    pub source: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none", default)]
    pub stale: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none", default)]
    pub present: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none", default)]
    pub prev: Option<Step>,
    #[serde(skip_serializing_if = "Option::is_none", default)]
    pub next: Option<Step>,
}

/// fold.ts's `opts`, plus the zone JS would take from the machine.
#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, Eq, Default)]
pub struct FoldOpts {
    #[serde(skip_serializing_if = "Option::is_none", default)]
    pub source: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none", default)]
    pub updated: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none", default)]
    pub stale: Option<String>,
    /// The artifact this page is: a row linking to `/a/<self>` goes back to the top.
    #[serde(rename = "self", skip_serializing_if = "Option::is_none", default)]
    pub self_id: Option<String>,
    /// The zone JS takes from the machine when the page has no `meta.timeZone`, and for times without an offset (IANA name; UTC
    /// when absent). The core reads no environment, so the caller says it.
    #[serde(rename = "localTimeZone", skip_serializing_if = "Option::is_none", default)]
    pub local_time_zone: Option<String>,
}

// ── JSON access with JS's semantics ─────────────────────────────────────

fn get<'a>(o: &'a Value, k: &str) -> Option<&'a Value> {
    o.as_object().and_then(|m| m.get(k))
}

/// fold.ts `str`: a non-blank string, trimmed.
fn s(v: Option<&Value>) -> Option<String> {
    match v {
        Some(Value::String(x)) if !js_trim(x).is_empty() => Some(js_trim(x).to_string()),
        _ => None,
    }
}

fn raw(v: Option<&Value>) -> &str {
    v.and_then(Value::as_str).unwrap_or("")
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

/// JS `a ?? b` over a property: `null` and missing both fall through.
fn nullish(v: Option<&Value>) -> bool {
    matches!(v, None | Some(Value::Null))
}

fn is_ref(s: &str) -> bool {
    let t = s.trim_end_matches(|c: char| c.is_ascii_digit());
    t.len() < s.len() && t.ends_with('#')
}

fn label(o: &Value) -> String {
    let own = LABEL_KEYS.iter().find_map(|k| s(get(o, k)));
    if let (Some(n), Some(t)) = (&own, s(get(o, "text"))) {
        if is_ref(n) {
            return clip(&strip_quotes(&t), 70);
        }
    }
    own.or_else(|| s(get(o, "text"))).or_else(|| s(get(o, "id"))).unwrap_or_else(|| "Untitled".into())
}

fn ref_hint(o: &Value) -> Option<String> {
    let n = s(get(o, "name"))?;
    (is_ref(&n) && s(get(o, "text")).is_some()).then_some(n)
}

fn slug(x: &str) -> String {
    let lower = x.to_lowercase();
    let mut out = String::new();
    let mut gap = false;
    for c in lower.chars() {
        if c.is_ascii_lowercase() || c.is_ascii_digit() || matches!(c, '_' | '.' | '-') {
            out.push(c);
            gap = false;
        } else if !gap {
            out.push('-');
            gap = true;
        }
    }
    let t: String = out.trim_matches('-').chars().take(60).collect();
    if t.is_empty() {
        "x".into()
    } else {
        t
    }
}

fn kids_of(o: &Value) -> Vec<&Value> {
    let mut out = Vec::new();
    for k in KID_KEYS {
        let Some(Value::Array(cs)) = get(o, k) else { continue };
        for c in cs {
            if !(c.is_object() || c.is_array()) {
                continue;
            }
            // A group with no title is layout only: its items belong to the parent.
            if k == "groups" && s(get(c, "title")).is_none() {
                out.extend(kids_of(c));
            } else {
                out.push(c);
            }
        }
    }
    out
}

fn short(x: &str) -> String {
    clip(x.strip_suffix('…').unwrap_or(x), 22)
}

/// The first whole sentence within 90 code units: `/^.{1,90}?[.!?](?=\s|$)/`.
fn first_sentence(h: &str) -> Option<String> {
    let u: Vec<u16> = h.encode_utf16().collect();
    for i in 1..=90.min(u.len()) {
        if js::is_line_terminator(u[i - 1]) {
            return None;
        }
        if i < u.len() && matches!(u[i], 0x2E | 0x21 | 0x3F) && (i + 1 == u.len() || js::is_ws(u[i + 1])) {
            return Some(String::from_utf16_lossy(&u[..=i]));
        }
    }
    None
}

// ── fold ────────────────────────────────────────────────────────────────

type Cell = Rc<RefCell<Node>>;

struct Folder<'a> {
    opts: &'a FoldOpts,
    clock: Clock,
    nodes: Vec<(String, Cell)>,
    at: HashMap<String, usize>,
    presenting: Vec<(String, String)>,
}

fn key(path: &[String]) -> String {
    if path.is_empty() {
        "root".into()
    } else {
        path.join("~")
    }
}

fn with(path: &[String], x: &str) -> Vec<String> {
    let mut p = path.to_vec();
    p.push(x.to_string());
    p
}

impl Folder<'_> {
    /// `nodes.set`: a key already there keeps its place and takes the new node.
    fn put(&mut self, path: &[String], n: Node) -> Cell {
        let k = key(path);
        let cell = Rc::new(RefCell::new(n));
        match self.at.get(&k) {
            Some(&i) => self.nodes[i].1 = cell.clone(),
            None => {
                self.at.insert(k.clone(), self.nodes.len());
                self.nodes.push((k, cell.clone()));
            }
        }
        cell
    }

    fn when(&self, iso: &str) -> String {
        when(iso, &self.clock)
    }

    fn field(&self, k: &str, v: &str) -> String {
        let lab = match k {
            "state" => "Status: ",
            "last" => "Last progress: ",
            "waits" => "Waits on: ",
            "action" => "What to do: ",
            "owner" => "Owner: ",
            _ => "",
        };
        let body = if k == "last" || k == "at" { self.when(v) } else { v.to_string() };
        format!("{lab}{body}")
    }

    fn walk(&mut self, o: &Value, path: &[String], trail: Vec<String>) {
        let title = label(o);
        if let Some(p @ ("slides" | "folds")) = get(o, "present").and_then(Value::as_str) {
            self.presenting.push((key(path), p.to_string()));
        }
        let alert = truthy(get(o, "alert"));
        let why = s(get(o, "why"));
        let mut text = Vec::new();
        if alert {
            text.push(format!("Alert: {}.", why.as_deref().unwrap_or("stuck or quiet too long")));
        }
        for k in TEXT_KEYS {
            let xs: Vec<&Value> = match get(o, k) {
                None | Some(Value::Null) => vec![],
                Some(Value::Array(a)) => a.iter().collect(),
                Some(v) => vec![v],
            };
            for x in xs {
                if k == "why" && alert && why.is_some() {
                    continue;
                }
                if let Some(v) = s(Some(x)) {
                    if strip_quotes(&v) != title {
                        text.push(self.field(k, &v));
                    }
                }
            }
        }
        let link = get(o, "link").filter(|l| l.is_object() || l.is_array());
        let kids = kids_of(o);
        if kids.is_empty() && s(get(o, "empty")).is_some() {
            text.push(raw(get(o, "empty")).to_string());
        }
        let lhref = link.and_then(|l| get(l, "href"));
        let hv = if nullish(lhref) { get(o, "href") } else { lhref };
        let mut n = Node { title: title.clone(), trail: trail.clone(), ..Default::default() };
        if !text.is_empty() {
            n.text = Some(text);
        }
        if s(hv).is_some() {
            n.href = Some(raw(hv).to_string());
            n.href_label = s(link.and_then(|l| get(l, "label")));
        }
        // fold.ts: cap, call and label non-blank strings, args a truthy object (arrays too, as `typeof` says); cap and call as written
        let act = get(o, "act");
        let args = act.and_then(|a| get(a, "args")).filter(|v| v.is_object() || v.is_array());
        if let (Some(_), Some(_), Some(label), Some(args)) =
            (s(act.and_then(|a| get(a, "cap"))), s(act.and_then(|a| get(a, "call"))), s(act.and_then(|a| get(a, "label"))), args)
        {
            let a = act.expect("act");
            let mut m = Map::new();
            m.insert("cap".into(), a["cap"].clone());
            m.insert("call".into(), a["call"].clone());
            m.insert("args".into(), args.clone());
            m.insert("label".into(), Value::String(label));
            n.act = Some(Value::Object(m));
        }
        if path.is_empty() {
            n.updated = self.opts.updated.clone();
            n.source = self.opts.source.clone();
            n.stale = self.opts.stale.clone().filter(|x| !x.is_empty());
        }
        let cell = self.put(path, n);
        self.chunk(&cell, &kids, path, with(&trail, &title));
    }

    fn chunk(&mut self, n: &Cell, kids: &[&Value], path: &[String], trail: Vec<String>) {
        if kids.len() > MAX_KIDS && (path.is_empty() || kids.len() < 2 * MAX_KIDS) {
            let rest = &kids[MAX_KIDS - 1..];
            self.chunk(n, &kids[..MAX_KIDS - 1], path, trail.clone());
            let more = if path.is_empty() { "More of this page".to_string() } else { format!("More in {}", n.borrow().title) };
            let mut seen = HashSet::new();
            let names: Vec<String> = rest.iter().map(|k| label(k)).filter(|l| seen.insert(l.clone())).collect();
            n.borrow_mut().kids.push(Kid { id: "more".into(), title: more.clone(), hint: Some(clip(&names.join(", "), 120)), href: None });
            let mp = with(path, "more");
            let m = self.put(&mp, Node { title: more.clone(), trail: trail.clone(), ..Default::default() });
            self.chunk(&m, rest, &mp, with(&trail, &more));
            return;
        }
        if kids.len() > MAX_KIDS {
            let mut span = MAX_KIDS;
            while span * MAX_KIDS < kids.len() {
                span *= MAX_KIDS;
            }
            let count = kids.len().div_ceil(span);
            let size = kids.len().div_ceil(count);
            for i in 0..count {
                let run = &kids[(i * size).min(kids.len())..((i + 1) * size).min(kids.len())];
                let id = format!("part-{}", i + 1);
                let title = format!("{} – {}", short(&label(run[0])), short(&label(run[run.len() - 1])));
                n.borrow_mut().kids.push(Kid { id: id.clone(), title: title.clone(), hint: None, href: None });
                let mp = with(path, &id);
                let m = self.put(&mp, Node { title: title.clone(), trail: trail.clone(), ..Default::default() });
                self.chunk(&m, run, &mp, with(&trail, &title));
            }
            return;
        }
        // The ids fold() makes itself (More, the named runs) and the root's are taken, so no kid overwrites one.
        let mut used: HashSet<String> = if path.is_empty() { HashSet::from(["root".to_string()]) } else { HashSet::new() };
        let taken = |used: &HashSet<String>, id: &str| used.contains(id) || id == "more" || js::is_run_id(id);
        for &k in kids {
            let base = slug(&s(get(k, "id")).unwrap_or_else(|| label(k)));
            let mut id = base.clone();
            let mut i = 2;
            while taken(&used, &id) {
                id = format!("{base}-{i}");
                i += 1;
            }
            used.insert(id.clone());

            let lab = label(k);
            let last = s(get(k, "last")).map(|_| format!("last progress {}", self.when(raw(get(k, "last")))));
            let progress = [s(get(k, "state")), last.clone()].into_iter().flatten().collect::<Vec<_>>().join(", ");
            let h = if truthy(get(k, "alert")) {
                let inner = s(get(k, "why")).unwrap_or_else(|| {
                    let waits = s(get(k, "waits")).map(|_| format!("waits on {}", raw(get(k, "waits"))));
                    let j = [waits, last.clone()].into_iter().flatten().collect::<Vec<_>>().join(", ");
                    if j.is_empty() {
                        "quiet too long".into()
                    } else {
                        j
                    }
                });
                Some(format!("Alert: {inner}"))
            } else {
                let text = s(get(k, "text")).filter(|t| *t != lab);
                [ref_hint(k), s(get(k, "summary")), s(get(k, "purpose")), Some(progress), s(get(k, "why")), text]
                    .into_iter()
                    .flatten()
                    .find(|x| !x.is_empty())
            };
            let hint = h.map(|h| {
                let h = if len16(&h) > 90 { first_sentence(&h).unwrap_or(h) } else { h };
                clip(&h, 90)
            });
            let link = get(k, "link").filter(|l| l.is_object() || l.is_array());
            let lhref = link.and_then(|l| get(l, "href"));
            let href = s(if nullish(lhref) { get(k, "href") } else { lhref });
            if let (Some(me), Some(href)) = (&self.opts.self_id, &href) {
                if js::self_link(me, href) {
                    let title = format!("{}: back to the top", js::drop_paren(&lab));
                    n.borrow_mut().kids.push(Kid { id, title, hint, href: Some("#/".into()) });
                    continue;
                }
            }
            let hint = hint.filter(|h| *h != lab);
            n.borrow_mut().kids.push(Kid { id: id.clone(), title: lab, hint, href: None });
            self.walk(k, &with(path, &id), trail.clone());
        }
    }

    fn sequence(&mut self, present: &str, under: &str) {
        let pre = format!("{under}~");
        let ids: Vec<usize> =
            (0..self.nodes.len()).filter(|&i| under == "root" || self.nodes[i].0 == under || self.nodes[i].0.starts_with(&pre)).collect();
        let step = |j: usize| Step { id: self.nodes[j].0.clone(), title: self.nodes[j].1.borrow().title.clone() };
        for (i, &at) in ids.iter().enumerate() {
            let prev = (i > 0).then(|| step(ids[i - 1]));
            let next = (i + 1 < ids.len()).then(|| step(ids[i + 1]));
            let mut n = self.nodes[at].1.borrow_mut();
            n.present = Some(present.to_string());
            // A deck inside a deck starts and ends at its own folds: no step out of it is left from an outer sequence.
            n.prev = prev;
            n.next = next;
        }
    }
}

/// fold.ts `fold`: the object as fold nodes, in the order fold.ts writes them (Map insertion order).
pub fn fold(root: &Value, opts: &FoldOpts) -> Vec<(String, Node)> {
    let tz = s(get(root, "meta").and_then(|m| get(m, "timeZone")));
    let mut f = Folder {
        opts,
        clock: Clock::new(tz.as_deref(), opts.local_time_zone.as_deref()),
        nodes: vec![],
        at: HashMap::new(),
        presenting: vec![],
    };
    f.walk(root, &[], vec![]);
    for (id, p) in std::mem::take(&mut f.presenting) {
        f.sequence(&p, &id);
    }
    f.nodes.into_iter().map(|(k, c)| (k, c.borrow().clone())).collect()
}

// ── Markdown, pages, decks, sources ─────────────────────────────────────

fn words(x: &str) -> usize {
    js::split_ws(x).filter(|w| !w.is_empty()).count()
}

/// `/[^.!?]+[.!?]+(?=\s|$)|[^.!?]+$/g`: a line's sentences.
fn sentences(l: &str) -> Option<Vec<String>> {
    let cs: Vec<char> = l.chars().collect();
    let punct = |c: char| matches!(c, '.' | '!' | '?');
    let (n, mut p, mut out) = (cs.len(), 0, vec![]);
    while p < n {
        let mut a = p;
        while a < n && !punct(cs[a]) {
            a += 1;
        }
        let mut b = a;
        while b < n && punct(cs[b]) {
            b += 1;
        }
        if a > p && b > a && (b == n || js::is_ws_char(cs[b])) {
            out.push(cs[p..b].iter().collect());
            p = b;
        } else if a > p && a == n {
            out.push(cs[p..n].iter().collect());
            p = n;
        } else {
            p = b.max(p + 1);
        }
    }
    (!out.is_empty()).then_some(out)
}

/// fold.ts `pages` at its default size, PAGE_WORDS.
pub fn pages(lines: &[String]) -> Vec<Vec<String>> {
    pages_of(lines, PAGE_WORDS)
}

/// fold.ts `byWords`: a sentence still over a page split every `size` words.
fn by_words(s: &str, size: usize) -> Vec<String> {
    let w: Vec<&str> = js::split_ws(s).filter(|w| !w.is_empty()).collect();
    w.chunks(size).map(|c| c.join(" ")).collect()
}

/// fold.ts `pages(lines, size)`: lines into screenfuls of at most `size` words, a long line split at its sentences, a sentence
/// still over a page at its words.
pub fn pages_of(lines: &[String], size: usize) -> Vec<Vec<String>> {
    let mut out: Vec<Vec<String>> = vec![vec![]];
    let mut n = 0;
    let parts = lines
        .iter()
        .flat_map(|l| if words(l) > size { sentences(l).unwrap_or_else(|| vec![l.clone()]) } else { vec![l.clone()] })
        .flat_map(|x| if words(&x) > size { by_words(&x, size) } else { vec![x] })
        .map(|x| js_trim(&x).to_string())
        .filter(|x| !x.is_empty());
    for p in parts {
        let w = words(&p);
        if n > 0 && n + w > size {
            out.push(vec![]);
            n = 0;
        }
        out.last_mut().expect("one page").push(p);
        n += w;
    }
    out
}

fn strings(v: &[String]) -> Value {
    Value::Array(v.iter().map(|x| Value::String(x.clone())).collect())
}

const MD_PAGE_WORDS: usize = 120;
const MD_PAGE_MAX: usize = 160;

/// fold.ts `mdPage`: a Markdown section's page size, grown until its pages and subsections fit one level of named runs.
pub fn md_page(lines: &[String], subsections: usize) -> usize {
    (MD_PAGE_WORDS..MD_PAGE_MAX)
        .step_by(10)
        .find(|&size| pages_of(lines, size).len() + subsections <= MAX_KIDS * MAX_KIDS)
        .unwrap_or(MD_PAGE_MAX)
}

/// fold.ts `goesOn(p, i)`: a screenful named by how it starts, id `p{i + 2}` (i may be -1: a section's first page is `p1`).
fn goes_on(p: &[String], i: i64) -> Value {
    let mut m = Map::new();
    m.insert("id".into(), Value::String(format!("p{}", i + 2)));
    m.insert("title".into(), Value::String(clip(&p[0], 60)));
    m.insert("text".into(), strings(p));
    Value::Object(m)
}

struct Md {
    title: String,
    text: Vec<String>,
    kids: Vec<usize>,
}

/// fold.ts `fromMarkdown`: each heading a fold holding its paragraphs and list items as text and its subheadings as kids.
pub fn from_markdown(md: &str) -> Value {
    let mut arena = vec![Md { title: String::new(), text: vec![], kids: vec![] }];
    let mut stack: Vec<(usize, usize)> = vec![(0, 0)];
    let mut para: Vec<String> = vec![];
    fn flush(arena: &mut [Md], stack: &[(usize, usize)], para: &mut Vec<String>) {
        if !para.is_empty() {
            arena[stack[stack.len() - 1].1].text.push(para.join(" "));
            para.clear();
        }
    }
    for line in md.split('\n') {
        if let Some((lvl, t)) = js::heading(line) {
            flush(&mut arena, &stack, &mut para);
            let title = js_trim(&t.replace(['*', '_', '`'], "")).to_string();
            while stack.len() > 1 && stack[stack.len() - 1].0 >= lvl {
                stack.pop();
            }
            if lvl == 1 && arena[0].title.is_empty() {
                arena[0].title = title;
                continue;
            }
            arena.push(Md { title, text: vec![], kids: vec![] });
            let me = arena.len() - 1;
            let top = stack[stack.len() - 1].1;
            arena[top].kids.push(me);
            stack.push((lvl, me));
        } else if let Some(rest) = js::list_item(line) {
            flush(&mut arena, &stack, &mut para);
            arena[stack[stack.len() - 1].1].text.push(rest.to_string());
        } else if !js_trim(line).is_empty() {
            para.push(js_trim(line).to_string());
        } else {
            flush(&mut arena, &stack, &mut para);
        }
    }
    flush(&mut arena, &stack, &mut para);
    fn tidy(arena: &[Md], i: usize) -> Value {
        let n = &arena[i];
        // one screen holds its text; a longer section is split once into sibling folds, ahead of its subsections (fold.ts #787)
        let ps = if n.text.is_empty() { vec![vec![]] } else { pages_of(&n.text, md_page(&n.text, n.kids.len())) };
        let own: &[String] = if ps.len() == 1 { &ps[0] } else { &[] };
        let split = if ps.len() > 1 { ps.iter().enumerate().map(|(i, p)| goes_on(p, i as i64 - 1)).collect() } else { vec![] };
        let kids: Vec<Value> = split.into_iter().chain(n.kids.iter().map(|&k| tidy(arena, k))).collect();
        let mut m = Map::new();
        m.insert("title".into(), Value::String(if n.title.is_empty() { "Untitled".into() } else { n.title.clone() }));
        if !own.is_empty() {
            m.insert("text".into(), strings(own));
        }
        if !kids.is_empty() {
            m.insert("kids".into(), Value::Array(kids));
        }
        Value::Object(m)
    }
    tidy(&arena, 0)
}

/// fold.ts `fromDeck`: a deck.json (`{meta, slides}`) as folds, one per slide. Errs where fold.ts would throw.
pub fn from_deck(d: &Value) -> Result<Value, String> {
    fn texts(b: Option<&Value>) -> Vec<String> {
        match b {
            Some(Value::Array(a)) => a.iter().flat_map(|x| texts(Some(x))).collect(),
            Some(o @ Value::Object(_)) => {
                let mut out = vec![];
                if s(get(o, "text")).is_some() {
                    out.push(raw(get(o, "text")).replace(['*', '`'], ""));
                }
                for k in ["items", "body", "rows"] {
                    out.extend(texts(get(o, k)));
                }
                out
            }
            Some(v @ Value::String(x)) if s(Some(v)).is_some() => vec![x.clone()],
            _ => vec![],
        }
    }
    let meta = get(d, "meta");
    let mut m = Map::new();
    m.insert("title".into(), Value::String(s(meta.and_then(|m| get(m, "title"))).unwrap_or_else(|| "Deck".into())));
    if s(meta.and_then(|m| get(m, "subtitle"))).is_some() {
        m.insert("text".into(), Value::Array(vec![Value::String(raw(meta.and_then(|m| get(m, "subtitle"))).to_string())]));
    }
    let mut kids = vec![];
    if let Some(Value::Array(slides)) = get(d, "slides") {
        for (i, sl) in slides.iter().enumerate() {
            if sl.is_null() {
                return Err("null is not an object (evaluating 'sl.body')".into());
            }
            let t = texts(get(sl, "body"));
            let pg = pages(&t);
            let (first, rest) = pg.split_first().expect("one page");
            let said = if s(get(sl, "transcript")).is_some() { pages(&[raw(get(sl, "transcript")).to_string()]) } else { vec![] };
            let mut sk: Vec<Value> = rest.iter().enumerate().map(|(i, p)| goes_on(p, i as i64)).collect();
            if let Some((s0, srest)) = said.split_first() {
                let mut sp = Map::new();
                sp.insert("id".into(), "spoken".into());
                sp.insert("title".into(), "What was said".into());
                sp.insert("text".into(), strings(s0));
                if !srest.is_empty() {
                    sp.insert("kids".into(), Value::Array(srest.iter().enumerate().map(|(i, p)| goes_on(p, i as i64)).collect()));
                }
                sk.push(Value::Object(sp));
            }
            let title = t.first().cloned().or_else(|| s(get(sl, "kicker"))).unwrap_or_else(|| format!("Slide {}", i + 1));
            let mut o = Map::new();
            o.insert("id".into(), Value::String(s(get(sl, "id")).unwrap_or_else(|| format!("s{}", i + 1))));
            o.insert("title".into(), Value::String(clip(&title, 70)));
            o.insert("text".into(), strings(first));
            if !sk.is_empty() {
                o.insert("kids".into(), Value::Array(sk));
            }
            kids.push(Value::Object(o));
        }
    }
    m.insert("kids".into(), Value::Array(kids));
    Ok(Value::Object(m))
}

/// JS spread `...v` into an array: arrays give their items, strings their code points; `null`/missing give nothing (as `?? []`).
fn spread(v: Option<&Value>, what: &str) -> Result<Vec<Value>, String> {
    match v {
        None | Some(Value::Null) => Ok(vec![]),
        Some(Value::Array(a)) => Ok(a.clone()),
        Some(Value::String(x)) => Ok(x.chars().map(|c| Value::String(c.to_string())).collect()),
        Some(_) => Err(format!("{what} is not iterable")),
    }
}

/// fold.ts `withSources` with a reader that says why a file could not be read (`Err(message)`).
pub fn with_sources_result(o: &Value, read: &dyn Fn(&str) -> Result<String, String>) -> Value {
    match o {
        Value::Array(a) => Value::Array(a.iter().map(|x| with_sources_result(x, read)).collect()),
        Value::Object(m) => {
            let mut out = Map::new();
            for (k, v) in m {
                out.insert(k.clone(), with_sources_result(v, read));
            }
            if s(m.get("from")).is_some() {
                let from = raw(m.get("from")).to_string();
                if let Err(e) = source_into(&mut out, &from, read) {
                    out.insert("note".into(), Value::String(format!("Could not read {from}: {e}")));
                }
                out.shift_remove("from");
            }
            Value::Object(out)
        }
        v => v.clone(),
    }
}

/// JS `[].concat(v ?? [])`: a list gives its items, anything else itself.
fn concat(v: Option<&Value>) -> Vec<Value> {
    match v {
        None | Some(Value::Null) => vec![],
        Some(Value::Array(a)) => a.clone(),
        Some(v) => vec![v.clone()],
    }
}

fn source_into(out: &mut Map<String, Value>, from: &str, read: &dyn Fn(&str) -> Result<String, String>) -> Result<(), String> {
    let t = read(from)?;
    let src = if from.ends_with(".md") {
        from_markdown(&t)
    } else {
        let j: Value = serde_json::from_str(&t).map_err(|e| e.to_string())?;
        if matches!(get(&j, "slides"), Some(Value::Array(_))) {
            from_deck(&j)?
        } else {
            j
        }
    };
    if src.is_null() {
        return Err("null is not an object (evaluating 'src.kids')".into());
    }
    let mut kids = spread(out.get("kids"), "out.kids")?;
    kids.extend(spread(get(&src, "kids"), "src.kids")?);
    kids.extend(spread(get(&src, "sections"), "src.sections")?);
    out.insert("kids".into(), Value::Array(kids));
    if let Some(p @ ("slides" | "folds")) = get(&src, "present").and_then(Value::as_str) {
        out.insert("present".into(), Value::String(p.into()));
    }
    let st = get(&src, "text");
    let has = match st {
        Some(Value::Array(a)) => !a.is_empty(),
        Some(Value::String(x)) => !x.is_empty(),
        Some(Value::Object(m)) => truthy(m.get("length")),
        _ => false,
    };
    if has {
        // text may be one string or a list: concat, never spread.
        let mut text = concat(out.get("text"));
        text.extend(concat(st));
        out.insert("text".into(), Value::Array(text));
    }
    Ok(())
}

/// fold.ts `withSources`: any object with `from` gets that file's folds as its own. `read` returns the file's text, or `None` when
/// it cannot be read; the note then says what fold.ts's `readFileSync` reader says (ENOENT).
pub fn with_sources(v: &Value, read: &dyn Fn(&str) -> Option<String>) -> Value {
    with_sources_result(v, &|p: &str| read(p).ok_or_else(|| format!("ENOENT: no such file or directory, open '{p}'")))
}
