//! fold-dom.js `toDom`, as text: one fold's accessibility tree (ax.rs) as the HTML of its `<main>`, so a server, the browser's
//! WASM and a native host all write the same bytes for one fold (BRIEF-fold-render-anywhere-2026-10-30, slice 1).
//!
//! The same one-role-one-element table and the same refusals as toDom: an unknown role, a role that needs a name without
//! one, a main that is not its path, heading and one list, a list holding anything but rows. Attributes are written in the
//! order toDom sets them, so a DOM's `outerHTML` of toDom's element reads as this does; text and attribute values are
//! escaped as the HTML serializer escapes them (and `<`, `>` in attributes too, as current browsers do).

use serde_json::Value;

use crate::js;

/// Each role's one element (fold-dom.js ELEMENT).
pub fn element(role: &str) -> Option<&'static str> {
    Some(match role {
        "main" => "main",
        "navigation" => "nav",
        "heading" => "h1",
        "list" => "ol",
        "listitem" => "li",
        "link" => "a",
        "button" => "button",
        "status" | "text" | "decoration" => "span",
        _ => return None,
    })
}

/// The roles that refuse to render without a name (fold-dom.js NEEDS_NAME).
pub const NEEDS_NAME: [&str; 5] = ["navigation", "heading", "list", "link", "button"];

/// What a fold's main may hold, in order: the path (its scope, as links), its heading, its one list.
pub const MAIN_ROLES: [&str; 3] = ["navigation", "heading", "list"];

fn get<'a>(o: &'a Value, k: &str) -> Option<&'a Value> {
    o.as_object().and_then(|m| m.get(k))
}

/// JS truthiness of a property.
fn truthy(v: Option<&Value>) -> bool {
    match v {
        None | Some(Value::Null) => false,
        Some(Value::Bool(b)) => *b,
        Some(Value::Number(n)) => n.as_f64().is_some_and(|f| f != 0.0 && !f.is_nan()),
        Some(Value::String(x)) => !x.is_empty(),
        Some(_) => true,
    }
}

fn role(ax: &Value) -> String {
    js::string(get(ax, "role"))
}

fn children(ax: &Value) -> &[Value] {
    get(ax, "children").and_then(Value::as_array).map_or(&[], Vec::as_slice)
}

pub(crate) fn text(s: &str) -> String {
    let mut o = String::with_capacity(s.len());
    for c in s.chars() {
        match c {
            '&' => o.push_str("&amp;"),
            '\u{a0}' => o.push_str("&nbsp;"),
            '<' => o.push_str("&lt;"),
            '>' => o.push_str("&gt;"),
            c => o.push(c),
        }
    }
    o
}

pub(crate) fn attr(s: &str) -> String {
    text(s).replace('"', "&quot;")
}

struct El {
    tag: &'static str,
    attrs: Vec<(&'static str, String)>,
    body: String,
}

impl El {
    fn write(&self) -> String {
        let attrs: String = self.attrs.iter().map(|(k, v)| format!(" {k}=\"{}\"", attr(v))).collect();
        format!("<{t}{attrs}>{}</{t}>", self.body, t = self.tag)
    }
}

/// One node and what its parent appends after it (a link's hint).
fn node(ax: &Value, n: &mut usize, row: Option<&str>) -> Result<(El, Option<String>), String> {
    let r = role(ax);
    let tag = element(&r).ok_or_else(|| format!("fold-ax: no element for role \"{r}\""))?;
    let name = get(ax, "name").filter(|v| !v.is_null()).map(|v| js::string(Some(v)));
    if NEEDS_NAME.contains(&r.as_str()) && name.as_deref().unwrap_or("").trim().is_empty() {
        return Err(format!("fold-ax: a {r} needs a name"));
    }
    let kids = children(ax);
    let roles: Vec<String> = kids.iter().map(role).collect();
    if r == "main" {
        let shape: Vec<&str> = roles.iter().map(String::as_str).filter(|x| *x != "navigation").collect();
        if shape != ["heading", "list"] || roles.iter().any(|x| !MAIN_ROLES.contains(&x.as_str())) {
            return Err(format!("fold-ax: a fold is its heading and one list, not {}", roles.join(", ")));
        }
    }
    let mut e = El { tag, attrs: vec![], body: String::new() };
    // A node's own id, as a page writes it (page.rs): its path under the document, an element id and never the data's identity. On a
    // row, the IRI of the thing it shows is its RDFa `about` (brief item 20): the element presents that thing, it is not it.
    let id = get(ax, "id").and_then(Value::as_str).filter(|x| !x.is_empty());
    if let Some(id) = id {
        e.attrs.push(("id", id.to_string()));
    }
    if truthy(get(ax, "haecceity")) {
        e.attrs.push(("about", js::string(get(ax, "haecceity"))));
    }
    if truthy(get(ax, "cls")) {
        e.attrs.push(("class", js::string(get(ax, "cls"))));
    }
    if r == "decoration" {
        e.attrs.push(("aria-hidden", "true".into()));
    }
    if r == "list" && roles.iter().any(|x| x != "listitem") {
        return Err(format!("fold-ax: a list holds rows, not {}", roles.join(", ")));
    }
    let named = name.clone().unwrap_or_default();
    if r == "list" {
        // WebKit drops list semantics under list-style:none without it.
        e.attrs.push(("role", "list".into()));
        e.attrs.push(("aria-label", named.clone()));
    }
    if r == "navigation" {
        e.attrs.push(("aria-label", named.clone()));
    }
    if r == "heading" && truthy(get(ax, "focus")) {
        e.attrs.push(("tabindex", "-1".into()));
    }
    let href = get(ax, "href").and_then(Value::as_str);
    if r == "link" {
        e.attrs.push(("href", href.ok_or("fold-ax: a link needs an href")?.to_string()));
    }
    if r == "button" {
        e.attrs.push(("type", "button".into()));
    }
    if r == "status" {
        e.attrs.push(("role", "status".into()));
    }
    // Off-page links open a new tab: clax frames the page without allow-top-navigation, so a plain link there goes nowhere.
    // A link to another fold's page (`local`, page.rs) is a move within the fold, as a `#` one is.
    if r == "link" && !href.unwrap_or("").starts_with('#') && !truthy(get(ax, "local")) {
        e.attrs.push(("target", "_blank".into()));
        e.attrs.push(("rel", "noopener".into()));
    }
    if let Some(i) = get(ax, "index").filter(|v| !v.is_null()) {
        e.attrs.push(("data-i", js::string(Some(i))));
    }
    if truthy(get(ax, "name")) && kids.is_empty() && r != "list" && r != "navigation" {
        e.body.push_str(&text(&named));
    }
    for c in kids {
        let (x, hint) = node(c, n, id.or(row))?;
        e.body.push_str(&x.write());
        e.body.extend(hint);
    }
    let mut hint = None;
    if r == "link" && truthy(get(ax, "description")) {
        // The hint is read after the link's name, as its description, and shown under it.
        // In a row with an id, the hint's is that id's (`:` is escaped in every segment, so no row's id is one); else counted.
        let d = match row {
            Some(r) => format!("{r}:hint"),
            None => {
                *n += 1;
                format!("d{n}")
            }
        };
        hint = Some(format!("<span class=\"hint\" id=\"{}\">{}</span>", attr(&d), text(&js::string(get(ax, "description")))));
        e.attrs.push(("aria-describedby", d));
    }
    Ok((e, hint))
}

/// One fold's tree as its HTML, or why toDom would refuse it.
pub fn html(ax: &Value) -> Result<String, String> {
    let (e, hint) = node(ax, &mut 0, None)?;
    Ok(e.write() + &hint.unwrap_or_default())
}
