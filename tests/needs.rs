//! Needs you, live (src/needs.rs, BRIEF-stint-2026-11-02-approve-into-fold): the overlay a document gets from `needs.json`, the
//! folds and forms a live item becomes (two passkey verdicts, BRIEF addendum 1), and `fold serve`'s `needs.json` with every refusal, held in
//! goldens/needs/overlay.json and goldens/needs/serve.json. `FOLD_BLESS=1 cargo test --test needs` writes them.

use std::fs;
use std::path::PathBuf;
use std::sync::{Arc, Mutex};

use fold_core::act::{Actor, Done, Push};
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
fn a_live_item_is_a_fold_with_two_passkey_verdicts() {
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
            { "role": "form", "name": "Decline with passkey", "cls": "act", "method": "get", "action": "", "intent": ID, "call": "decline",
              "children": [{ "role": "button", "name": "Decline with passkey", "cls": "act" }] },
            { "role": "status", "cls": "result", "id": "did" }
        ])
    );
    let html: Value = serde_json::from_str(&fold_core::wasm::html_text(&tree.to_string())).unwrap();
    let html = html["html"].as_str().unwrap();
    assert!(html.contains(&format!("<form class=\"act\" aria-label=\"Approve with passkey\" method=\"get\" action=\"\" data-intent=\"{ID}\" data-call=\"approve\">")), "{html}");
    assert!(
        html.contains(&format!(
            "aria-label=\"Decline with passkey\" method=\"get\" action=\"\" data-intent=\"{ID}\" data-call=\"decline\">"
        )),
        "{html}"
    );
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

/// No other act names an intent, and each verdict names only its own endpoints: endpoints from anywhere but the intent's own id,
/// an id chitin would not make, another cap, or one verdict's endpoints under the other's call (an approve dressed as a decline).
#[test]
fn only_an_intents_own_endpoints_name_it() {
    let of = |a: Value| fold_core::needs::intent_of(&a).map(|(i, c)| format!("{i} {c}"));
    let pk = |call: &str, c: &str, a: &str| json!({ "cap": "webauthn", "call": call, "args": { "challenge": c, "assertion": a } });
    let (ac, aa) = (format!("/approve/v1/challenge/{ID}"), format!("/approve/v1/assertion/{ID}"));
    let dc = format!("/approve/v1/decline/{ID}");
    assert_eq!(of(pk("approve", &ac, &aa)), Some(format!("{ID} approve")));
    assert_eq!(of(pk("decline", &dc, &dc)), Some(format!("{ID} decline")));
    assert_eq!(
        of(json!({ "cap": "approve", "call": "sign", "args": { "challenge": ac, "assertion": aa } })),
        Some(format!("{ID} approve"))
    );
    assert_eq!(of(pk("decline", &ac, &aa)), None, "an approve's endpoints under decline");
    assert_eq!(of(pk("approve", &dc, &dc)), None, "a decline's endpoints under approve");
    assert_eq!(of(pk("approve", &format!("https://evil.example{ac}"), &aa)), None);
    assert_eq!(of(pk("approve", &ac, "/approve/v1/assertion/bbbbbbbbbbbb")), None);
    assert_eq!(of(pk("approve", "/approve/v1/challenge/../x", "/approve/v1/assertion/../x")), None);
    assert_eq!(of(pk("approve", "/approve/v1/challenge/a", "/approve/v1/assertion/a")), None);
    assert_eq!(of(pk("ack", &ac, &aa)), None);
    assert_eq!(of(json!({ "cap": "chitin", "call": "decline", "args": { "challenge": dc, "assertion": dc } })), None);
    assert_eq!(of(json!({ "cap": "intent", "call": "decline", "args": { "intent": ID } })), None, "no unsigned decline");
}

/// The actor the serve goldens give the server: chitin's pending list (or a failure), each read recorded.
#[derive(Clone, Default)]
struct Stub {
    calls: Arc<Mutex<Vec<String>>>,
    down: bool,
    /// a notify chitin refuses once for this intent (its record is tried again at the next look)
    refuse_once: Arc<Mutex<Option<String>>>,
    /// the pending list, when not the fixture's
    list: Arc<Mutex<Option<Value>>>,
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
        Ok(self.list.lock().unwrap().clone().unwrap_or_else(pending))
    }
    fn notify(&self, seat: &str, title: &str, url: &str, dedupe: &str) -> Result<(), String> {
        self.calls.lock().unwrap().push(format!("notify --as {seat} --dedupe {dedupe} --url {url} {title}"));
        let mut r = self.refuse_once.lock().unwrap();
        if r.as_deref().is_some_and(|id| dedupe.ends_with(id)) {
            *r = None;
            return Err("timeout: router did not answer".into());
        }
        Ok(())
    }
    fn push(&self, op: &Push) -> Result<String, String> {
        self.calls.lock().unwrap().push(format!("push {op:?}"));
        match op {
            Push::Key => Ok("BPub_key-0".into()),
            Push::Subscribe(s) if s.contains("evil") => Err("refused: the endpoint is not an https URL on a known push service".into()),
            Push::Subscribe(_) => Ok("subscribed".into()),
            Push::Unsubscribe(_) => Ok("removed".into()),
        }
    }
}

fn served(stub: &Stub, needs_as: Option<&str>) -> Site {
    served_as(stub, needs_as, None)
}

fn served_as(stub: &Stub, needs_as: Option<&str>, notify_as: Option<&str>) -> Site {
    // one directory per site: tests run at once, and two may serve the same flags
    static N: std::sync::atomic::AtomicUsize = std::sync::atomic::AtomicUsize::new(0);
    let n = N.fetch_add(1, std::sync::atomic::Ordering::SeqCst);
    let root = std::env::temp_dir().join(format!(
        "fold-needs-{}-{}-{}-{n}",
        needs_as.unwrap_or("none"),
        notify_as.unwrap_or("none"),
        std::process::id()
    ));
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
        notify_as: notify_as.map(str::to_string),
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
fn needs_json_refuses_all_but_the_one_login_and_nothing_declines_here() {
    let stub = Stub::default();
    let down = Stub { down: true, ..Default::default() };
    let (s, off, broken) = (served(&stub, Some("bobby")), served(&stub, None), served(&down, Some("bobby")));
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
        (
            "no unsigned decline: a POST to decline/<id> is a read-only server's 405, and chitin is not run",
            &s,
            from_me("POST", &format!("/decline/{ID}"), "{}", &[]),
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
    // chitin was only ever read, and only for what passed every check
    assert_eq!(*stub.calls.lock().unwrap(), ["pending --as bobby", "pending --as bobby"]);
    assert_eq!(*down.calls.lock().unwrap(), ["pending --as bobby"]);
    bless_or_hold("serve.json", &Value::Array(got));
    for site in [s, off, broken] {
        let _ = fs::remove_dir_all(&site.root);
    }
}

/// Notifications (BRIEF addendum 2): what is pushed for a new item is its one line and its fold, never its text; the overlay gives
/// Needs you its two device acts only when the server sends them; and the forms are the script's (data-push).
#[test]
fn a_notice_is_the_items_line_and_fold_and_the_acts_appear_only_when_sent() {
    let w = &fold_core::needs::waiting(&pending(), "bobby")[0];
    assert_eq!(
        fold_core::needs::notice(w, "phobos.tail556f5b.ts.net", "/fold"),
        Some((
            "To personal-assistant".into(),
            "https://phobos.tail556f5b.ts.net/fold/needs/approvals/needs-sign-0123456789ab/".into(),
            "needs-you:sign-0123456789ab".into()
        ))
    );
    // the url is the fold the viewer folds the item to
    let root = overlay(json!({ "root": page(), "needs": { "as": "bobby", "waiting": pending() }, "now": NOW_MS }));
    let folds: Value = serde_json::from_str(&fold_core::wasm::fold_text(&json!({ "root": root, "opts": {} }).to_string())).unwrap();
    assert!(folds.as_array().unwrap().iter().any(|p| p[0] == "needs~approvals~needs-sign-0123456789ab"));
    assert_eq!(fold_core::needs::notice(w, "evil host/x", "/fold"), None);
    assert_eq!(fold_core::needs::notice(w, "h", "/../x"), None);
    assert_eq!(fold_core::needs::notice(w, "h", "/basis/").unwrap().1, "https://h/basis/needs/approvals/needs-sign-0123456789ab/");
    assert_eq!(fold_core::needs::notice(&json!({ "id": "../x", "to": "a" }), "h", "/fold"), None);
    assert!(!fold_core::needs::notice(w, "h", "/fold").unwrap().0.contains("Route the plan"), "never the text");
    // the acts: only with push: true
    let acts = |push: bool| {
        let r = overlay(json!({ "root": page(), "needs": { "as": "bobby", "waiting": pending(), "push": push }, "now": NOW_MS }));
        r["sections"][0].get("acts").cloned()
    };
    assert_eq!(acts(false), None);
    assert_eq!(
        acts(true),
        Some(json!([
            { "cap": "push", "call": "subscribe", "label": "Notify me on this device", "args": {} },
            { "cap": "push", "call": "unsubscribe", "label": "Stop notifying this device", "args": {} }
        ]))
    );
    let root = overlay(json!({ "root": page(), "needs": { "as": "bobby", "waiting": pending(), "push": true }, "now": NOW_MS }));
    let folds: Value = serde_json::from_str(&fold_core::wasm::fold_text(&json!({ "root": root, "opts": {} }).to_string())).unwrap();
    let node = folds.as_array().unwrap().iter().find(|p| p[0] == "needs").unwrap()[1].clone();
    let tree: Value = serde_json::from_str(&fold_core::wasm::ax_text(
        &json!({ "node": node, "path": ["needs"], "mode": "folds", "href": "viewer" }).to_string(),
    ))
    .unwrap();
    let html: Value = serde_json::from_str(&fold_core::wasm::html_text(&tree.to_string())).unwrap();
    let html = html["html"].as_str().unwrap();
    assert!(html.contains(r#"aria-label="Notify me on this device" method="get" action="" data-push="subscribe">"#), "{html}");
    assert!(html.contains(r#"aria-label="Stop notifying this device" method="get" action="" data-push="unsubscribe">"#), "{html}");
}

#[test]
fn push_routes_hand_a_subscription_to_chitin_for_the_one_login_only() {
    let stub = Stub::default();
    let (on, off) = (served_as(&stub, Some("bobby"), Some("fold-notify")), served(&stub, Some("bobby")));
    let sub = r#"{"endpoint":"https://web.push.apple.com/QGx-abc","keys":{"p256dh":"BNcR","auth":"tBHI"},"expirationTime":null,"extra":"dropped"}"#;
    let cases: Vec<(&str, &Site, Request)> = vec![
        ("needs.json says notifications are sent here", &on, from_me("GET", "/needs.json", "", &[])),
        ("the key a device subscribes with", &on, from_me("GET", "/push/key", "", &[])),
        ("not sent here: no route", &off, from_me("GET", "/push/key", "", &[])),
        ("subscribe: handed to chitin cut to endpoint and keys", &on, from_me("POST", "/push/subscribe", sub, &[])),
        ("subscribe: another login", &on, from_me("POST", "/push/subscribe", sub, &[("Tailscale-User-Login", Some("eve@example.com"))])),
        ("subscribe: cross-site", &on, from_me("POST", "/push/subscribe", sub, &[("Sec-Fetch-Site", Some("cross-site"))])),
        (
            "subscribe: a form: JSON only",
            &on,
            from_me("POST", "/push/subscribe", "endpoint=x", &[("Content-Type", Some("application/x-www-form-urlencoded"))]),
        ),
        (
            "subscribe: not https",
            &on,
            from_me("POST", "/push/subscribe", r#"{"endpoint":"http://web.push.apple.com/x","keys":{"p256dh":"a","auth":"b"}}"#, &[]),
        ),
        ("subscribe: no keys", &on, from_me("POST", "/push/subscribe", r#"{"endpoint":"https://web.push.apple.com/x"}"#, &[])),
        (
            "subscribe: chitin refuses it, and says why",
            &on,
            from_me("POST", "/push/subscribe", r#"{"endpoint":"https://evil.example/x","keys":{"p256dh":"a","auth":"b"}}"#, &[]),
        ),
        ("subscribe: got, not posted", &on, from_me("GET", "/push/subscribe", "", &[])),
        ("unsubscribe", &on, from_me("POST", "/push/unsubscribe", r#"{"endpoint":"https://web.push.apple.com/QGx-abc"}"#, &[])),
        ("no such verb", &on, from_me("POST", "/push/list", "{}", &[])),
    ];
    let got: Vec<Value> = cases
        .iter()
        .map(|(what, site, r)| {
            let b = stub.calls.lock().unwrap().len();
            let (res, log) = answer(site, r, NOW);
            let body = String::from_utf8_lossy(&res.body).to_string();
            json!({ "what": what, "target": r.target, "method": r.method, "body_limit": limit(&r.method, &r.target), "status": res.status,
                "body": serde_json::from_str::<Value>(&body).unwrap_or(Value::String(body)), "actor": stub.calls.lock().unwrap()[b..].to_vec(), "log": log })
        })
        .collect();
    let subs: Vec<String> = stub.calls.lock().unwrap().iter().filter(|c| c.starts_with("push Subscribe")).cloned().collect();
    assert_eq!(subs.len(), 2, "chitin saw the good one and the one it refused: {subs:?}");
    assert!(!subs[0].contains("extra") && !subs[0].contains("expirationTime"), "{}", subs[0]);
    bless_or_hold("push.json", &Value::Array(got));
    for site in [on, off] {
        let _ = fs::remove_dir_all(&site.root);
    }
}

/// The sender: one notify per new waiting intent, none again, none for one that left; one chitin did not take is tried again at
/// each look, and its failure is logged when it starts or changes, not every look.
#[test]
fn the_watcher_notifies_once_per_new_item() {
    let stub = Stub::default();
    *stub.refuse_once.lock().unwrap() = Some("0a0a0a0a0a0a".into());
    let s = served_as(&stub, Some("bobby"), Some("fold-notify"));
    let mut seen = fold_core::serve::Watched::default();
    let notes = |stub: &Stub| stub.calls.lock().unwrap().iter().filter(|c| c.starts_with("notify")).cloned().collect::<Vec<_>>();
    let first = fold_core::serve::watch(&s, &mut seen, NOW);
    assert_eq!(
        first.iter().map(|l| l["result"].as_str().unwrap().to_string()).collect::<Vec<_>>(),
        ["ok", "chitin:timeout: router did not answer"]
    );
    assert_eq!(
        notes(&stub),
        [
            "notify --as fold-notify --dedupe needs-you:sign-0123456789ab --url https://phobos.example.ts.net/fold/needs/approvals/needs-sign-0123456789ab/ To personal-assistant",
            "notify --as fold-notify --dedupe needs-you:sign-0a0a0a0a0a0a --url https://phobos.example.ts.net/fold/needs/approvals/needs-sign-0a0a0a0a0a0a/ To personal-cake-goal",
        ]
    );
    // the refused one again, and nothing for the one already sent
    let again = fold_core::serve::watch(&s, &mut seen, NOW + 30);
    assert_eq!(again.len(), 1);
    assert_eq!(again[0]["notify"], "0a0a0a0a0a0a");
    assert!(fold_core::serve::watch(&s, &mut seen, NOW + 60).is_empty());
    // one leaves (approved): nothing is sent; were it queued again, it is new again
    *stub.list.lock().unwrap() = Some(json!([pending()[0]]));
    assert!(fold_core::serve::watch(&s, &mut seen, NOW + 90).is_empty());
    assert_eq!(seen.seen.len(), 1);
    assert!(first.iter().chain(again.iter()).all(|l| !l.to_string().contains("Route the plan")), "a log line never holds the text");
    // without --notify-as nothing is watched
    let quiet = served(&stub, Some("bobby"));
    assert!(fold_core::serve::watch(&quiet, &mut Default::default(), NOW).is_empty());
    // chitin down: one line when it starts, none while it stays down, one when it changes
    let down = Stub { down: true, ..Default::default() };
    let d = served_as(&down, Some("bobby"), Some("fold-notify"));
    let mut dw = fold_core::serve::Watched::default();
    assert_eq!(fold_core::serve::watch(&d, &mut dw, NOW).len(), 1);
    assert!(fold_core::serve::watch(&d, &mut dw, NOW + 30).is_empty());
    let _ = fs::remove_dir_all(&d.root);
    for site in [s, quiet] {
        let _ = fs::remove_dir_all(&site.root);
    }
}
