//! Needs you, live (src/needs.rs, BRIEF-stint-2026-11-02-approve-into-fold): the overlay a document gets from `needs.json`, the
//! folds and forms a live item becomes, and `fold serve`'s two routes (`needs.json`, `decline/<id>`) with every refusal, held in
//! goldens/needs/overlay.json and goldens/needs/serve.json. `FOLD_BLESS=1 cargo test --test needs` writes them.

use std::fs;
use std::path::PathBuf;
use std::sync::{Arc, Mutex};

use fold_core::act::{Actor, Done};
use fold_core::serve::{answer, limit, Acting, Request, Site};
use serde_json::{json, Value};

const NOW_MS: i64 = 1_793_361_600_000;
const NOW: i64 = NOW_MS / 1000;
const ME: &str = "bobby@example.com";
const HOST: &str = "phobos.example.ts.net";
const ID: &str = "0123456789ab";

/// What `chitin pending --as bobby --json` might list: one that waits, and one of each kind that must not be shown.
fn pending() -> Value {
    let it = |id: &str, as_: &str, to: Value, text: Value, declined: bool, expired: bool| {
        json!({ "id": id, "as": as_, "to": to, "text": text, "chars": 15, "text_hash": "deadbeefcafe", "age_s": 125,
            "queued_by": "claude-break-glass", "created_at": "2026-10-30T11:57:55.000Z", "declined": declined, "expired": expired,
            "brief": { "ask": "route it", "why": "x", "effect": "y", "undo": { "can": true } } })
    };
    json!([
        it(ID, "bobby", json!("personal-assistant"), json!("Route the plan to the hubs."), false, false),
        it("bbbbbbbbbbbb", "bobby", json!("seat-b"), json!("declined"), true, false),
        it("cccccccccccc", "bobby", json!("seat-c"), json!("expired"), false, true),
        it("dddddddddddd", "work-assistant", json!("seat-d"), json!("another title's"), false, false),
        it("BAD", "bobby", json!("seat-e"), json!("an id chitin would never make"), false, false),
        it("eeeeeeeeeeee", "bobby", json!(""), json!("no recipient"), false, false),
        it("ffffffffffff", "bobby", json!("seat-f"), json!(5), false, false),
        { "id": "0a0a0a0a0a0a", "as": "bobby", "to": "personal-cake-goal", "text": "Land <b>it</b> \"now\".", "declined": false, "expired": false },
    ])
}

/// A page as home's fleet_map.ts writes one: Needs you with an ask and a sign item the producer read minutes ago, and another section.
fn page() -> Value {
    json!({ "title": "Everything", "meta": { "timeZone": "UTC" }, "sections": [
        { "id": "needs", "title": "Needs you", "purpose": "What the agents need from you, each with what to do.",
          "empty": "Nothing needs you right now.", "groups": [{ "title": "", "items": [
            { "id": "needs/ask-0", "action": "Answer cake's question.", "link": { "href": "https://example.test/pr/1", "label": "Open PR #1" } },
            { "id": "needs/sign-999999999999", "name": "To a seat", "nameKind": "agent", "text": "Signed already, minutes ago.", "quoted": true,
              "act": { "cap": "webauthn", "call": "approve", "label": "Approve with passkey",
                "args": { "challenge": "/approve/v1/challenge/999999999999", "assertion": "/approve/v1/assertion/999999999999" } },
              "link": { "href": "https://example.test/approve/#i-999999999999", "label": "Approve on the approval page" } }
          ] }] },
        { "id": "agents", "title": "Agent fleet", "items": [{ "id": "seat-x", "name": "seat-x", "state": "idle" }] }
    ] })
}

fn bless_or_hold(name: &str, got: &Value) {
    let path = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join(format!("goldens/needs/{name}"));
    let text = serde_json::to_string_pretty(got).unwrap() + "\n";
    if std::env::var("FOLD_BLESS").is_ok() {
        fs::create_dir_all(path.parent().unwrap()).unwrap();
        fs::write(&path, &text).unwrap();
    }
    assert!(text == fs::read_to_string(&path).unwrap_or_default(), "goldens/needs/{name} is stale (FOLD_BLESS=1 cargo test --test needs)");
}

fn overlay(case: Value) -> Value {
    serde_json::from_str(&fold_core::wasm::needs_text(&case.to_string())).unwrap()
}

#[test]
fn the_overlay_puts_the_live_list_first_and_nothing_else() {
    let answered = json!({ "as": "bobby", "waiting": fold_core::needs::waiting(&pending(), "bobby") });
    let cases = [
        (
            "read live: the waiting intents, first, as Approvals; the producer's sign item gone, its ask where it was",
            json!({ "root": page(), "needs": answered, "now": NOW_MS }),
        ),
        (
            "the read failed: Approvals says why and offers nothing to sign",
            json!({ "root": page(), "error": "403 not who this server acts for", "now": NOW_MS }),
        ),
        ("no read and no reason: the same, never the producer's list", json!({ "root": page(), "now": NOW_MS })),
        (
            "nothing waits: no Approvals, and the producer's sign item still gone",
            json!({ "root": page(), "needs": { "as": "bobby", "waiting": [] }, "now": NOW_MS }),
        ),
        (
            "an answer is filtered again for its own title: chitin's raw list, not only what the server cut",
            json!({ "root": page(), "needs": { "as": "bobby", "waiting": pending() }, "now": NOW_MS }),
        ),
        (
            "an answer that names no title shows nothing to sign",
            json!({ "root": page(), "needs": { "waiting": pending() }, "now": NOW_MS }),
        ),
        (
            "a document with no Needs you is as it was",
            json!({ "root": { "title": "A deck", "sections": [{ "id": "one", "title": "One" }] }, "needs": answered, "now": NOW_MS }),
        ),
    ];
    let got: Vec<Value> =
        cases.into_iter().map(|(what, case)| json!({ "what": what, "input": case, "root": overlay(case.clone()) })).collect();
    // what a reader is handed widens nothing: only the one well-formed intent queued as bobby, and the one with no age
    let ids: Vec<&str> =
        got[0]["root"]["sections"][0]["groups"][0]["items"].as_array().unwrap().iter().map(|i| i["id"].as_str().unwrap()).collect();
    assert_eq!(ids, ["needs/sign-0123456789ab", "needs/sign-0a0a0a0a0a0a"]);
    assert_eq!(got[4]["root"], got[0]["root"]);
    bless_or_hold("overlay.json", &Value::Array(got));
}

/// The live document folded: Needs you › Approvals › the item, at the producer's id, with its two forms, each naming the intent.
#[test]
fn a_live_item_is_a_fold_with_its_passkey_and_its_decline() {
    let root = overlay(json!({ "root": page(), "needs": { "as": "bobby", "waiting": pending() }, "now": NOW_MS }));
    let folds: Value =
        serde_json::from_str(&fold_core::wasm::fold_text(&json!({ "root": root, "opts": { "localTimeZone": "UTC" } }).to_string()))
            .unwrap();
    let ids: Vec<&str> = folds.as_array().unwrap().iter().map(|p| p[0].as_str().unwrap()).collect();
    assert!(ids.contains(&"needs~approvals~needs-sign-0123456789ab"), "{ids:?}");
    assert!(ids.contains(&"needs~needs-ask-0"), "the ask stays where the producer put it: {ids:?}");
    assert!(!ids.iter().any(|i| i.contains("999999999999")), "the producer's sign item is gone: {ids:?}");
    let node = folds.as_array().unwrap().iter().find(|p| p[0] == "needs~approvals~needs-sign-0123456789ab").unwrap()[1].clone();
    let path = ["needs", "approvals", "needs-sign-0123456789ab"];
    let tree: Value = serde_json::from_str(&fold_core::wasm::ax_text(
        &json!({ "node": node, "path": path, "mode": "folds", "href": "viewer", "localTimeZone": "UTC" }).to_string(),
    ))
    .unwrap();
    let row = tree["children"].as_array().unwrap().iter().find(|c| c["role"] == "list").unwrap()["children"]
        .as_array()
        .unwrap()
        .iter()
        .find(|r| r["cls"] == "prop act")
        .unwrap()
        .clone();
    assert_eq!(
        row["children"],
        json!([
            { "role": "form", "name": "Approve with passkey", "cls": "act", "method": "get", "action": "", "intent": ID, "call": "approve",
              "children": [{ "role": "button", "name": "Approve with passkey", "cls": "act" }] },
            { "role": "form", "name": "Decline", "cls": "act", "method": "post", "action": format!("decline/{ID}"), "intent": ID, "call": "decline",
              "children": [{ "role": "button", "name": "Decline", "cls": "act" }] },
            { "role": "status", "cls": "result", "id": "did" }
        ])
    );
    let html: Value = serde_json::from_str(&fold_core::wasm::html_text(&tree.to_string())).unwrap();
    let html = html["html"].as_str().unwrap();
    assert!(html.contains(&format!("<form class=\"act\" aria-label=\"Approve with passkey\" method=\"get\" action=\"\" data-intent=\"{ID}\" data-call=\"approve\">")), "{html}");
    assert!(html.contains(&format!("action=\"decline/{ID}\" data-intent=\"{ID}\" data-call=\"decline\">")), "{html}");
}

/// `basis --needs` reads the same folds at the same paths: `Needs you/Approvals/<item>` by title or by id, and its text names the
/// two acts (goldens/needs/basis.txt).
#[test]
fn basis_reads_the_live_document_at_the_viewers_paths() {
    let root = overlay(json!({ "root": page(), "needs": { "as": "bobby", "waiting": pending() }, "now": NOW_MS }));
    let d = fold_core::basis::Doc::new(&root, Some("UTC".into())).unwrap();
    let by_id = d.resolve("needs/approvals/needs-sign-0123456789ab").unwrap();
    assert_eq!(d.resolve("Needs you/Approvals/To personal-assistant").unwrap(), by_id);
    let text = [d.resolve("needs").unwrap(), d.resolve("needs/approvals").unwrap(), by_id]
        .iter()
        .map(|p| d.text(p))
        .collect::<Vec<_>>()
        .join("\n");
    let path = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("goldens/needs/basis.txt");
    if std::env::var("FOLD_BLESS").is_ok() {
        fs::write(&path, &text).unwrap();
    }
    assert!(
        text == fs::read_to_string(&path).unwrap_or_default(),
        "goldens/needs/basis.txt is stale (FOLD_BLESS=1 cargo test --test needs)"
    );
}

/// No other act names an intent: endpoints from anywhere but the intent's own id, an id chitin would not make, or a decline on
/// another cap stay as they were (a passkey with no sign page is a plain act form, which `fold serve` never runs).
#[test]
fn only_an_intents_own_endpoints_name_it() {
    let of = |a: Value| fold_core::needs::intent_of(&a).map(|(i, c)| format!("{i} {c}"));
    let pk = |c: &str, a: &str| json!({ "cap": "webauthn", "call": "approve", "args": { "challenge": c, "assertion": a } });
    assert_eq!(of(pk(&format!("/approve/v1/challenge/{ID}"), &format!("/approve/v1/assertion/{ID}"))), Some(format!("{ID} approve")));
    assert_eq!(of(pk(&format!("https://evil.example/approve/v1/challenge/{ID}"), &format!("/approve/v1/assertion/{ID}"))), None);
    assert_eq!(of(pk(&format!("/approve/v1/challenge/{ID}"), "/approve/v1/assertion/bbbbbbbbbbbb")), None);
    assert_eq!(of(pk("/approve/v1/challenge/../x", "/approve/v1/assertion/../x")), None);
    assert_eq!(of(pk("/approve/v1/challenge/a", "/approve/v1/assertion/a")), None);
    assert_eq!(of(json!({ "cap": "intent", "call": "decline", "args": { "intent": ID } })), Some(format!("{ID} decline")));
    assert_eq!(of(json!({ "cap": "chitin", "call": "decline", "args": { "intent": ID } })), None);
    assert_eq!(of(json!({ "cap": "intent", "call": "approve", "args": { "intent": ID } })), None);
}

/// The actor the serve goldens give the server: chitin's pending list (or a failure), and a record of each decline.
#[derive(Clone, Default)]
struct Stub {
    calls: Arc<Mutex<Vec<String>>>,
    down: bool,
}

impl Actor for Stub {
    fn ack(&self, _: &str, _: &str, _: &str, _: Option<&str>, _: &str) -> Result<Done, String> {
        Err("no ack here".into())
    }
    fn say(&self, _: &str, _: &str, _: Option<&str>, _: &str) -> Result<Done, String> {
        Err("no say here".into())
    }
    fn pending(&self, title: &str) -> Result<Value, String> {
        self.calls.lock().unwrap().push(format!("pending --as {title}"));
        if self.down {
            return Err("chitin: state dir unreadable".into());
        }
        Ok(pending())
    }
    fn decline(&self, id: &str) -> Result<Done, String> {
        self.calls.lock().unwrap().push(format!("decline {id}"));
        Ok(Done::Ok)
    }
}

fn served(stub: &Stub, needs_as: Option<&str>) -> Site {
    let root = std::env::temp_dir().join(format!("fold-needs-{}-{}", needs_as.unwrap_or("none"), std::process::id()));
    let _ = fs::remove_dir_all(&root);
    fs::create_dir_all(root.join("needs")).unwrap();
    fs::write(root.join("needs/index.html"), "<!doctype html><title>Needs you</title><main><h1>Needs you</h1></main>\n").unwrap();
    let acting = Acting {
        login: ME.into(),
        host: HOST.into(),
        key: vec![7; 32],
        actor: Box::new(stub.clone()),
        spent: Default::default(),
        needs_as: needs_as.map(str::to_string),
    };
    Site { root, mount: "/fold".into(), no_store: vec![], acting: Some(acting), reporting: None }
}

/// A request as tailscale serve hands one on from ME's page: loopback, ME's login, through serve, same-origin, JSON.
fn from_me(method: &str, target: &str, body: &str, change: &[(&str, Option<&str>)]) -> Request {
    let mut headers: Vec<(String, String)> = [
        ("Tailscale-User-Login", ME),
        ("X-Forwarded-For", "100.64.0.2"),
        ("X-Forwarded-Proto", "https"),
        ("X-Forwarded-Host", HOST),
        ("Sec-Fetch-Site", "same-origin"),
        ("Content-Type", "application/json"),
    ]
    .iter()
    .map(|(k, v)| (k.to_string(), v.to_string()))
    .collect();
    for (k, v) in change {
        headers.retain(|(n, _)| n != k);
        if let Some(v) = v {
            headers.push((k.to_string(), v.to_string()));
        }
    }
    Request {
        method: method.into(),
        target: target.into(),
        headers,
        body: body.as_bytes().to_vec(),
        peer: Some("127.0.0.1".parse().unwrap()),
    }
}

#[test]
fn needs_json_and_decline_refuse_all_but_the_one_login() {
    let stub = Stub::default();
    let down = Stub { down: true, ..Default::default() };
    let (s, off, broken) = (served(&stub, Some("bobby")), served(&stub, None), served(&down, Some("bobby")));
    let dec = format!("/decline/{ID}");
    let mut far = from_me("GET", "/needs.json", "", &[]);
    far.peer = Some("10.0.0.9".parse().unwrap());
    let cases: Vec<(&str, &Site, Request)> = vec![
        ("needs.json for ME: the title's waiting intents, cut to what is shown, never stored", &s, from_me("GET", "/needs.json", "", &[])),
        ("a HEAD: the same", &s, from_me("HEAD", "/needs.json", "", &[])),
        ("without --needs-as: not read live here", &off, from_me("GET", "/needs.json", "", &[])),
        ("not from loopback", &s, far),
        ("another login", &s, from_me("GET", "/needs.json", "", &[("Tailscale-User-Login", Some("eve@example.com"))])),
        ("not through tailscale serve", &s, from_me("GET", "/needs.json", "", &[("X-Forwarded-Host", Some("evil.example"))])),
        ("cross-site (a page elsewhere reading it)", &s, from_me("GET", "/needs.json", "", &[("Sec-Fetch-Site", Some("cross-site"))])),
        ("posted to", &s, from_me("POST", "/needs.json", "{}", &[])),
        ("chitin can't list them: 502, and why", &broken, from_me("GET", "/needs.json", "", &[])),
        ("decline: got, not posted", &s, from_me("GET", &dec, "", &[])),
        ("decline: without --needs-as", &off, from_me("POST", &dec, "{}", &[])),
        ("decline: another login", &s, from_me("POST", &dec, "{}", &[("Tailscale-User-Login", Some("eve@example.com"))])),
        ("decline: cross-site", &s, from_me("POST", &dec, "{}", &[("Sec-Fetch-Site", Some("cross-site"))])),
        (
            "decline: no Sec-Fetch-Site, another Origin",
            &s,
            from_me("POST", &dec, "{}", &[("Sec-Fetch-Site", None), ("Origin", Some("https://evil.example"))]),
        ),
        (
            "decline: a form (a cross-site form could send one): JSON only",
            &s,
            from_me("POST", &dec, "x=1", &[("Content-Type", Some("application/x-www-form-urlencoded"))]),
        ),
        ("decline: not an intent id", &s, from_me("POST", "/decline/..%2Fetc", "{}", &[])),
        ("decline: an id chitin would not make", &s, from_me("POST", "/decline/0123456789AB", "{}", &[])),
        ("decline: declined already", &s, from_me("POST", "/decline/bbbbbbbbbbbb", "{}", &[])),
        ("decline: expired", &s, from_me("POST", "/decline/cccccccccccc", "{}", &[])),
        ("decline: another title's intent", &s, from_me("POST", "/decline/dddddddddddd", "{}", &[])),
        ("decline: no such intent", &s, from_me("POST", "/decline/abcabcabcabc", "{}", &[])),
        ("decline: chitin can't list them, so nothing is declined", &broken, from_me("POST", &dec, "{}", &[])),
        ("decline: one that waits for the title, declined", &s, from_me("POST", &dec, "{}", &[])),
        (
            "decline: no Sec-Fetch-Site, its own Origin (an older browser)",
            &s,
            from_me("POST", &dec, "{}", &[("Sec-Fetch-Site", None), ("Origin", Some("https://phobos.example.ts.net"))]),
        ),
    ];
    let got: Vec<Value> = cases
        .iter()
        .map(|(what, site, r)| {
            let (b1, b2) = (stub.calls.lock().unwrap().len(), down.calls.lock().unwrap().len());
            let (res, log) = answer(site, r, NOW);
            let mut calls: Vec<String> = stub.calls.lock().unwrap()[b1..].to_vec();
            calls.extend(down.calls.lock().unwrap()[b2..].iter().cloned());
            let body = String::from_utf8_lossy(&res.body).to_string();
            json!({
                "what": what,
                "request": { "method": r.method, "target": r.target, "peer": r.peer.map(|p| p.to_string()), "headers": r.headers, "body": String::from_utf8_lossy(&r.body) },
                "body_limit": limit(&r.method, &r.target),
                "status": res.status,
                "headers": res.headers,
                "body": serde_json::from_str::<Value>(&body).unwrap_or(Value::String(body)),
                "actor": calls,
                "log": log,
            })
        })
        .collect();
    let declined: Vec<String> = stub.calls.lock().unwrap().iter().filter(|c| c.starts_with("decline")).cloned().collect();
    assert_eq!(declined, [format!("decline {ID}"), format!("decline {ID}")], "chitin declined only what passed every check");
    assert!(down.calls.lock().unwrap().iter().all(|c| !c.starts_with("decline")));
    bless_or_hold("serve.json", &Value::Array(got));
    for site in [s, off, broken] {
        let _ = fs::remove_dir_all(&site.root);
    }
}
