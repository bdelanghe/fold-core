//! Byte parity with walk.ts: every goldens/walk/<case>.input.json, walked here (wasm::walk_text, the module's own entry),
//! serializes exactly as goldens/walk/<case>.walk.json.

use std::fs;
use std::path::Path;

use serde_json::Value;

#[test]
fn walks_at_byte_parity() {
    let dir = Path::new(env!("CARGO_MANIFEST_DIR")).join("goldens/walk");
    let mut cases: Vec<_> = fs::read_dir(&dir)
        .unwrap()
        .filter_map(|e| e.ok())
        .map(|e| e.path())
        .filter(|p| p.to_string_lossy().ends_with(".input.json"))
        .collect();
    cases.sort();
    assert!(!cases.is_empty(), "no walk goldens in {}", dir.display());
    let mut failed = vec![];
    for input in &cases {
        let name = input.file_name().unwrap().to_string_lossy().trim_end_matches(".input.json").to_string();
        let want: Value = serde_json::from_str(&fs::read_to_string(dir.join(format!("{name}.walk.json"))).unwrap()).unwrap();
        let got: Value = serde_json::from_str(&fold_core::wasm::walk_text(&fs::read_to_string(input).unwrap())).unwrap();
        let (ws, gs) = (serde_json::to_string(&want).unwrap(), serde_json::to_string(&got).unwrap());
        if ws == gs {
            println!("{name}: {} findings, byte parity", want["walk"]["findings"].as_array().unwrap().len());
            continue;
        }
        println!("{name}: DIFFERS");
        for k in ["walk", "verdict", "facto"] {
            if want[k] != got[k] {
                let (w, g) = (&want[k], &got[k]);
                match (w["findings"].as_array(), g["findings"].as_array()) {
                    (Some(a), Some(b)) => {
                        let i = (0..a.len().max(b.len())).find(|&i| a.get(i) != b.get(i)).unwrap_or(0);
                        println!("  {k} folds/reached: want {}/{}, got {}/{}", w["folds"], w["reached"], g["folds"], g["reached"]);
                        println!("  finding #{i}:\n    want {:?}\n    got  {:?}", a.get(i), b.get(i));
                    }
                    _ => println!("  {k}:\n    want {w}\n    got  {g}"),
                }
            }
        }
        failed.push(name);
    }
    assert!(failed.is_empty(), "not at parity: {failed:?}");
}

#[test]
fn a_bad_walk_input_says_why() {
    let e = |s: &str| serde_json::from_str::<Value>(&fold_core::wasm::walk_text(s)).unwrap()["error"].as_str().unwrap().to_string();
    assert!(e("{").starts_with("input is not JSON"));
    assert_eq!(e("{}"), "no nodes");
    assert_eq!(e(r#"{"nodes": {}, "opts": {"budgets": []}}"#), "budgets: no view.words");
}
