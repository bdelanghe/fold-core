//! fold-ax.js `axOf`: one fold as the accessibility tree the viewer derives its DOM from (and Allie reads in Chrome), and
//! walk.ts `spoken`: the words a screen reader says for it. The tree is JSON in fold-ax.js's key order, keys whose value is
//! `undefined` left out, as `JSON.stringify` leaves them out.

use serde_json::{json, Map, Value};

use crate::js::{self, Clock};

/// Where a link goes: `href(path, mode)` (mode "folds" or "slides").
pub type Href<'a> = &'a dyn Fn(&[String], &str) -> String;

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

fn at(id: &str) -> Vec<String> {
    if id == "root" {
        vec![]
    } else {
        id.split('~').map(str::to_string).collect()
    }
}

/// fold-ax.js `axOf(node, path, href, mode)`. `clock`'s local zone is the machine's, where JS's `toLocaleString` reads the time.
pub fn ax_of(node: Option<&Value>, path: &[String], href: Href, mode: &str, clock: &Clock) -> Value {
    let slides = mode == "slides";
    let Some(node) = node.filter(|n| truthy(Some(n))) else {
        return json!({ "role": "main", "children": [
            { "role": "heading", "name": "Nothing here yet" },
            { "role": "list", "name": "Nothing here yet", "cls": "rows", "children": [said("This fold has no data.".into())] },
        ] });
    };
    let trail = get(node, "trail");
    let crumbs: Vec<Value> = (0..path.len())
        .map(|i| {
            let name = present(trail.and_then(|t| t.get(i))).cloned().unwrap_or_else(|| "Home".into());
            json!({ "role": "listitem", "children": [{ "role": "link", "name": name, "href": href(&path[..i], mode) }] })
        })
        .collect();
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
    props.extend(concat(get(node, "text")).into_iter().map(said));
    if truthy(get(node, "act")) {
        let label = get(&node["act"], "label").cloned();
        props.push(prop(
            vec![
                obj(vec![("role", Some("button".into())), ("name", label), ("cls", Some("act".into()))]),
                json!({ "role": "status", "cls": "result" }),
            ],
            "prop act",
        ));
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
    let mut children = vec![];
    if !path.is_empty() {
        children
            .push(json!({ "role": "navigation", "name": "Path", "children": [{ "role": "list", "name": "Path", "children": crumbs }] }));
    }
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
