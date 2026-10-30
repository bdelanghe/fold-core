//! The rotor and yank (ax-rebuild briefs/rotor-yank-core.md): Allie's kinds, its drawer, its filter and its two copies, as pure
//! functions over a tree the host hands in. No DOM, no clock, no clipboard: a key gives the next state and the effects
//! (what the live region says, the quiet line under the path, what to copy), and the host performs them.
//!
//! A kind is a view of the place you are in: the nodes under it whose role is one of the kind's, in tree order, the menu bar
//! never. Inside a kind the cursor stands on a match's real path. The goldens are goldens/rotor-yank/ (Allie's, vendored).

use serde::{Deserialize, Deserializer, Serialize};
use serde_json::Value;

/// One node of the host's tree.
#[derive(Deserialize, Serialize, Clone, Debug, Default)]
pub struct Node {
    pub role: String,
    #[serde(default)]
    pub name: String,
    #[serde(default, deserialize_with = "text", skip_serializing_if = "Option::is_none")]
    pub value: Option<String>,
    #[serde(default, rename = "roleDescription", skip_serializing_if = "Option::is_none")]
    pub role_description: Option<String>,
    #[serde(default)]
    pub kids: Vec<Node>,
    /// A whole clipboard entry, copied by `y` in place of the name and value.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub clip: Option<String>,
}

/// A value as the host gives it (a string, or a number or flag read as its text).
fn text<'de, D: Deserializer<'de>>(d: D) -> Result<Option<String>, D::Error> {
    Ok(match Value::deserialize(d)? {
        Value::Null => None,
        Value::String(s) => Some(s),
        v => Some(v.to_string()),
    })
}

/// A kind, as the host lists them (its order is the order `[` `]` and the drawer go). A kind with no roles is the host's own
/// (Allie's keyboard, commands, keys, apps, actions): opening it is handed back as `effects.host`.
#[derive(Deserialize, Serialize, Clone, Debug)]
pub struct Kind {
    pub name: String,
    pub say: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub roles: Option<Vec<String>>,
}

/// The kinds line as Allie numbers it: ⌘d (or the digit d in the drawer) is the kind named ORDER[(d + 9) mod 10].
const ORDER: [&str; 10] = ["keyboard", "commands", "keys", "inputs", "buttons", "links", "headings", "windows", "apps", "actions"];

/// The core's own kinds, for a host that gives none: the five VoiceOver kinds and lists.
pub fn default_kinds() -> Vec<Kind> {
    let k = |name: &str, roles: &[&str]| Kind {
        name: name.into(),
        say: name.into(),
        roles: Some(roles.iter().map(|r| r.to_string()).collect()),
    };
    vec![
        k("inputs", &["textfield", "textarea", "searchfield", "combobox"]),
        k("buttons", &["button", "menubutton", "popupbutton", "checkbox", "radiobutton"]),
        k("links", &["link"]),
        k("headings", &["heading"]),
        k("windows", &["window", "sheet", "tabgroup", "main", "navigation", "region"]),
        k("lists", &["list"]),
    ]
}

pub type Cursor = Vec<usize>;

#[derive(Deserialize, Serialize, Clone, Debug, Default, PartialEq)]
pub struct View {
    pub kind: String,
    pub filter: String,
    /// `normal` or `filter`
    pub mode: String,
    /// where the cursor was before the first kind opened: leaving goes back there
    pub from: Cursor,
    /// the matches the filter keeps, in tree order
    pub matches: Vec<Cursor>,
    pub at: usize,
}

#[derive(Deserialize, Serialize, Clone, Debug, Default, PartialEq)]
pub struct Drawer {
    pub at: usize,
}

#[derive(Deserialize, Serialize, Clone, Debug, Default, PartialEq)]
pub struct State {
    pub at: Cursor,
    pub view: Option<View>,
    pub drawer: Option<Drawer>,
    /// the last kind opened (`r r`)
    #[serde(default)]
    pub last: String,
    /// digits typed before a move (vim's 3j)
    #[serde(default)]
    pub count: String,
    /// the line under the path as the last key left it: the same line again is not written again
    #[serde(default)]
    pub hint: String,
}

#[derive(Deserialize, Serialize, Clone, Debug, Default, PartialEq)]
pub struct HostOp {
    pub op: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub arg: Option<String>,
}

#[derive(Serialize, Clone, Debug, Default, PartialEq)]
pub struct Effects {
    /// the live region (a screen reader speaks it)
    pub say: Option<String>,
    /// the quiet line under the path
    pub hint: Option<String>,
    /// what this key copied, in order (the host puts the last one on the clipboard)
    pub copy: Vec<String>,
    /// where the keyboard goes, when this key moved it: `tree` or `drawer`
    #[serde(skip_serializing_if = "Option::is_none")]
    pub focus: Option<String>,
    /// keys for the host's own kinds, handed back
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub host: Vec<HostOp>,
}

#[derive(Serialize, Clone, Debug, PartialEq)]
pub struct KeyResult {
    pub state: State,
    pub effects: Effects,
    pub handled: bool,
}

#[derive(Deserialize, Clone, Debug, Default)]
pub struct Opts {
    #[serde(default)]
    pub kinds: Option<Vec<Kind>>,
    /// single-letter keys on (false while VoiceOver runs unless the person turned them on)
    #[serde(default)]
    pub letters: Option<bool>,
    /// the subtree the kinds list from (the seat, else the app); the root when absent
    #[serde(default)]
    pub place: Option<Cursor>,
}

/// What the cursor row says: `name, role[, fact]`.
#[derive(Serialize, Clone, Debug, PartialEq)]
pub struct Row {
    pub name: String,
    pub role: String,
    pub fact: String,
    /// the spoken path (`phobos › Fixture › Needs you › Signing › Approve`)
    pub path: String,
    /// role[name] per step from the root
    pub address: String,
}

impl Row {
    pub fn said(&self) -> String {
        [&self.name, &self.role, &self.fact].iter().filter(|s| !s.is_empty()).map(|s| s.as_str()).collect::<Vec<_>>().join(", ")
    }
}

/// A role's word, as Allie says it (copy.json SAY).
pub fn role_word(role: &str) -> &str {
    match role {
        "statictext" => "text",
        "textfield" => "text field",
        "searchfield" => "search field",
        "combobox" => "combo box",
        "textarea" => "text box",
        "radiobutton" => "radio button",
        "popupbutton" => "pop-up menu",
        "menubutton" => "menu button",
        "tabgroup" => "tabs",
        r => r,
    }
}

/// The places a match is said to be in (the label column in a kind).
const PLACES: [&str; 3] = ["window", "sheet", "tabgroup"];
/// Nothing under these is ever a match.
const MENUS: [&str; 2] = ["menubar", "menu"];

pub fn node<'a>(tree: &'a Node, at: &[usize]) -> Option<&'a Node> {
    at.iter().try_fold(tree, |n, &i| n.kids.get(i))
}

fn chain<'a>(tree: &'a Node, at: &[usize]) -> Vec<&'a Node> {
    let mut out = vec![tree];
    for &i in at {
        match out.last().unwrap().kids.get(i) {
            Some(n) => out.push(n),
            None => break,
        }
    }
    out
}

/// Every node in tree order (parents before children), the root first.
fn preorder(tree: &Node) -> Vec<Cursor> {
    fn go(n: &Node, at: &mut Cursor, out: &mut Vec<Cursor>) {
        out.push(at.clone());
        for (i, k) in n.kids.iter().enumerate() {
            at.push(i);
            go(k, at, out);
            at.pop();
        }
    }
    let mut out = vec![];
    go(tree, &mut vec![], &mut out);
    out
}

/// A kind's matches under `place`, in tree order, the menu bar never; then the filter's.
fn matches(tree: &Node, place: &[usize], roles: &[String], filter: &str) -> Vec<Cursor> {
    fn go(n: &Node, at: &mut Cursor, roles: &[String], out: &mut Vec<Cursor>) {
        for (i, k) in n.kids.iter().enumerate() {
            if MENUS.contains(&k.role.as_str()) {
                continue;
            }
            at.push(i);
            if roles.iter().any(|r| r == &k.role) {
                out.push(at.clone());
            }
            go(k, at, roles, out);
            at.pop();
        }
    }
    let mut out = vec![];
    if let Some(p) = node(tree, place) {
        go(p, &mut place.to_vec(), roles, &mut out);
    }
    let f = filter.to_lowercase();
    out.retain(|at| {
        let n = node(tree, at).unwrap();
        let text = format!("{} {} {}", n.name, n.value.as_deref().unwrap_or(""), n.role_description.as_deref().unwrap_or(""));
        f.is_empty() || text.to_lowercase().contains(&f)
    });
    out
}

/// role[name] per step from the root, a sibling with the same role and name numbered #2, #3.
pub fn address(tree: &Node, at: &[usize]) -> String {
    let mut steps = vec![format!("{}[{}]", tree.role, tree.name)];
    let mut n = tree;
    for &i in at {
        let k = &n.kids[i];
        let same = n.kids[..i].iter().filter(|s| s.role == k.role && s.name == k.name).count();
        steps.push(if same > 0 { format!("{}[{}]#{}", k.role, k.name, same + 1) } else { format!("{}[{}]", k.role, k.name) });
        n = k;
    }
    steps.join("/")
}

/// The spoken path: the named steps from the root, joined by ›.
pub fn spoken(tree: &Node, at: &[usize]) -> String {
    chain(tree, at).iter().map(|n| n.name.as_str()).filter(|s| !s.is_empty()).collect::<Vec<_>>().join(" › ")
}

fn kinds_of(opts: &Opts) -> Vec<Kind> {
    opts.kinds.clone().unwrap_or_else(default_kinds)
}

pub fn init(_tree: &Node, at: Cursor) -> State {
    State { at, ..State::default() }
}

/// What the cursor row says; in an empty kind its one row ("No links here."); None off the tree.
pub fn row(tree: &Node, state: &State) -> Option<Row> {
    if let Some(v) = &state.view {
        if v.matches.is_empty() {
            return Some(Row {
                name: format!("No {} here.", v.kind),
                role: "".into(),
                fact: "".into(),
                path: "".into(),
                address: "".into(),
            });
        }
    }
    let at = &state.at;
    let n = node(tree, at)?;
    let fact = match &state.view {
        // in a kind: the place the match is in
        Some(_) => {
            let c = chain(tree, at);
            c[..c.len() - 1]
                .iter()
                .rev()
                .find(|a| PLACES.contains(&a.role.as_str()))
                .map(|a| role_word(&a.role).to_string())
                .unwrap_or_default()
        }
        None => String::new(),
    };
    Some(Row { name: n.name.clone(), role: role_word(&n.role).into(), fact, path: spoken(tree, at), address: address(tree, at) })
}

/// The drawer's rows: has = anything here (never how many), null for the host's own kinds.
pub fn kinds(tree: &Node, _state: &State, opts: &Opts) -> Vec<(String, Option<bool>)> {
    let place = opts.place.clone().unwrap_or_default();
    kinds_of(opts).iter().map(|k| (k.name.clone(), k.roles.as_ref().map(|r| !matches(tree, &place, r, "").is_empty()))).collect()
}

/// The first `n` words of `s`, then " …" when cut.
fn words(s: &str, n: usize) -> String {
    let w: Vec<&str> = s.split_whitespace().collect();
    if w.len() > n {
        format!("{} …", w[..n].join(" "))
    } else {
        w.join(" ")
    }
}

struct Key<'a> {
    tree: &'a Node,
    kinds: Vec<Kind>,
    place: Cursor,
    s: State,
    e: Effects,
    hint: Option<String>,
}

impl Key<'_> {
    fn say(&mut self, t: impl Into<String>) {
        self.e.say = Some(t.into());
    }

    fn kind_index(&self, name: &str) -> Option<usize> {
        self.kinds.iter().position(|k| k.name == name)
    }

    /// The cursor row as the tree says it (no label column).
    fn row_words(&self) -> String {
        row(self.tree, &self.s).map(|r| Row { fact: String::new(), ..r }.said()).unwrap_or_default()
    }

    fn open(&mut self, name: &str) {
        let Some(i) = self.kind_index(name) else { return };
        let k = self.kinds[i].clone();
        let Some(roles) = &k.roles else {
            self.e.host.push(HostOp { op: "kind".into(), arg: Some(k.name.clone()) });
            return;
        };
        let from = self.s.view.as_ref().map(|v| v.from.clone()).unwrap_or_else(|| self.s.at.clone());
        let m = matches(self.tree, &self.place, roles, "");
        let at = m.iter().position(|c| c == &from).unwrap_or(0);
        self.s.at = m.get(at).cloned().unwrap_or_else(|| from.clone());
        self.s.view = Some(View { kind: k.name.clone(), filter: "".into(), mode: "normal".into(), from, matches: m, at });
        self.s.last = k.name.clone();
        self.say(k.say);
    }

    fn leave(&mut self) {
        if let Some(v) = self.s.view.take() {
            self.s.at = v.from;
        }
    }

    /// ⌘n: open that kind, or leave it when you are in it.
    fn command(&mut self, d: u32) -> bool {
        let name = ORDER[((d + 9) % 10) as usize];
        if self.kind_index(name).is_none() {
            return false;
        }
        if self.s.view.as_ref().is_some_and(|v| v.kind == name) {
            self.leave();
        } else {
            self.open(name);
        }
        true
    }

    fn refilter(&mut self, filter: String) {
        let v = self.s.view.as_mut().unwrap();
        let roles = self.kinds.iter().find(|k| k.name == v.kind).and_then(|k| k.roles.clone()).unwrap_or_default();
        v.matches = matches(self.tree, &self.place, &roles, &filter);
        v.filter = filter;
        v.at = 0;
        if let Some(c) = v.matches.first() {
            self.s.at = c.clone();
        }
    }

    fn step(&mut self, by: isize, wrap: bool) {
        let v = self.s.view.as_mut().unwrap();
        let n = v.matches.len() as isize;
        if n == 0 {
            return;
        }
        let to = v.at as isize + by;
        v.at = if wrap { to.rem_euclid(n) } else { to.clamp(0, n - 1) } as usize;
        self.s.at = v.matches[v.at].clone();
    }

    fn yank(&mut self, with_path: bool) {
        let empty = self.s.view.as_ref().is_some_and(|v| v.matches.is_empty());
        let Some(n) = (!empty).then(|| node(self.tree, &self.s.at)).flatten() else {
            self.say("nothing to yank here");
            return;
        };
        let mut copy = match (&n.clip, n.value.as_deref()) {
            (Some(c), _) => c.clone(),
            (None, Some(v)) if !v.is_empty() => format!("{} — {}", n.name, v),
            _ => n.name.clone(),
        };
        let what = words(&n.name, 4);
        if with_path {
            copy = format!("{copy}\n\n{}\n{}", spoken(self.tree, &self.s.at), address(self.tree, &self.s.at));
            self.hint = Some(format!("yanked with its path: {what}"));
        } else {
            self.hint = Some(format!("yanked: {what}"));
        }
        self.e.copy.push(copy);
    }

    fn drawer(&mut self, key: &str) -> bool {
        let at = self.s.drawer.as_ref().unwrap().at;
        let last = self.kinds.len().saturating_sub(1);
        match key {
            "j" | "ArrowDown" | "]" if at < last => self.s.drawer = Some(Drawer { at: at + 1 }),
            "j" | "ArrowDown" | "]" => self.say("last kind"),
            "k" | "ArrowUp" | "[" if at > 0 => self.s.drawer = Some(Drawer { at: at - 1 }),
            "k" | "ArrowUp" | "[" => self.say("first kind"),
            "Enter" => self.pick(self.kinds[at].name.clone()),
            "r" => self.pick(if self.s.last.is_empty() { self.kinds[at].name.clone() } else { self.s.last.clone() }),
            "Escape" | "h" | "ArrowLeft" => {
                self.s.drawer = None;
                self.e.focus = Some("tree".into());
                let w = self.row_words();
                let where_ = match &self.s.view {
                    Some(v) => format!("{} view: {w}", v.kind),
                    None => format!("tree: {w}"),
                };
                self.say(where_);
            }
            k if k.len() == 1 && k.as_bytes()[0].is_ascii_digit() => {
                let name = ORDER[((k.as_bytes()[0] - b'0') as usize + 9) % 10];
                if self.kind_index(name).is_some() {
                    self.pick(name.into());
                }
            }
            _ => {}
        }
        true
    }

    fn pick(&mut self, name: String) {
        self.s.drawer = None;
        self.e.focus = Some("tree".into());
        match &self.s.view {
            Some(v) if v.kind == name => {
                let w = self.row_words();
                self.say(format!("{name} view: {w}"));
            }
            _ => self.open(&name),
        }
    }

    fn open_drawer(&mut self) {
        let name = match &self.s.view {
            Some(v) => v.kind.clone(),
            None => [self.s.last.as_str(), "keys"].into_iter().find(|n| self.kind_index(n).is_some()).unwrap_or("").to_string(),
        };
        let at = self.kind_index(&name).or_else(|| self.kinds.iter().position(|k| k.roles.is_some())).unwrap_or(0);
        self.s.drawer = Some(Drawer { at });
        self.e.focus = Some("drawer".into());
    }

    /// `[` `]` inside a kind: the previous or next kind in the host's order, no wrap.
    fn cycle(&mut self, by: isize) {
        let i = self.kind_index(&self.s.view.as_ref().unwrap().kind).unwrap_or(0) as isize + by;
        match self.kinds.get(i.max(-1) as usize).filter(|_| i >= 0) {
            Some(k) => self.open(&k.name.clone()),
            None => self.say(if by > 0 { "last kind" } else { "first kind" }),
        }
    }

    fn take_count(&mut self) -> isize {
        std::mem::take(&mut self.s.count).parse().unwrap_or(1).max(1)
    }

    fn walk(&mut self, by: isize) {
        let all = preorder(self.tree);
        let i = all.iter().position(|c| c == &self.s.at).unwrap_or(0) as isize;
        // the root is the place's machine, not a row
        self.s.at = all[(i + by).clamp(1.min(all.len() as isize - 1), all.len() as isize - 1) as usize].clone();
    }

    fn run(&mut self, key: &str) -> bool {
        if self.s.drawer.is_some() {
            return self.drawer(key);
        }
        if let Some(d) = key.strip_prefix('⌘').and_then(|d| d.parse::<u32>().ok()).filter(|d| *d < 10) {
            return self.command(d);
        }
        let filtering = self.s.view.as_ref().is_some_and(|v| v.mode == "filter");
        if filtering {
            let f = self.s.view.as_ref().unwrap().filter.clone();
            match key {
                "Enter" => self.s.view.as_mut().unwrap().mode = "normal".into(),
                "Escape" => {
                    self.s.view.as_mut().unwrap().mode = "normal".into();
                    self.refilter(String::new());
                }
                "Backspace" if f.is_empty() => self.s.view.as_mut().unwrap().mode = "normal".into(),
                "Backspace" => {
                    let mut f = f;
                    f.pop();
                    self.refilter(f);
                }
                k if k.chars().count() == 1 && k != "[" && k != "]" => self.refilter(f + k),
                _ => return false,
            }
            return true;
        }
        let in_kind = self.s.view.is_some();
        if key.len() == 1 && key.as_bytes()[0].is_ascii_digit() && (key != "0" || !self.s.count.is_empty()) {
            self.s.count.push_str(key);
            self.hint = Some(self.s.count.clone());
            return true;
        }
        let n = if matches!(key, "j" | "k" | "ArrowDown" | "ArrowUp") { self.take_count() } else { 1 };
        self.s.count.clear();
        match key {
            "j" | "ArrowDown" if in_kind => self.step(n, false),
            "k" | "ArrowUp" if in_kind => self.step(-n, false),
            "j" | "ArrowDown" => self.walk(n),
            "k" | "ArrowUp" => self.walk(-n),
            "h" | "ArrowLeft" | "Escape" | "Backspace" if in_kind => self.leave(),
            "h" | "ArrowLeft" => {
                if self.s.at.len() > self.place.len() + 1 {
                    self.s.at.pop();
                }
            }
            "l" | "ArrowRight" | "Enter" if !in_kind => match node(self.tree, &self.s.at) {
                Some(n) if !n.kids.is_empty() => self.s.at.push(0),
                _ => return false,
            },
            "]" if in_kind => self.cycle(1),
            "[" if in_kind => self.cycle(-1),
            "]" | "[" => {
                let t = format!("{key} isn't a key here. i to type, ? for keys");
                self.say(t.clone());
                self.hint = Some(t);
            }
            "/" if in_kind => self.s.view.as_mut().unwrap().mode = "filter".into(),
            "n" | "N" if in_kind => {
                if !self.s.view.as_ref().unwrap().filter.is_empty() {
                    self.step(if key == "n" { 1 } else { -1 }, true);
                }
            }
            "r" => self.open_drawer(),
            "y" => self.yank(false),
            "Y" => self.yank(true),
            _ => return false,
        }
        true
    }
}

const LETTERS: [&str; 10] = ["j", "k", "r", "y", "Y", "n", "N", "h", "l", "/"];

/// One key: the next state and what to do. `handled: false` is a key the core does not own (the host runs its own).
pub fn key(tree: &Node, state: &State, key: &str, opts: &Opts) -> KeyResult {
    let mut k = Key {
        tree,
        kinds: kinds_of(opts),
        place: opts.place.clone().unwrap_or_default(),
        s: state.clone(),
        e: Effects::default(),
        hint: None,
    };
    let filtering = state.view.as_ref().is_some_and(|v| v.mode == "filter");
    let handled = if opts.letters == Some(false) && !filtering && LETTERS.contains(&key) {
        k.say("letters are off while VoiceOver runs");
        true
    } else {
        k.run(key)
    };
    // the line under the path: written only when this key changes it
    let hint = k.hint.take().unwrap_or_default();
    if !hint.is_empty() && hint != state.hint {
        k.e.hint = Some(hint.clone());
    }
    k.s.hint = hint;
    KeyResult { state: k.s, effects: k.e, handled }
}
