//! Byte parity with fold-ax.js axOf: every [id, mode, href, tree] in goldens/ax/<case>.ax.json, built here from the fold in
//! goldens/walk/<case>.input.json (wasm::ax_text, the module's own entry), serializes exactly as its tree.

use std::fs;
use std::path::Path;

use serde_json::{json, Value};

#[test]
fn trees_at_byte_parity() {
    let dir = Path::new(env!("CARGO_MANIFEST_DIR")).join("goldens");
    let mut cases: Vec<_> = fs::read_dir(dir.join("ax")).unwrap().filter_map(|e| e.ok()).map(|e| e.path()).collect();
    cases.sort();
    assert!(!cases.is_empty(), "no ax goldens");
    let (mut trees, mut failed) = (0, vec![]);
    for p in &cases {
        let name = p.file_name().unwrap().to_string_lossy().trim_end_matches(".ax.json").to_string();
        let input: Value = serde_json::from_str(&fs::read_to_string(dir.join(format!("walk/{name}.input.json"))).unwrap()).unwrap();
        let mut nodes = serde_json::Map::new();
        for pair in input["nodes"].as_array().unwrap() {
            nodes.insert(pair[0].as_str().unwrap().into(), pair[1].clone());
        }
        let rows: Vec<(String, String, String, Value)> = serde_json::from_str(&fs::read_to_string(p).unwrap()).unwrap();
        for (id, mode, href, want) in rows {
            let path: Vec<&str> = if id == "root" { vec![] } else { id.split('~').collect() };
            let case = json!({ "node": nodes.get(&id), "path": path, "mode": mode, "href": href, "localTimeZone": input["opts"]["localTimeZone"] });
            let got = fold_core::wasm::ax_text(&case.to_string());
            trees += 1;
            if got != serde_json::to_string(&want).unwrap() {
                println!("{name} {id} {mode} {href}: DIFFERS\n  want {want}\n  got  {got}");
                failed.push(format!("{name} {id} {mode} {href}"));
            }
        }
    }
    println!("{trees} trees");
    assert!(failed.is_empty(), "not at parity: {failed:?}");
}

#[test]
fn a_bad_ax_input_says_why() {
    let e = |s: &str| serde_json::from_str::<Value>(&fold_core::wasm::ax_text(s)).unwrap()["error"].as_str().unwrap().to_string();
    assert!(e("{").starts_with("input is not JSON"));
    assert_eq!(e(r#"{"href": "other"}"#), "href: other is not walk or viewer");
}
