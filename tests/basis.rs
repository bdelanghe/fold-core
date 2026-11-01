//! `basis read`, native: every fold of each goldens/walk/<case>.input.json as text (src/basis.rs) is goldens/basis/<case>.txt,
//! byte for byte, and every row's `→ <path>` is a fold `basis read` reaches and the row anchor its page writes (page.rs).
//! `FOLD_BLESS=1 cargo test --test basis` writes them.

use std::fs;
use std::path::Path;

use fold_core::basis::{path_text, Doc};
use serde_json::{json, Value};

fn cases() -> Vec<(String, Value)> {
    let dir = Path::new(env!("CARGO_MANIFEST_DIR")).join("goldens/walk");
    let mut ps: Vec<_> = fs::read_dir(dir)
        .unwrap()
        .filter_map(|e| e.ok())
        .map(|e| e.path())
        .filter(|p| p.to_string_lossy().ends_with(".input.json"))
        .collect();
    ps.sort();
    ps.iter()
        .map(|p| {
            let name = p.file_name().unwrap().to_string_lossy().trim_end_matches(".input.json").to_string();
            (name, serde_json::from_str(&fs::read_to_string(p).unwrap()).unwrap())
        })
        .collect()
}

fn doc(input: &Value) -> Doc {
    let nodes: Vec<(String, Value)> = serde_json::from_value(input["nodes"].clone()).unwrap();
    Doc::from_nodes(&nodes, input["opts"]["localTimeZone"].as_str().map(str::to_string))
}

#[test]
fn every_golden_document_reads_as_its_golden_text() {
    let dir = Path::new(env!("CARGO_MANIFEST_DIR")).join("goldens/basis");
    let bless = std::env::var("FOLD_BLESS").is_ok();
    let mut failed = vec![];
    for (name, input) in cases() {
        let d = doc(&input);
        let text = d.paths().iter().map(|p| d.text(p)).collect::<Vec<_>>().join("\n");
        let f = dir.join(format!("{name}.txt"));
        if bless {
            fs::create_dir_all(&dir).unwrap();
            fs::write(&f, &text).unwrap();
        } else if fs::read_to_string(&f).ok().as_deref() != Some(text.as_str()) {
            failed.push(name);
        }
    }
    assert!(failed.is_empty(), "goldens/basis differs for {failed:?} (FOLD_BLESS=1 rewrites them)");
}

#[test]
fn every_row_goes_to_a_fold_basis_reads_and_its_page_anchors() {
    for (name, input) in cases() {
        let d = doc(&input);
        let pages = fold_core::wasm::site_text(&json!({ "nodes": input["nodes"], "source": name }).to_string());
        for p in d.paths() {
            for line in d.text(&p).lines() {
                let Some((_, to)) = line.rsplit_once("  → ") else { continue };
                let at = d.resolve(to).unwrap_or_else(|e| panic!("{name} {}: {line}: {e}", path_text(&p)));
                assert_eq!(path_text(&at), to, "{name}: {line}");
                assert!(pages.contains(&format!("id=\\\"{to}\\\"")), "{name}: no page row has the anchor {to}");
            }
        }
    }
}

#[test]
fn a_path_is_ids_or_titles_and_a_miss_says_what_is_there() {
    let d = Doc::new(
        &json!({ "title": "Fleet", "sections": [{ "id": "needs", "title": "Needs you", "items": [{ "id": "a b", "title": "Ship it?" }] }] }),
        Some("UTC".into()),
    )
    .unwrap();
    assert_eq!(d.resolve("").unwrap(), Vec::<String>::new());
    assert_eq!(d.resolve("needs").unwrap(), ["needs"]);
    assert_eq!(d.resolve("needs you").unwrap(), ["needs"]);
    let e = d.resolve("nope").unwrap_err();
    assert!(e.contains("needs (Needs you)"), "{e}");
}
