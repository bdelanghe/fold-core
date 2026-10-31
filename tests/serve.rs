//! fold serve (src/serve.rs, brief items 23, 24 and 27): every resource class's headers, and every write refused, held in
//! goldens/serve/headers.json. `FOLD_BLESS=1 cargo test --test serve` writes it. One more test runs the real loop on a loopback
//! port, for what only it adds (`serve;dur`, a body refused before it is read). The act route (S2: allie-s2-fold-act-write §6) is
//! goldens/serve/acts.json: every refusal and every success, with a fixed key and clock so its tokens hold still, each with the
//! calls the (recording) actor saw and the log line it left.

use std::fs;
use std::io::{Read, Write};
use std::net::{TcpListener, TcpStream};
use std::path::PathBuf;
use std::sync::{Arc, Mutex};
use std::time::{Duration, SystemTime};

use fold_core::act::{self, Actor, Done};
use fold_core::serve::{answer, respond, run, Acting, Request, Site};
use serde_json::{json, Value};

/// A served directory with one file of each class, every mtime 2026-10-30T12:00:00Z so Last-Modified and asof hold still.
fn site(name: &str) -> Site {
    let root = std::env::temp_dir().join(format!("fold-serve-{name}-{}", std::process::id()));
    let _ = fs::remove_dir_all(&root);
    let files = [
        ("index.html", "<!doctype html><title>Fleet</title><main><h1>Fleet</h1></main>\n"),
        ("a b/index.html", "<!doctype html><title>Desk — Fleet</title><main><h1>Desk</h1></main>\n"),
        ("fleet/page.json", "{\"title\":\"Fleet\"}\n"),
        ("view.js", "export {};\n"),
        ("sw.js", "self.addEventListener(\"fetch\", () => {});\n"),
        ("fold@sha256:5e1f.css", "main{}\n"),
        ("sign/pending.json", "[]\n"),
    ];
    let t = SystemTime::UNIX_EPOCH + Duration::from_secs(1_793_361_600);
    for (p, body) in files {
        let f = root.join(p);
        fs::create_dir_all(f.parent().unwrap()).unwrap();
        fs::write(&f, body).unwrap();
        fs::File::options().write(true).open(&f).unwrap().set_modified(t).unwrap();
    }
    Site { root, mount: "/fold".into(), no_store: vec!["sign/".into()], acting: None }
}

fn req(method: &str, target: &str, headers: &[(&str, &str)], body: &str) -> Request {
    Request {
        method: method.into(),
        target: target.into(),
        headers: headers.iter().map(|(k, v)| (k.to_string(), v.to_string())).collect(),
        body: body.as_bytes().to_vec(),
        peer: None,
    }
}

#[test]
fn every_resource_class_has_its_headers() {
    let s = site("goldens");
    let me = ("Tailscale-User-Login", "bobby@example.com");
    let root_etag = respond(&s, &req("GET", "/", &[], "")).header("ETag").unwrap().to_string();
    let cases: Vec<(&str, Request)> = vec![
        (
            "a fold page at its bare path: always revalidated, its file digest the ETag",
            req("GET", "/", &[me, ("Referer", "https://phobos/fold/a%20b/")], ""),
        ),
        ("a fold page under it, its segment percent-decoded as the file names are", req("GET", "/a%20b/", &[me], "")),
        ("a directory without its slash moves to it, under the mount", req("GET", "/a%20b", &[], "")),
        ("revalidated and unchanged: 304, no body", req("GET", "/", &[me, ("If-None-Match", &root_etag)], "")),
        ("a prefetch: served the same (and, never pinging, never a view)", req("GET", "/a%20b/", &[me, ("Sec-Purpose", "prefetch")], "")),
        (
            "a data document: a minute fresh, four more stale while revalidating, a day stale on error",
            req("GET", "/fleet/page.json", &[], ""),
        ),
        ("a HEAD: the GET's headers, its length too", req("HEAD", "/fleet/page.json", &[], "")),
        ("a viewer file not addressed by digest: always revalidated", req("GET", "/view.js", &[], "")),
        ("the service worker: always revalidated", req("GET", "/sw.js", &[], "")),
        ("addressed by digest: immutable", req("GET", "/fold@sha256:5e1f.css", &[], "")),
        ("live state (--no-store sign/): never stored, no validator", req("GET", "/sign/pending.json", &[("If-None-Match", "*")], "")),
        ("a ping sent here, not to the sink: read-only", req("POST", "/_v", &[me, ("Content-Type", "text/ping")], "PING")),
        (
            "an act's form with no script to take it: nothing done, a page that says so and the way back",
            req("POST", "/act/needs/a%2Fb/0", &[me, ("Content-Type", "application/x-www-form-urlencoded")], "cap=chitin.ack&call=ack"),
        ),
        ("a POST under act/ that names no act: read-only", req("POST", "/act/", &[], "")),
        ("no PUT", req("PUT", "/index.html", &[], "x")),
        ("a .. segment, even encoded", req("GET", "/%2e%2e/etc/passwd", &[], "")),
        ("nothing there", req("GET", "/missing/", &[], "")),
        ("no other method", req("DELETE", "/", &[], "")),
    ];
    let got: Vec<Value> = cases
        .iter()
        .map(|(what, r)| {
            let res = respond(&s, r);
            json!({
                "what": what,
                "request": { "method": r.method, "target": r.target, "headers": r.headers },
                "status": res.status,
                "headers": res.headers,
                "body_bytes": res.body.len(),
            })
        })
        .collect();
    let path = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("goldens/serve/headers.json");
    let text = serde_json::to_string_pretty(&got).unwrap() + "\n";
    if std::env::var("FOLD_BLESS").is_ok() {
        fs::create_dir_all(path.parent().unwrap()).unwrap();
        fs::write(&path, &text).unwrap();
    }
    assert!(
        text == fs::read_to_string(&path).unwrap_or_default(),
        "goldens/serve/headers.json is stale (FOLD_BLESS=1 cargo test --test serve)"
    );
    // The page an act's form gets, byte for byte.
    let page = respond(&s, &req("POST", "/act/needs/a%2Fb/0", &[], "cap=chitin.ack")).body;
    let path = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("goldens/serve/act.html");
    if std::env::var("FOLD_BLESS").is_ok() {
        fs::write(&path, &page).unwrap();
    }
    assert!(page == fs::read(&path).unwrap_or_default(), "goldens/serve/act.html is stale (FOLD_BLESS=1 cargo test --test serve)");
    let _ = fs::remove_dir_all(&s.root);
}

/// One raw request to the real loop, and its whole answer.
fn over_the_wire(port: u16, raw: &[u8]) -> String {
    let mut c = TcpStream::connect(("127.0.0.1", port)).unwrap();
    c.write_all(raw).unwrap();
    let mut out = String::new();
    c.read_to_string(&mut out).unwrap();
    out
}

#[test]
fn the_loop_times_each_answer_and_takes_no_body() {
    let s = site("loop");
    let root = s.root.clone();
    let l = TcpListener::bind("127.0.0.1:0").unwrap();
    let port = l.local_addr().unwrap().port();
    std::thread::spawn(move || run(l, s, None));
    let out = over_the_wire(port, b"GET /a%20b/ HTTP/1.1\r\nHost: x\r\n\r\n");
    assert!(out.starts_with("HTTP/1.1 200 OK\r\n"), "{out}");
    assert!(out.lines().any(|l| l.starts_with("Server-Timing: digest;desc=\"sha-256:") && l.contains(", serve;dur=")), "{out}");
    assert!(out.ends_with("<h1>Desk</h1></main>\n"), "{out}");
    let out = over_the_wire(port, b"POST /_v HTTP/1.1\r\nContent-Type: text/ping\r\nContent-Length: 4\r\n\r\nPING");
    assert!(out.starts_with("HTTP/1.1 413 "), "{out}");
    // an act's form is the one body taken, read only to be dropped, and answered by a page; one too big is refused unread
    let out = over_the_wire(
        port,
        b"POST /act/needs/0 HTTP/1.1\r\nContent-Type: application/x-www-form-urlencoded\r\nContent-Length: 8\r\n\r\ncap=x&c=",
    );
    assert!(out.starts_with("HTTP/1.1 405 ") && out.contains("<h1 tabindex=\"-1\">Acting from here isn't on yet</h1>"), "{out}");
    let out = over_the_wire(port, b"POST /act/needs/0 HTTP/1.1\r\nContent-Length: 9000\r\n\r\n");
    assert!(out.starts_with("HTTP/1.1 413 "), "{out}");
    // it wrote nothing: the directory is the one site() made
    let mut names: Vec<String> = fs::read_dir(&root).unwrap().map(|e| e.unwrap().file_name().into_string().unwrap()).collect();
    names.sort();
    assert_eq!(names, ["a b", "fleet", "fold@sha256:5e1f.css", "index.html", "sign", "sw.js", "view.js"]);
    let _ = fs::remove_dir_all(&root);
}

/// The actor the act goldens give the server: it records each call, says `written` the first time a message reaches a state and
/// noop after, and cannot reach the seat behind `m-down`.
#[derive(Clone, Default)]
struct Stub(Arc<Mutex<Vec<String>>>);

impl Actor for Stub {
    fn ack(&self, msg_id: &str, state: &str, to: &str, why: Option<&str>, act: &str) -> Result<Done, String> {
        let call = format!("{msg_id} {state} to={to} ref=fold:{act}{}", why.map(|w| format!(" why={w}")).unwrap_or_default());
        let mut seen = self.0.lock().unwrap();
        let before = seen.contains(&call);
        seen.push(call);
        match (msg_id, before) {
            ("m-down", _) => Err("timeout: seat-b did not answer".into()),
            (_, true) => Ok(Done::Already),
            _ => Ok(Done::Ok),
        }
    }
}

const NOW: i64 = 1_793_361_600;
const KEY: [u8; 32] = [7; 32];
const ME: &str = "bobby@example.com";
const HOST: &str = "phobos.example.ts.net";

/// A site as `site()` writes it (page.rs), acting for ME at HOST through `stub`: a Needs-you item with its three acts, a passkey
/// act, chitin's `say` (a pair this server never runs) and the legacy `{cap: "ack", call: "status"}` shape, an act naming no
/// message, and one whose seat is down. Each act is `{cap, call}` as Allie's cap host names them (personal-allie-lead 4f93c11f).
fn acting_site(name: &str, stub: &Stub) -> Site {
    let root = std::env::temp_dir().join(format!("fold-serve-{name}-{}", std::process::id()));
    let _ = fs::remove_dir_all(&root);
    let a = |call: &str, msg: &str| json!({ "cap": "chitin", "call": call, "label": call, "args": { "msg_id": msg, "from": "seat-a" } });
    let passkey = json!({ "cap": "approve", "call": "sign", "label": "Sign", "args": { "msg_id": "m-1", "from": "seat-a" } });
    let legacy = json!({ "cap": "ack", "call": "status", "label": "Ack", "args": { "msg_id": "m-1", "from": "seat-a" } });
    let kid = |id: &str| json!({ "id": id, "title": id });
    let nodes = json!([
        ["root", { "title": "Everything", "trail": [], "kids": [kid("needs")] }],
        ["needs", { "title": "Needs you", "trail": ["Everything"], "kids": [kid("m 1"), kid("bare"), kid("down")] }],
        ["needs~m 1", { "title": "From seat-a", "trail": ["Everything", "Needs you"], "kids": [], "text": ["Ship the fix?"],
            "acts": [a("ack", "m-1"), a("defer", "m-1"), a("decline", "m-1"), passkey, a("say", "m-1"), legacy] }],
        ["needs~bare", { "title": "No message", "trail": ["Everything", "Needs you"], "kids": [],
            "acts": [{ "cap": "chitin", "call": "ack", "label": "Ack", "args": {} }] }],
        ["needs~down", { "title": "Seat down", "trail": ["Everything", "Needs you"], "kids": [], "acts": [a("ack", "m-down")] }]
    ]);
    let pages: Value =
        serde_json::from_str(&fold_core::wasm::site_text(&json!({ "nodes": nodes, "source": "fleet" }).to_string())).unwrap();
    for p in pages.as_array().unwrap() {
        let f = root.join(p["file"].as_str().unwrap());
        fs::create_dir_all(f.parent().unwrap()).unwrap();
        fs::write(&f, p["html"].as_str().or(p["json"].as_str()).unwrap()).unwrap();
    }
    let acting = Acting { login: ME.into(), host: HOST.into(), key: KEY.to_vec(), actor: Box::new(stub.clone()) };
    Site { root, mount: "/fold".into(), no_store: vec![], acting: Some(acting) }
}

/// A request as tailscale serve hands one on from ME's own page: loopback, ME's login, through serve, same-origin.
fn from_me(method: &str, target: &str, body: &str, change: &[(&str, Option<&str>)]) -> Request {
    let mut h: Vec<(String, String)> = [
        ("Tailscale-User-Login", ME),
        ("X-Forwarded-For", "100.64.0.2"),
        ("X-Forwarded-Proto", "https"),
        ("X-Forwarded-Host", HOST),
        ("Sec-Fetch-Site", "same-origin"),
        ("Content-Type", "application/x-www-form-urlencoded"),
    ]
    .iter()
    .map(|(k, v)| (k.to_string(), v.to_string()))
    .collect();
    for (k, v) in change {
        h.retain(|(n, _)| n != k);
        if let Some(v) = v {
            h.push((k.to_string(), v.to_string()));
        }
    }
    let mut r = req(method, target, &[], body);
    r.headers = h;
    r.peer = Some("127.0.0.1".parse().unwrap());
    r
}

/// The act's fixity as the site's act map holds it.
fn fixity(s: &Site, id: &str) -> String {
    let map: Value = serde_json::from_slice(&fs::read(s.root.join("_acts.json")).unwrap()).unwrap();
    map[id]["fixity"].as_str().unwrap().to_string()
}

/// What a page says in its status line, and the tokens its forms carry.
fn said(html: &str) -> (Option<String>, Vec<String>) {
    let status =
        html.split("role=\"status\">").nth(1).and_then(|r| r.split("</span>").next()).filter(|t| !t.is_empty()).map(str::to_string);
    let tokens = html.split("name=\"token\" value=\"").skip(1).map(|r| r.split('"').next().unwrap().to_string()).collect();
    (status, tokens)
}

#[test]
fn every_act_refusal_and_success() {
    let stub = Stub::default();
    let s = acting_site("acts", &stub);
    let page = "/needs/m%201/";
    let ack = "/act/needs/m%201/ack";
    let good = act::mint(&KEY, "needs/m%201/ack", &fixity(&s, "needs/m%201/ack"), ME, NOW);
    let body = |t: &str| format!("token={t}&cap=chitin&call=ack&args.msg_id=m-1&args.from=seat-a");
    let gb = body(&good);
    let mut far = from_me("POST", ack, &gb, &[]);
    far.peer = Some("10.0.0.9".parse().unwrap());
    let other_key = act::mint(&[9; 32], "needs/m%201/ack", &fixity(&s, "needs/m%201/ack"), ME, NOW);
    let expired = act::mint(&KEY, "needs/m%201/ack", &fixity(&s, "needs/m%201/ack"), ME, NOW - act::TTL - 1);
    let changed = act::mint(&KEY, "needs/m%201/ack", &"0".repeat(64), ME, NOW);
    let gone = act::mint(&KEY, "needs/gone/ack", &"0".repeat(64), ME, NOW);
    let cases: Vec<(&str, Request)> = vec![
        ("an act's page for ME: each form's token filled, private, revalidated", from_me("GET", page, "", &[])),
        (
            "an act's page for another login: no token filled",
            from_me("GET", page, "", &[("Tailscale-User-Login", Some("eve@example.com"))]),
        ),
        ("a page with no act: as it is on disk", from_me("GET", "/needs/", "", &[])),
        ("the act map is never served", from_me("GET", "/_acts.json", "", &[])),
        ("an act is posted, never got", from_me("GET", ack, "", &[])),
        ("nor HEAD", from_me("HEAD", ack, "", &[])),
        ("nor PUT", from_me("PUT", ack, &gb, &[])),
        ("not from loopback", far),
        ("no login", from_me("POST", ack, &gb, &[("Tailscale-User-Login", None)])),
        ("another login", from_me("POST", ack, &gb, &[("Tailscale-User-Login", Some("eve@example.com"))])),
        ("a Q-encoded login", from_me("POST", ack, &gb, &[("Tailscale-User-Login", Some("=?utf-8?q?bobby@example.com?="))])),
        ("no X-Forwarded-Proto", from_me("POST", ack, &gb, &[("X-Forwarded-Proto", None)])),
        ("another X-Forwarded-Host", from_me("POST", ack, &gb, &[("X-Forwarded-Host", Some("evil.example"))])),
        ("Sec-Fetch-Site cross-site", from_me("POST", ack, &gb, &[("Sec-Fetch-Site", Some("cross-site"))])),
        ("Sec-Fetch-Site same-site", from_me("POST", ack, &gb, &[("Sec-Fetch-Site", Some("same-site"))])),
        ("Sec-Fetch-Site none", from_me("POST", ack, &gb, &[("Sec-Fetch-Site", Some("none"))])),
        ("no Sec-Fetch-Site, Origin null", from_me("POST", ack, &gb, &[("Sec-Fetch-Site", None), ("Origin", Some("null"))])),
        (
            "no Sec-Fetch-Site, another origin",
            from_me("POST", ack, &gb, &[("Sec-Fetch-Site", None), ("Origin", Some("https://evil.example"))]),
        ),
        ("no Sec-Fetch-Site and no Origin", from_me("POST", ack, &gb, &[("Sec-Fetch-Site", None)])),
        ("same-origin, but an Origin that is not", from_me("POST", ack, &gb, &[("Origin", Some("https://evil.example"))])),
        ("not a form", from_me("POST", ack, &gb, &[("Content-Type", Some("application/json"))])),
        ("no body", from_me("POST", ack, "", &[])),
        ("no token", from_me("POST", ack, "cap=chitin&call=ack", &[])),
        ("a token of the wrong shape", from_me("POST", ack, &body("v2.1.2.3"), &[])),
        ("a token of the wrong length", from_me("POST", ack, &body(&good[..good.len() - 1]), &[])),
        ("the form's call is not the fold's: a tripwire", from_me("POST", ack, &gb.replace("call=ack", "call=decline"), &[])),
        ("nor is the form's cap", from_me("POST", ack, &gb.replace("cap=chitin", "cap=approve"), &[])),
        ("an act id that is not one", from_me("POST", "/act/needs/%zz/ack", &gb, &[])),
        ("no such act", from_me("POST", "/act/needs/m%201/nudge", &gb, &[])),
        ("the passkey (approve/sign) is never run here", from_me("POST", "/act/needs/m%201/sign", &gb, &[])),
        ("nor chitin/say: only chitin's ack, defer and decline", from_me("POST", "/act/needs/m%201/say", &gb, &[])),
        ("nor the legacy {cap: ack, call: status}", from_me("POST", "/act/needs/m%201/status", &gb, &[])),
        ("a token minted by another key", from_me("POST", ack, &body(&other_key), &[])),
        ("an expired token: the page again, a fresh token, nothing done", from_me("POST", ack, &body(&expired), &[])),
        ("a form no server filled: the same", from_me("POST", ack, &body(""), &[])),
        ("the item changed since the page was served: the page again, nothing done", from_me("POST", ack, &body(&changed), &[])),
        (
            "the item has gone: the page of a fold that is not there, with its way back",
            from_me("POST", "/act/needs/gone/ack", &body(&gone), &[]),
        ),
        (
            "an act that names no message",
            from_me("POST", "/act/needs/bare/ack", &body(&act::mint(&KEY, "needs/bare/ack", &fixity(&s, "needs/bare/ack"), ME, NOW)), &[]),
        ),
        (
            "chitin does not take it",
            from_me("POST", "/act/needs/down/ack", &body(&act::mint(&KEY, "needs/down/ack", &fixity(&s, "needs/down/ack"), ME, NOW)), &[]),
        ),
        ("ack: done, and back to the fold", from_me("POST", ack, &gb, &[])),
        ("the fold then says so", from_me("GET", "/needs/m%201/?did=ack&as=ok", "", &[])),
        ("ack again, the same token: done before, and harmless", from_me("POST", ack, &gb, &[])),
        ("a ?did= that is not in the table says nothing", from_me("GET", "/needs/m%201/?did=%3Cb%3E&as=ok", "", &[])),
        (
            "defer: with chitin's reason",
            from_me(
                "POST",
                "/act/needs/m%201/defer",
                &body(&act::mint(&KEY, "needs/m%201/defer", &fixity(&s, "needs/m%201/defer"), ME, NOW)).replace("call=ack", "call=defer"),
                &[],
            ),
        ),
        (
            "decline, Origin and no Sec-Fetch-Site (an older browser)",
            from_me(
                "POST",
                "/act/needs/m%201/decline",
                &body(&act::mint(&KEY, "needs/m%201/decline", &fixity(&s, "needs/m%201/decline"), ME, NOW))
                    .replace("call=ack", "call=decline"),
                &[("Sec-Fetch-Site", None), ("Origin", Some("https://phobos.example.ts.net"))],
            ),
        ),
    ];
    let got: Vec<Value> = cases
        .iter()
        .map(|(what, r)| {
            let before = stub.0.lock().unwrap().len();
            let (res, log) = answer(&s, r, NOW);
            let calls: Vec<String> = stub.0.lock().unwrap()[before..].to_vec();
            let html = res.header("Content-Type").is_some_and(|c| c.starts_with("text/html"));
            let text = String::from_utf8_lossy(&res.body).to_string();
            let body = if html {
                let (status, tokens) = said(&text);
                json!({ "html_bytes": res.body.len(), "says": status, "tokens": tokens, "h1": text.split("<h1").nth(1).and_then(|h| h.split('>').nth(1)).and_then(|h| h.split('<').next()) })
            } else {
                json!(text)
            };
            json!({
                "what": what,
                "request": { "method": r.method, "target": r.target, "peer": r.peer.map(|p| p.to_string()), "headers": r.headers, "body": String::from_utf8_lossy(&r.body) },
                "status": res.status,
                "headers": res.headers.iter().filter(|(k, _)| k != "Server-Timing" && k != "Content-Digest" && k != "Last-Modified").collect::<Vec<_>>(),
                "body": body,
                "actor": calls,
                "log": log,
            })
        })
        .collect();
    // The actor saw exactly the three that passed every check, and the replay; nothing for any refusal.
    assert_eq!(
        *stub.0.lock().unwrap(),
        [
            "m-down acting to=seat-a ref=fold:needs/down/ack",
            "m-1 acting to=seat-a ref=fold:needs/m%201/ack",
            "m-1 acting to=seat-a ref=fold:needs/m%201/ack",
            "m-1 deferred to=seat-a ref=fold:needs/m%201/defer why=Deferred from Basis",
            "m-1 declined to=seat-a ref=fold:needs/m%201/decline why=Declined from Basis"
        ]
    );
    // A token the GET filled is one the POST takes.
    let (_, tokens) = said(&String::from_utf8_lossy(&answer(&s, &from_me("GET", page, "", &[]), NOW).0.body));
    assert_eq!(
        tokens.iter().filter(|t| !t.is_empty()).count(),
        3,
        "ack, defer and decline get one; the passkey and archive forms, which this server never runs, do not"
    );
    let path = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("goldens/serve/acts.json");
    let text = serde_json::to_string_pretty(&got).unwrap() + "\n";
    if std::env::var("FOLD_BLESS").is_ok() {
        fs::write(&path, &text).unwrap();
    }
    assert!(
        text == fs::read_to_string(&path).unwrap_or_default(),
        "goldens/serve/acts.json is stale (FOLD_BLESS=1 cargo test --test serve)"
    );
    let _ = fs::remove_dir_all(&s.root);
}

/// The site tools/act-e2e.sh serves (goldens/serve/act-site): one Needs-you item with its acts, as `site()` writes it.
#[test]
fn the_e2e_site_is_what_site_writes() {
    let a = |call: &str| json!({ "cap": "chitin", "call": call, "label": call, "args": { "msg_id": "m-1", "from": "seat-a" } });
    let nodes = json!([
        ["root", { "title": "Everything", "trail": [], "kids": [{ "id": "m1", "title": "From seat-a" }] }],
        ["m1", { "title": "From seat-a", "trail": ["Everything"], "kids": [], "text": ["Ship the fix?"], "acts": [a("ack"), a("defer"), a("decline")] }]
    ]);
    let pages: Value =
        serde_json::from_str(&fold_core::wasm::site_text(&json!({ "nodes": nodes, "source": "fleet" }).to_string())).unwrap();
    let dir = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("goldens/serve/act-site");
    for p in pages.as_array().unwrap() {
        let f = dir.join(p["file"].as_str().unwrap());
        let want = p["html"].as_str().or(p["json"].as_str()).unwrap();
        if std::env::var("FOLD_BLESS").is_ok() {
            fs::create_dir_all(f.parent().unwrap()).unwrap();
            fs::write(&f, want).unwrap();
        }
        assert!(fs::read_to_string(&f).ok().as_deref() == Some(want), "{} is stale (FOLD_BLESS=1 cargo test --test serve)", f.display());
    }
}
