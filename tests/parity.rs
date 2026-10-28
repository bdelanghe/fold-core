//! Byte parity with the fold viewer fold-core was ported from: every goldens/<case>.input.json, folded here, serializes exactly as goldens/<case>.folds.json.

use std::fs;
use std::path::Path;

use fold_core::{fold, with_sources, FoldOpts};
use serde_json::Value;

/// The first place two fold lists differ: the fold id and the field.
fn diff(want: &Value, got: &Value) -> String {
    let (w, g) = (want.as_array().unwrap(), got.as_array().unwrap());
    for i in 0..w.len().max(g.len()) {
        let (Some(a), Some(b)) = (w.get(i), g.get(i)) else {
            return format!(
                "length: want {} folds, got {}; first unmatched at {i}: want {:?}, got {:?}",
                w.len(),
                g.len(),
                w.get(i).map(|x| &x[0]),
                g.get(i).map(|x| &x[0])
            );
        };
        if a == b && serde_json::to_string(a).unwrap() == serde_json::to_string(b).unwrap() {
            continue;
        }
        if a[0] != b[0] {
            return format!("fold #{i}: want id {}, got id {}", a[0], b[0]);
        }
        let (an, bn) = (a[1].as_object().unwrap(), b[1].as_object().unwrap());
        let keys = |m: &serde_json::Map<String, Value>| m.keys().cloned().collect::<Vec<_>>();
        if keys(an) != keys(bn) {
            return format!("fold {}: keys differ\n  want {:?}\n  got  {:?}", a[0], keys(an), keys(bn));
        }
        for (k, v) in an {
            if Some(v) != bn.get(k) {
                return format!("fold {} field {k}:\n  want {}\n  got  {}", a[0], v, bn[k]);
            }
        }
        return format!("fold {}: same values, different serialization", a[0]);
    }
    "no difference found in values (key order?)".into()
}

#[test]
fn goldens_at_byte_parity() {
    let dir = Path::new(env!("CARGO_MANIFEST_DIR")).join("goldens");
    let mut cases: Vec<_> = fs::read_dir(&dir)
        .unwrap()
        .filter_map(|e| e.ok())
        .map(|e| e.path())
        .filter(|p| p.to_string_lossy().ends_with(".input.json"))
        .collect();
    cases.sort();
    assert!(!cases.is_empty(), "no goldens in {}", dir.display());
    let mut failed = vec![];
    for input in &cases {
        let name = input.file_name().unwrap().to_string_lossy().trim_end_matches(".input.json").to_string();
        let case: Value = serde_json::from_str(&fs::read_to_string(input).unwrap()).unwrap();
        let want: Value = serde_json::from_str(&fs::read_to_string(dir.join(format!("{name}.folds.json"))).unwrap()).unwrap();
        let opts: FoldOpts = serde_json::from_value(case["opts"].clone()).unwrap();
        let files = case["files"].as_object().cloned().unwrap_or_default();
        let read = |p: &str| files.get(p).and_then(Value::as_str).map(str::to_string);
        let got = serde_json::to_value(fold(&with_sources(&case["root"], &read), &opts)).unwrap();
        let (ws, gs) = (serde_json::to_string(&want).unwrap(), serde_json::to_string(&got).unwrap());
        if ws == gs {
            println!("{name}: {} folds, byte parity", want.as_array().unwrap().len());
        } else {
            println!("{name}: DIFFERS\n{}", diff(&want, &got));
            failed.push(name);
        }
    }
    assert!(failed.is_empty(), "not at parity: {failed:?}");
}
