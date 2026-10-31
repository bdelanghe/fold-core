//! fold-ax.js `axOf`: one fold as the accessibility tree the viewer derives its DOM from (and Allie reads in Chrome), and
//! walk.ts `spoken`: the words a screen reader says for it. The tree is JSON in fold-ax.js's key order, keys whose value is
//! `undefined` left out, as `JSON.stringify` leaves them out.

use serde_json::{json, Map, Value};

use crate::js::{self, Clock};

/// Where a link goes: `href(path, mode)` (mode "folds" or "slides").
pub type Href<'a> = &'a dyn Fn(&[String], &str) -> String;

/// The two links axOf is given in home: the walk's (`#/a/b`, `#slides/a/b`: walk.ts, load.ts, coldread.ts) and the viewer's
/// (viewer.html: the same, each id through `encodeURIComponent`).
pub fn link(style: &str) -> Option<fn(&[String], &str) -> String> {
    fn at(m: &str) -> &'static str {
        if m == "slides" {
            "#slides/"
        } else {
            "#/"
        }
    }
    match style {
        "walk" => Some(|p, m| format!("{}{}", at(m), p.join("/"))),
        "viewer" => Some(|p, m| format!("{}{}", at(m), p.iter().map(|x| js::encode_uri_component(x)).collect::<Vec<_>>().join("/"))),
        _ => None,
    }
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

/// JS `a ?? b`: a property that is missing or `null` falls through.
fn present(v: Option<&Value>) -> Option<&Value> {
    v.filter(|x| !x.is_null())
}

/// `[].concat(v ?? [])`.
pub(crate) fn concat(v: Option<&Value>) -> Vec<Value> {
    match present(v) {
        None => vec![],
        Some(Value::Array(a)) => a.clone(),
        Some(x) => vec![x.clone()],
    }
}

/// An object literal: a key set to `undefined` (`None`) is left out.
fn obj(kv: Vec<(&str, Option<Value>)>) -> Value {
    let mut m = Map::new();
    for (k, v) in kv {
        if let Some(v) = v {
            m.insert(k.into(), v);
        }
    }
    Value::Object(m)
}

fn prop(children: Vec<Value>, cls: &str) -> Value {
    json!({ "role": "listitem", "cls": cls, "children": children })
}

fn said(name: Value) -> Value {
    prop(vec![json!({ "role": "text", "name": name })], "prop")
}

pub(crate) fn at(id: &str) -> Vec<String> {
    if id == "root" {
        vec![]
    } else {
        id.split('~').map(str::to_string).collect()
    }
}

/// The root's name in a path that has no trail to name it (a missing fold's).
pub const ROOT: &str = "Everything";

/// A fold's path as its navigation: a link to each fold above it, named by the trail (or, with none, the root's name and then
/// the ids). A fold with no way back of its own (`always`: missing, failed, or an empty root) still gets one, to the root.
fn path_nav(path: &[String], trail: Option<&Value>, href: Href, mode: &str, always: bool) -> Option<Value> {
    let steps = if path.is_empty() && always { 1 } else { path.len() };
    let crumbs: Vec<Value> = (0..steps)
        .map(|i| {
            let name = present(trail.and_then(|t| t.get(i))).cloned().unwrap_or_else(|| match (always, i) {
                (false, _) => "Home".into(),
                (true, 0) => ROOT.into(),
                (true, _) => path[i - 1].clone().into(),
            });
            let to = &path[..i.min(path.len())];
            json!({ "role": "listitem", "children": [{ "role": "link", "name": name, "href": href(to, mode) }] })
        })
        .collect();
    (steps > 0).then(|| json!({ "role": "navigation", "name": "Path", "children": [{ "role": "list", "name": "Path", "children": crumbs }] }))
}

/// The root, as a row of a fold that has no way back of its own; none on the root itself.
fn to_root(path: &[String], trail: Option<&Value>, href: Href, mode: &str) -> Option<Value> {
    let name = present(trail.and_then(|t| t.get(0))).map_or_else(|| ROOT.to_string(), |v| js::string(Some(v)));
    (!path.is_empty()).then(|| prop(vec![json!({ "role": "link", "name": format!("Back to {name}"), "href": href(&[], mode) })], "prop root"))
}

/// The act's own id: the fold's path (each id through `encodeURIComponent`, as the viewer's links write it) and the act's index
/// among the fold's acts, always the last step, so a path ending in a fold whose id is a number never reads as another's.
pub fn act_id(path: &[String], i: usize) -> String {
    path.iter().map(|p| js::encode_uri_component(p) + "/").collect::<String>() + &i.to_string()
}

/// A fold's act as an HTML form (BRIEF-basis-forms-push-signing-2026-10-31 §1): it posts its cap, call and args as hidden
/// fields to `act/<act id>` under the viewer's mount, with a native submit button. No script. The passkey (`webauthn`, or
/// `approve`) is the exception: WebAuthn runs only in a document, so where the fold links its sign page, the form GETs that
/// page (its `#i-<intent>` kept, as a GET form keeps the action's fragment), which runs the ceremony and comes back.
fn act_form(node: &Value, path: &[String]) -> Value {
    let act = &node["act"];
    let label = get(act, "label").cloned();
    let cap = js::string(get(act, "cap"));
    let passkey = matches!(cap.trim(), "webauthn" | "approve") && truthy(get(node, "href"));
    let (method, action, fields) = if passkey {
        ("get", node["href"].clone(), vec![])
    } else {
        let mut f = vec![json!(["cap", cap]), json!(["call", js::string(get(act, "call"))])];
        match present(get(act, "args")) {
            None => {}
            Some(Value::Object(m)) => f.extend(m.iter().map(|(k, v)| json!([format!("args.{k}"), field(v)]))),
            Some(v) => f.push(json!(["args", field(v)])),
        }
        ("post", format!("act/{}", act_id(path, 0)).into(), f)
    };
    let name = present(get(node, "title")).or(label.as_ref()).cloned();
    obj(vec![
        ("role", Some("form".into())),
        ("name", name),
        ("cls", Some("act".into())),
        ("method", Some(method.into())),
        ("action", Some(action)),
        ("fields", (!fields.is_empty()).then(|| fields.into())),
        ("children", Some(json!([obj(vec![("role", Some("button".into())), ("name", label), ("cls", Some("act".into()))])]))),
    ])
}

/// An arg as a field's value: a string as it is, anything else as its JSON.
fn field(v: &Value) -> String {
    match v {
        Value::String(s) => s.clone(),
        v => v.to_string(),
    }
}

/// fold-ax.js `axOf(node, path, href, mode)`. `clock`'s local zone is the machine's, where JS's `toLocaleString` reads the time.
///
/// Where fold-core leaves home's fold-ax.js (goldens/REV), on purpose: an act is a form (`act_form`); and a fold with no way back
/// of its own still has one (BRIEF-basis-forms-push-signing-2026-10-31 §5). A missing fold, a failed one (`failed`: why its data
/// could not be read) and a root with nothing in it each show the path (to the root, at least), one line saying what was asked
/// for and why there is nothing, and, off the root, a link back to it.
pub fn ax_of(node: Option<&Value>, path: &[String], href: Href, mode: &str, clock: &Clock) -> Value {
    let slides = mode == "slides";
    let Some(node) = node.filter(|n| truthy(Some(n))) else {
        let asked = if path.is_empty() { ROOT.to_string() } else { path.join(" › ") };
        let rows: Vec<Value> =
            [Some(said(format!("You asked for {asked}, and no fold has that path.").into())), to_root(path, None, href, mode)].into_iter().flatten().collect();
        let mut children: Vec<Value> = path_nav(path, None, href, mode, true).into_iter().collect();
        children.push(json!({ "role": "heading", "name": "Nothing here", "focus": true }));
        children.push(json!({ "role": "list", "name": "Nothing here", "cls": "rows", "children": rows }));
        return json!({ "role": "main", "children": children });
    };
    let trail = get(node, "trail");
    let kids = concat(get(node, "kids"));
    let title = get(node, "title").cloned();
    let steps: Vec<(&str, &str)> = if slides {
        [("prev", "Previous: "), ("next", "Next: ")].into_iter().filter(|(k, _)| truthy(get(node, k))).collect()
    } else {
        vec![]
    };

    let mut props = vec![];
    if truthy(get(node, "stale")) {
        props.push(prop(vec![json!({ "role": "text", "name": node["stale"] })], "prop stale"));
    }
    let failed = truthy(get(node, "failed"));
    if failed {
        props.push(said(node["failed"].clone()));
    }
    props.extend(concat(get(node, "text")).into_iter().map(said));
    if truthy(get(node, "act")) {
        props.push(prop(vec![act_form(node, path), json!({ "role": "status", "cls": "result" })], "prop act"));
    }
    if truthy(get(node, "href")) {
        let name = present(get(node, "hrefLabel")).unwrap_or(&node["href"]).clone();
        props.push(prop(vec![json!({ "role": "link", "name": name, "href": node["href"] })], "prop"));
    }
    for k in ["health", "checked"] {
        if truthy(get(node, k)) {
            props.push(said(node[k].clone()));
        }
    }

    let mut tail = vec![];
    for (k, say) in steps {
        let step = &node[k];
        let name = format!("{say}{}", js::string(get(step, "title")));
        let to = at(&js::string(get(step, "id")));
        tail.push(prop(vec![json!({ "role": "link", "name": name, "href": href(&to, mode) })], "prop step"));
    }
    if slides {
        tail.push(said("Arrows change slide; h goes back out.".into()));
    }
    if truthy(get(node, "present")) {
        let (name, other) = if slides { ("Show as folds", "folds") } else { ("Show as slides", "slides") };
        tail.push(prop(vec![json!({ "role": "link", "name": name, "href": href(path, other) })], "prop"));
    }
    if truthy(get(node, "updated")) {
        let from = present(get(node, "source")).map_or_else(|| "its writer".to_string(), |v| js::string(Some(v)));
        let when = match &node["updated"] {
            Value::String(s) => js::medium(s, clock),
            _ => "Invalid Date".into(),
        };
        tail.push(prop(vec![json!({ "role": "text", "name": format!("Data from {from}, {when}") })], "prop when"));
    }

    let objs = kids.iter().enumerate().map(|(i, k)| {
        let id = js::string(get(k, "id"));
        let to = present(get(k, "href")).cloned().unwrap_or_else(|| {
            let mut p = path.to_vec();
            p.push(id.clone());
            href(&p, mode).into()
        });
        let enter = format!("⏎ go in to {}", js::string(get(k, "title")));
        json!({ "role": "listitem", "index": i, "cls": "obj", "children": [
            { "role": "decoration", "cls": "n", "name": (i + 1).to_string() },
            obj(vec![("role", Some("link".into())), ("name", get(k, "title").cloned()), ("href", Some(to)), ("description", get(k, "hint").cloned())]),
            { "role": "decoration", "cls": "go", "name": "›" },
            { "role": "decoration", "cls": "enter", "name": enter },
        ] })
    });

    let list_name = format!("{}{}", if slides { "Points of " } else { "" }, js::string(title.as_ref()));
    // A root with nothing in it says so; a failed fold has said why, and both keep the way back.
    let empty = path.is_empty() && props.is_empty() && kids.is_empty();
    if empty {
        props.push(said("There is nothing in this document yet.".into()));
    }
    let lost = failed || empty;
    props.extend(to_root(path, trail, href, mode).filter(|_| lost));
    let mut children: Vec<Value> = path_nav(path, trail, href, mode, lost).into_iter().collect();
    children.push(obj(vec![("role", Some("heading".into())), ("name", title), ("focus", Some(true.into()))]));
    let rows: Vec<Value> = props.into_iter().chain(objs).chain(tail).collect();
    children.push(json!({ "role": "list", "name": list_name, "cls": "rows", "children": rows }));
    let mut main = Map::new();
    main.insert("role".into(), "main".into());
    if slides {
        main.insert("cls".into(), "slide".into());
    }
    main.insert("children".into(), children.into());
    Value::Object(main)
}

/// walk.ts `spoken`: every name and description in the tree, in order, decorations left out (assistive tech never sees them).
pub fn spoken(ax: &Value) -> Vec<String> {
    if get(ax, "role").and_then(Value::as_str) == Some("decoration") {
        return vec![];
    }
    let mut out = vec![];
    for k in ["name", "description"] {
        if truthy(get(ax, k)) {
            out.push(js::string(get(ax, k)));
        }
    }
    for c in concat(get(ax, "children")) {
        out.extend(spoken(&c));
    }
    out
}
