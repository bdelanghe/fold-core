//! Byte parity with fold-ax.js axOf: every [id, mode, href, tree] in goldens/ax/<case>.ax.json, built here from the fold in
//! goldens/walk/<case>.input.json (wasm::ax_text, the module's own entry), serializes exactly as its tree.
//!
//! The goldens were home's fold-ax.js's (goldens/gen.ts); where the core leaves it on purpose (src/ax.rs `ax_of`: acts as forms,
//! a way back from a missing, failed or empty fold), they are the core's, and `FOLD_BLESS=1 cargo test --test ax` rewrites them.

use std::fs;
use std::path::Path;

use serde_json::{json, Value};

#[test]
fn trees_at_byte_parity() {
    let dir = Path::new(env!("CARGO_MANIFEST_DIR")).join("goldens");
    let mut cases: Vec<_> = fs::read_dir(dir.join("ax")).unwrap().filter_map(|e| e.ok()).map(|e| e.path()).collect();
    cases.sort();
    assert!(!cases.is_empty(), "no ax goldens");
    let bless = std::env::var("FOLD_BLESS").is_ok();
    let (mut trees, mut failed) = (0, vec![]);
    for p in &cases {
        let mut lines = vec![];
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
            lines.push(format!("[{},{},{},{got}]", json!(id), json!(mode), json!(href)));
        }
        if bless {
            fs::write(p, format!("[\n{}\n]\n", lines.join(",\n"))).unwrap();
        }
    }
    if bless {
        return;
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

/// An act is a form (BRIEF-basis-forms-push-signing-2026-10-31 §1): named by its fold, posting its cap, call and each arg as a
/// hidden field to `act/<path>/<index>`, with a native submit button; the passkey GETs its sign page instead, fields and all left
/// to that page, the fragment that names the intent kept.
#[test]
fn an_act_is_a_form() {
    let tree = |node: Value, path: &[&str]| -> Value {
        let case = json!({ "node": node, "path": path, "mode": "folds", "href": "viewer", "localTimeZone": "UTC" });
        serde_json::from_str(&fold_core::wasm::ax_text(&case.to_string())).unwrap()
    };
    let form = |t: &Value| t.pointer("/children/2/children/0/children/0").unwrap().clone();
    let ack = json!({ "title": "Reply from a seat", "trail": ["Everything", "Needs you"], "kids": [],
        "act": { "cap": "chitin.ack", "call": "ack", "label": "Done", "args": { "msgId": "m1", "state": "done", "n": 2 } } });
    assert_eq!(
        form(&tree(ack, &["needs", "a/b"])),
        json!({ "role": "form", "name": "Reply from a seat", "cls": "act", "method": "post", "action": "act/needs/a%2Fb/0",
            "fields": [["cap", "chitin.ack"], ["call", "ack"], ["args.msgId", "m1"], ["args.state", "done"], ["args.n", "2"]],
            "children": [{ "role": "button", "name": "Done", "cls": "act" }] })
    );
    let sign = json!({ "title": "To a seat", "trail": ["Everything"], "href": "/approve/#i-0123456789ab",
        "act": { "cap": "approve", "call": "sign", "label": "Sign", "args": { "intent": "0123456789ab" } } });
    assert_eq!(
        form(&tree(sign, &["needs"])),
        json!({ "role": "form", "name": "To a seat", "cls": "act", "method": "get", "action": "/approve/#i-0123456789ab",
            "children": [{ "role": "button", "name": "Sign", "cls": "act" }] })
    );
    let html = fold_core::html::html(&json!({ "role": "form", "name": "N", "method": "post", "action": "act/0",
        "fields": [["cap", "a\"b"]], "children": [{ "role": "button", "name": "Go" }] })).unwrap();
    assert_eq!(html, "<form aria-label=\"N\" method=\"post\" action=\"act/0\"><input type=\"hidden\" name=\"cap\" value=\"a&quot;b\"><button type=\"submit\">Go</button></form>");
}
