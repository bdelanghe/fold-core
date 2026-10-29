//! schema/node.schema.json is the fold node types (Node, Kid, Step) as JSON Schema, generated from them: a type change without
//! regenerating it fails here (`FOLD_SCHEMA_WRITE=1 cargo test --test schema` writes it). Every golden's folds read as those types.
#![cfg(feature = "schema")]

use std::fs;
use std::path::Path;

use fold_core::Node;
use serde_json::Value;

#[test]
fn schema_is_the_node_types() {
    let path = Path::new(env!("CARGO_MANIFEST_DIR")).join("schema/node.schema.json");
    // Every optional field is left out when absent (skip_serializing_if), never null: optional, not nullable.
    let mut settings = schemars::generate::SchemaSettings::draft2020_12();
    settings.contract = schemars::generate::Contract::Serialize;
    settings.transforms.push(Box::new(schemars::transform::RecursiveTransform(|s: &mut schemars::Schema| {
        if let Some(Value::Array(ts)) = s.get_mut("type") {
            ts.retain(|t| t != "null");
            if let [one] = ts.as_slice() {
                let one = one.clone();
                s.insert("type".into(), one);
            }
        }
        if let Some(Value::Array(xs)) = s.get_mut("anyOf") {
            xs.retain(|x| x.get("type").is_none_or(|t| t != "null"));
            if let [Value::Object(one)] = xs.as_slice() {
                let one = one.clone();
                s.remove("anyOf");
                for (k, v) in one {
                    s.insert(k, v);
                }
            }
        }
    })));
    let gen = settings.into_generator();
    let got = serde_json::to_string_pretty(&gen.into_root_schema_for::<Node>()).unwrap() + "\n";
    if std::env::var_os("FOLD_SCHEMA_WRITE").is_some() {
        fs::create_dir_all(path.parent().unwrap()).unwrap();
        fs::write(&path, &got).unwrap();
    }
    assert_eq!(
        fs::read_to_string(&path).unwrap_or_default(),
        got,
        "schema/node.schema.json is stale: FOLD_SCHEMA_WRITE=1 cargo test --test schema"
    );
}

#[test]
fn golden_folds_read_as_the_node_types() {
    let dir = Path::new(env!("CARGO_MANIFEST_DIR")).join("goldens");
    for e in fs::read_dir(&dir).unwrap().filter_map(|e| e.ok()) {
        let p = e.path();
        if !p.to_string_lossy().ends_with(".folds.json") {
            continue;
        }
        let folds: Vec<(String, Value)> = serde_json::from_str(&fs::read_to_string(&p).unwrap()).unwrap();
        for (id, n) in folds {
            let node: Node = serde_json::from_value(n.clone()).unwrap_or_else(|e| panic!("{} {id}: {e}", p.display()));
            assert_eq!(serde_json::to_value(&node).unwrap(), n, "{} {id}: fields the types do not hold", p.display());
        }
    }
}
