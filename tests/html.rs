//! One fold's HTML, native: every [id, mode, href, tree] in goldens/ax/<case>.ax.json put through src/html.rs (wasm::html_text,
//! the module's own entry) is the [id, mode, href, html] (or `{"error"}` where toDom refuses it) in goldens/html/<case>.html.json, byte for byte. js/parity.test.ts holds
//! the WASM build to the same file, so native and WASM write the same bytes. `FOLD_BLESS=1 cargo test --test html` writes them.

use std::fs;
use std::path::Path;

use serde_json::Value;

fn html(tree: &Value) -> Result<String, String> {
    let out: Value = serde_json::from_str(&fold_core::wasm::html_text(&tree.to_string())).unwrap();
    match (out["html"].as_str(), out["error"].as_str()) {
        (Some(h), _) => Ok(h.to_string()),
        (_, Some(e)) => Err(e.to_string()),
        _ => panic!("neither html nor error: {out}"),
    }
}

#[test]
fn every_golden_tree_writes_its_golden_html() {
    let dir = Path::new(env!("CARGO_MANIFEST_DIR")).join("goldens");
    let mut cases: Vec<_> = fs::read_dir(dir.join("ax")).unwrap().filter_map(|e| e.ok()).map(|e| e.path()).collect();
    cases.sort();
    assert!(!cases.is_empty(), "no ax goldens");
    let bless = std::env::var("FOLD_BLESS").is_ok();
    let (mut n, mut failed) = (0, vec![]);
    for p in &cases {
        let name = p.file_name().unwrap().to_string_lossy().trim_end_matches(".ax.json").to_string();
        let rows: Vec<(String, String, String, Value)> = serde_json::from_str(&fs::read_to_string(p).unwrap()).unwrap();
        // A tree toDom refuses (an act with no label, say) is held as its refusal, `{"error": ...}`: never HTML.
        let got: Vec<(String, String, String, Value)> = rows
            .into_iter()
            .map(|(id, mode, href, tree)| {
                let h = html(&tree).map_or_else(|e| serde_json::json!({ "error": e }), Value::from);
                (id, mode, href, h)
            })
            .collect();
        let f = dir.join(format!("html/{name}.html.json"));
        // One row per line, as goldens/ax holds its trees.
        let text = format!("[\n{}\n]\n", got.iter().map(|r| serde_json::to_string(r).unwrap()).collect::<Vec<_>>().join(",\n"));
        if bless {
            fs::create_dir_all(f.parent().unwrap()).unwrap();
            fs::write(&f, &text).unwrap();
            continue;
        }
        n += got.len();
        if fs::read_to_string(&f).ok().as_deref() != Some(text.as_str()) {
            failed.push(name);
        }
    }
    println!("{n} folds");
    assert!(failed.is_empty(), "goldens/html differs for {failed:?} (FOLD_BLESS=1 rewrites them)");
}

#[test]
fn it_refuses_what_to_dom_refuses() {
    let e = |s: &str| html(&serde_json::from_str(s).unwrap()).unwrap_err();
    assert_eq!(e(r#"{"role":"marquee"}"#), "fold-ax: no element for role \"marquee\"");
    assert_eq!(e(r##"{"role":"link","name":"  ","href":"#/"}"##), "fold-ax: a link needs a name");
    assert_eq!(
        e(r#"{"role":"main","children":[{"role":"heading","name":"A"},{"role":"text","name":"loose"}]}"#),
        "fold-ax: a fold is its heading and one list, not heading, text"
    );
    assert_eq!(e(r#"{"role":"list","name":"L","children":[{"role":"text","name":"x"}]}"#), "fold-ax: a list holds rows, not text");
    assert_eq!(e(r#"{"role":"link","name":"L"}"#), "fold-ax: a link needs an href");
}

#[test]
fn it_escapes_text_and_attributes() {
    let t = r##"{"role":"link","name":"a < b & \"c\"","href":"#/x?a=1&b=\"2\"","description":"<i>"}"##;
    assert_eq!(
        html(&serde_json::from_str(t).unwrap()).unwrap(),
        r##"<a href="#/x?a=1&amp;b=&quot;2&quot;" aria-describedby="d1">a &lt; b &amp; "c"</a><span class="hint" id="d1">&lt;i&gt;</span>"##
    );
}
