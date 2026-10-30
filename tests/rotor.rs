//! The rotor and yank against Allie's goldens (goldens/rotor-yank/, vendored from ax-rebuild tests/fixtures/rotor-yank at main 4b6852b):
//! every `core` case, through the module's own entry (wasm::rotor_text), each step's view, mode, filter, drawer, cursor address,
//! row, live region, hint, copies and focus. `allie` cases need Allie's shell (its keys, apps and actions kinds, pieces, lines,
//! entities, the box) and are not this core's.

use std::fs;
use std::path::Path;

use serde_json::{json, Value};

/// The collector's dump (axRole, name, value, children) as the core's tree, under the machine (`host[phobos]`).
fn tree_of(dump: &Value) -> Value {
    fn node(n: &Value) -> Value {
        let role = n["axRole"].as_str().unwrap().trim_start_matches("AX").to_lowercase();
        let kids: Vec<Value> = n["children"].as_array().map(|c| c.iter().map(node).collect()).unwrap_or_default();
        let mut o = json!({ "role": role, "name": n["name"].as_str().unwrap_or(""), "kids": kids });
        if !n["value"].is_null() {
            o["value"] = n["value"].clone();
        }
        o
    }
    json!({ "role": "host", "name": "phobos", "kids": dump["tree"].as_array().unwrap().iter().map(node).collect::<Vec<_>>() })
}

/// Allie's ten kinds (allie.model.json meta.kinds): the five VoiceOver kinds by role, the rest Allie's own.
fn allie_kinds() -> Value {
    let roles = |k: &str| fold_core::rotor::default_kinds().into_iter().find(|x| x.name == k).and_then(|x| x.roles);
    let names = ["keyboard", "commands", "keys", "inputs", "buttons", "links", "headings", "windows", "apps", "actions"];
    Value::Array(
        names
            .iter()
            .map(|n| {
                let mut r = roles(n);
                if *n == "windows" {
                    r = Some(vec!["window".into(), "sheet".into(), "tabgroup".into()]);
                }
                json!({ "name": n, "say": n, "roles": r })
            })
            .collect(),
    )
}

fn first_named(t: &Value, name: &str, at: &mut Vec<usize>) -> bool {
    if t["name"] == name {
        return true;
    }
    for (i, k) in t["kids"].as_array().unwrap().iter().enumerate() {
        at.push(i);
        if first_named(k, name, at) {
            return true;
        }
        at.pop();
    }
    false
}

fn call(v: Value) -> Value {
    let out: Value = serde_json::from_str(&fold_core::wasm::rotor_text(&v.to_string())).unwrap();
    assert!(out.get("error").is_none(), "{out}");
    out
}

#[test]
fn core_cases_match_allies_goldens() {
    let dir = Path::new(env!("CARGO_MANIFEST_DIR")).join("goldens/rotor-yank");
    let kinds = allie_kinds();
    let opts = json!({ "kinds": kinds });
    let (mut run, mut skipped, mut failed) = (0, 0, vec![]);
    for file in ["kinds", "cycle", "filter", "empty", "yank"] {
        let g: Value = serde_json::from_str(&fs::read_to_string(dir.join(format!("{file}.json"))).unwrap()).unwrap();
        let dump: Value =
            serde_json::from_str(&fs::read_to_string(dir.join(format!("trees/{}.tree.json", g["tree"].as_str().unwrap()))).unwrap())
                .unwrap();
        let tree = tree_of(&dump);
        for c in g["cases"].as_array().unwrap() {
            let id = c["id"].as_str().unwrap();
            if c["scope"] != "core" {
                skipped += 1;
                continue;
            }
            run += 1;
            let mut at = vec![];
            assert!(first_named(&tree, c["start"].as_str().unwrap(), &mut at), "{id}: no node named {}", c["start"]);
            let mut state = call(json!({ "op": "init", "tree": tree, "at": at }));
            let keys = c["keys"].as_array().unwrap();
            for (i, want) in c["expect"].as_array().unwrap().iter().enumerate() {
                let (key, effects) = if i == 0 {
                    ("", json!({ "say": null, "hint": null, "copy": [] }))
                } else {
                    let k = keys[i - 1].as_str().unwrap();
                    let r = call(json!({ "op": "key", "tree": tree, "state": state, "key": k, "opts": opts }));
                    state = r["state"].clone();
                    (k, r["effects"].clone())
                };
                let row = call(json!({ "op": "row", "tree": tree, "state": state }));
                let said = |r: &Value| {
                    ["name", "role", "fact"].iter().filter_map(|k| r[k].as_str().filter(|s| !s.is_empty())).collect::<Vec<_>>().join(", ")
                };
                let view = &state["view"];
                let got = json!({
                    "key": key,
                    "view": view["kind"].as_str().unwrap_or(""),
                    "mode": view["mode"].as_str().unwrap_or(""),
                    "filter": view["filter"].as_str().unwrap_or(""),
                    "rotor": state["drawer"]["at"].as_u64().map(|i| kinds[i as usize]["name"].clone()),
                    "at": row["address"].as_str().unwrap_or(""),
                    "row": if row.is_null() { Value::Null } else { said(&row).into() },
                    "said": effects["say"].as_str().unwrap_or(""),
                    "hint": effects["hint"].as_str().unwrap_or(""),
                    "copy": effects["copy"],
                    "focus": if state["drawer"].is_null() { "ax-tree" } else { "ax-rotor-list" },
                });
                if &got != want {
                    failed.push(format!("{file}/{id} step {i} [{key}]\n  want {want}\n  got  {got}"));
                    break;
                }
            }
        }
    }
    for f in &failed {
        println!("{f}");
    }
    println!("{run} core cases, {} failed; {skipped} allie cases not this core's", failed.len());
    assert!(failed.is_empty());
    assert_eq!(run, 57, "the brief's 57 core cases");
}
