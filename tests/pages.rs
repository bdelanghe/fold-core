//! Every fold as a whole page at its real path, native: each goldens/walk/<case>.input.json's folds put through src/page.rs
//! (wasm::site_text, the module's own entry, with the case's name as the document's source) are goldens/pages/<case>.pages.json,
//! byte for byte. js/parity.test.ts holds the WASM build to the same file. `FOLD_BLESS=1 cargo test --test pages` writes them.

use std::fs;
use std::path::Path;

use serde_json::{json, Value};

fn site(input: &Value) -> Value {
    serde_json::from_str(&fold_core::wasm::site_text(&input.to_string())).unwrap()
}

#[test]
fn every_golden_document_writes_its_golden_pages() {
    let dir = Path::new(env!("CARGO_MANIFEST_DIR")).join("goldens");
    let mut cases: Vec<_> = fs::read_dir(dir.join("walk"))
        .unwrap()
        .filter_map(|e| e.ok())
        .map(|e| e.path())
        .filter(|p| p.to_string_lossy().ends_with(".input.json"))
        .collect();
    cases.sort();
    assert!(!cases.is_empty(), "no walk inputs");
    let bless = std::env::var("FOLD_BLESS").is_ok();
    let (mut n, mut failed) = (0, vec![]);
    for p in &cases {
        let name = p.file_name().unwrap().to_string_lossy().trim_end_matches(".input.json").to_string();
        let input: Value = serde_json::from_str(&fs::read_to_string(p).unwrap()).unwrap();
        let out = site(&json!({ "nodes": input["nodes"], "source": name, "localTimeZone": input["opts"]["localTimeZone"] }));
        let pages = out.as_array().unwrap_or_else(|| panic!("{name}: {out}"));
        let f = dir.join(format!("pages/{name}.pages.json"));
        // One page per line, as goldens/html holds its folds.
        let text = format!("[\n{}\n]\n", pages.iter().map(|r| serde_json::to_string(r).unwrap()).collect::<Vec<_>>().join(",\n"));
        if bless {
            fs::create_dir_all(f.parent().unwrap()).unwrap();
            fs::write(&f, &text).unwrap();
            continue;
        }
        n += pages.len();
        if fs::read_to_string(&f).ok().as_deref() != Some(text.as_str()) {
            failed.push(name);
        }
    }
    println!("{n} pages");
    assert!(failed.is_empty(), "goldens/pages differs for {failed:?} (FOLD_BLESS=1 rewrites them)");
}

fn one(nodes: Value) -> Vec<Value> {
    site(&json!({ "nodes": nodes, "source": "doc", "localTimeZone": "UTC" })).as_array().unwrap().clone()
}

#[test]
fn a_fold_is_a_page_at_its_real_path() {
    let pages = one(json!([
        ["root", { "title": "Fleet", "trail": [], "kids": [{ "id": "a b", "title": "Desk", "hint": "4 open" }] }],
        ["a b", { "title": "Desk", "trail": ["Fleet"], "kids": [], "present": "slides" }]
    ]));
    let at: Vec<(&str, &str, &str)> =
        pages.iter().map(|p| (p["href"].as_str().unwrap(), p["file"].as_str().unwrap(), p["mode"].as_str().unwrap())).collect();
    assert_eq!(
        at,
        [
            ("/fold/", "index.html", "folds"),
            ("/fold/a%20b/", "a b/index.html", "folds"),
            ("/fold/a%20b/@slides/", "a b/@slides/index.html", "slides")
        ]
    );
    let root = pages[0]["html"].as_str().unwrap();
    assert!(root.contains("<title>Fleet</title>"), "{root}");
    // The heading: the fold's id (the root's, its document's), where a link in lands; no autofocus.
    assert!(root.contains(r#"<h1 id="doc" tabindex="-1">Fleet</h1>"#), "{root}");
    // The row: its path as its id, its IRI, a same-tab link to its page's heading (no ping), and its hint named by
    // its id.
    assert!(root.contains(r##"<li id="a%20b" resource="#a%20b" typeof="schema:WebPageElement" class="obj" data-i="0"><span property="schema:about" resource="https://ipso.bounded.tools/doc/a%20b"></span>"##), "{root}");
    assert!(
        root.contains(
            r#"<a href="/fold/a%20b/#a%20b" aria-describedby="a%20b:hint">Desk</a><span class="hint" id="a%20b:hint">4 open</span>"#
        ),
        "{root}"
    );
    assert!(!root.contains("target="), "{root}");
    let desk = pages[1]["html"].as_str().unwrap();
    assert!(desk.contains("<title>Desk — Fleet</title>"), "{desk}");
    // Back up the path, at the row it was left by; a move like any other.
    assert!(desk.contains(r#"<a href="/fold/#a%20b">Fleet</a>"#), "{desk}");
    assert!(desk.contains(r#"<h1 id="a%20b" tabindex="-1">Desk</h1>"#), "{desk}");
}

#[test]
fn a_step_no_file_can_hold_is_refused() {
    let pages = one(json!([["..", { "title": "Up", "trail": ["Fleet"], "kids": [] }]]));
    assert_eq!(pages[0]["error"], r#"page: no file can hold the step "..""#);
    let e = |i: Value| site(&i)["error"].as_str().unwrap().to_string();
    assert_eq!(e(json!({ "nodes": [] })), "no source: a document is named");
    assert_eq!(e(json!({ "nodes": [], "source": "d", "base": "fold" })), "base: fold is not a path from / to a /");
}
