//! The one-id vectors (goldens/ids.vectors.json): segments, quiddities, steps, haecceities and a whole document's paths, each
//! case's output recomputed from its input. Other implementations (facto's graphdb-ids, Allie) check against the same file.
//! `FOLD_BLESS=1 cargo test --test ids` writes the outputs from the inputs.

use std::fs;
use std::path::Path;

use fold_core::id;
use serde_json::{json, Value};

fn outputs(v: &Value) -> Value {
    let segment: Vec<Value> = v["segment"]
        .as_array()
        .unwrap()
        .iter()
        .map(|c| json!({ "in": c["in"], "out": id::segment_of(c["in"].as_str().unwrap()) }))
        .collect();
    let step: Vec<Value> = v["step"]
        .as_array()
        .unwrap()
        .iter()
        .map(|c| {
            let (parent, key, node) = (c["parent"].as_str().unwrap(), c["key"].as_str().unwrap(), &c["node"]);
            let kind = id::quiddity(node, key);
            let provisional = id::source_id(node).is_none();
            let named_by = if provisional { "provisional-token" } else { "source-id" };
            let mut o = json!({ "note": c["note"], "parent": parent, "key": key, "node": node, "quiddity": kind, "step": id::step(parent, node, &kind), "named_by": named_by });
            if provisional {
                o["hashed"] = json!([parent, kind.as_str(), &id::raw_name(node), &id::distinguishing(node)].join("\n"));
            }
            o
        })
        .collect();
    let haecceity: Vec<Value> = v["haecceity"]
        .as_array()
        .unwrap()
        .iter()
        .map(|c| {
            let (source, path) = (c["source"].as_str().unwrap(), c["path"].as_str().unwrap());
            json!({ "source": source, "path": path, "haecceity": id::haecceity(source, path), "html_id": id::html_id(source, path) })
        })
        .collect();
    let docs: Vec<Value> = v["document"]
        .as_array()
        .unwrap()
        .iter()
        .map(|c| {
            let ps = id::paths(&c["data"], c["source"].as_str().unwrap());
            let unique = match id::unique(ps.iter().map(|p| p.0.as_str())) {
                Ok(()) => Value::Null,
                Err(dup) => json!(dup),
            };
            let rows: Vec<Value> = ps
                .iter()
                .map(|(p, q, h)| json!({ "path": p, "quiddity": q, "haecceity": h, "named_by": if p.rsplit('/').next().is_some_and(|s| s.starts_with('~')) { "provisional-token" } else { "source-id" } }))
                .collect();
            json!({ "note": c["note"], "source": c["source"], "data": c["data"], "paths": rows, "duplicate": unique })
        })
        .collect();
    let mut out = v.clone();
    out["segment"] = json!(segment);
    out["step"] = json!(step);
    out["haecceity"] = json!(haecceity);
    out["document"] = json!(docs);
    out
}

#[test]
fn vectors() {
    let f = Path::new(env!("CARGO_MANIFEST_DIR")).join("goldens/ids.vectors.json");
    let v: Value = serde_json::from_str(&fs::read_to_string(&f).unwrap()).unwrap();
    let got = outputs(&v);
    if std::env::var("FOLD_BLESS").is_ok() {
        fs::write(&f, serde_json::to_string_pretty(&got).unwrap() + "\n").unwrap();
        return;
    }
    assert_eq!(got, v, "goldens/ids.vectors.json: an output differs from its input's (FOLD_BLESS=1 rewrites them)");
}
