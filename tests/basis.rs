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

// basis say, ack and ask (slice 2).

use fold_core::basis::{add_ask, approve_shaped, ask_row, Ask, ASK_MAX};
use std::process::Command;

fn ask() -> Ask {
    Ask {
        seat: "seat-a".into(),
        topic: "fold".into(),
        item: "Look at #32".into(),
        why: "slice 1".into(),
        link: Some(("https://example.test/32".into(), "Open".into())),
        ..Ask::default()
    }
}

#[test]
fn an_ask_is_a_typed_row_with_a_button_and_an_id_from_what_it_asks() {
    let row = ask_row(&ask(), "2026-11-01T00:00:00Z").unwrap();
    assert_eq!(
        row,
        json!({ "id": "fold-656960", "item": "Look at #32", "why": "slice 1", "seat": "seat-a", "at": "2026-11-01T00:00:00Z",
                "topic": "fold", "link": { "href": "https://example.test/32", "label": "Open" } })
    );
    // The id is the topic's and the item's alone: not the seat's, the time's or the place in the file.
    let other = ask_row(&Ask { seat: "seat-b".into(), ..ask() }, "2026-11-02T00:00:00Z").unwrap();
    assert_eq!(other["id"], row["id"]);
    let signed = ask_row(&Ask { intent: Some("0123456789ab".into()), link: None, ..ask() }, "t").unwrap();
    assert_eq!(signed["intent"], "0123456789ab");
    let refused = |a: Ask| ask_row(&a, "t").unwrap_err();
    assert!(refused(Ask { link: None, ..ask() }).contains("no button"));
    assert!(refused(Ask { topic: "Fold".into(), ..ask() }).contains("--topic"));
    assert!(refused(Ask { topic: "-x".into(), ..ask() }).contains("--topic"));
    assert!(refused(Ask { seat: " ".into(), ..ask() }).contains("no seat"));
    assert!(refused(Ask { item: "a\nb".into(), ..ask() }).contains("one line"));
    assert!(refused(Ask { why: "x".repeat(ASK_MAX + 1), ..ask() }).contains("over 300"));
    assert!(ask_row(&Ask { why: "é".repeat(ASK_MAX), ..ask() }, "t").is_ok(), "the limit counts chars, not bytes");
    assert!(refused(Ask { intent: Some("0123456789AB".into()), ..ask() }).contains("12 lowercase hex"));
    assert!(refused(Ask { link: Some(("javascript:x".into(), "Open".into())), ..ask() }).contains("--link"));
}

#[test]
fn an_ask_is_added_to_the_file_in_its_shape_once() {
    let row = ask_row(&ask(), "t").unwrap();
    let list: Value = serde_json::from_str(&add_ask(None, row.clone()).unwrap()).unwrap();
    assert_eq!(list, json!([row]));
    let wrapped = add_ask(Some(r#"{"asks": [{"item": "old"}], "note": "kept"}"#), row.clone()).unwrap();
    let w: Value = serde_json::from_str(&wrapped).unwrap();
    assert_eq!(w["note"], "kept");
    assert_eq!(w["asks"][1], row);
    assert!(add_ask(Some(&list.to_string()), row.clone()).unwrap_err().contains("already asked"));
    assert!(add_ask(Some("{}"), row.clone()).is_err());
    assert!(add_ask(Some("not json"), row).is_err());
}

#[test]
fn a_say_that_asks_for_a_signature_is_not_a_say() {
    for t in ["Approve: fold-core builds basis", "  sign: this", "Please approve the PR"] {
        assert!(approve_shaped(t), "{t}");
    }
    for t in ["approved, thanks", "Signed off on #32", "what to approve next?"] {
        assert!(!approve_shaped(t), "{t}");
    }
}

/// The binary against a stub chitin that prints its arguments: what basis runs, as which seat, and what it refuses.
#[test]
fn basis_says_acks_and_asks_through_chitin_and_the_asks_file() {
    let dir = std::env::temp_dir().join(format!("basis-test-{}", std::process::id()));
    fs::create_dir_all(&dir).unwrap();
    let stub = dir.join("chitin");
    fs::write(&stub, "#!/bin/sh\necho \"$@\"\n").unwrap();
    fs::set_permissions(&stub, std::os::unix::fs::PermissionsExt::from_mode(0o755)).unwrap();
    let run = |args: &[&str]| {
        let o = Command::new(env!("CARGO_BIN_EXE_basis"))
            .args(args)
            .env("BASIS_CHITIN", &stub)
            .env("BASIS_SEAT", "seat-a")
            .env("FM_ASKS", dir.join("asks.json"))
            .output()
            .unwrap();
        (o.status.code(), String::from_utf8_lossy(&o.stdout).into_owned(), String::from_utf8_lossy(&o.stderr).into_owned())
    };
    assert_eq!(run(&["say", "bobby", "slice 1 is up", "--ref", "needs"]).1, "say --as seat-a --meta ref=basis:needs bobby slice 1 is up\n");
    assert_eq!(run(&["say", "bobby", "hi", "--in-reply-to", "m1", "--as", "seat-b"]).1, "say --as seat-b --meta ref=basis --in-reply-to m1 bobby hi\n");
    let (code, out, err) = run(&["say", "bobby", "Approve: ship it"]);
    assert_eq!((code, out.as_str()), (Some(1), ""), "{err}");
    assert_eq!(run(&["ack", "m1", "done", "--why", "built"]).1, "ack m1 done --as seat-a --why built\n");
    assert_eq!(run(&["ack", "m1", "maybe"]).0, Some(2));
    let a = ["ask", "--topic", "fold", "--item", "Look at #32", "--why", "slice 1", "--link", "https://example.test/32"];
    assert_eq!(run(&a), (Some(0), "fold-656960\n".to_string(), String::new()));
    assert_eq!(run(&a).0, Some(1), "the same ask twice");
    let rows: Value = serde_json::from_str(&fs::read_to_string(dir.join("asks.json")).unwrap()).unwrap();
    assert_eq!(rows.as_array().unwrap().len(), 1);
    assert_eq!(rows[0]["seat"], "seat-a");
    fs::remove_dir_all(&dir).unwrap();
}
