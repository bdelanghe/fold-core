//! Reporting to Clax (src/report.rs, basis-clax goal close condition 3): the config that never holds a key, the mint request fold
//! serve makes of the Clax instance, `POST report-token` (src/serve.rs) answering only the one login, same-origin, through tailscale
//! serve, and `GET clax/overlay.js` served from this origin out of a (fake) instance on loopback. The minter is a stub: the real one
//! signs with the queue's grant (`clax-ed25519 v2`, not built yet) and refuses until it can.

use std::io::{BufReader, Write};
use std::net::TcpListener;
use std::sync::{Arc, Mutex};

use fold_core::http::{answer_of, call, loopback, parse};
use fold_core::report::{ClaxFiler, ClaxMinter, Comment, Config, Filer, Grant, Minted, Minter, Reporting};
use fold_core::serve::{answer, limit, Request, Site, OVERLAY, REPORT_BODY, REPORT_TOKEN};
use serde_json::Value;

const ME: &str = "bobby@example.com";
const HOST: &str = "phobos.example.ts.net";
const QUEUE: &str = "basis";
const OWNER: &str = "personal-allie-lead";
const PK: &str = "MCowBQYDK2VwAyEAGb9ECWmEzf6FQbrBZ9w7lshQhqowtrbLDFw4rXAxZuE";

// The upstream is port 9 (discard), where no Clax ever listens: a test never reaches Bobby's own instance on 7480.
const CONFIG: &str = r#"{"login":"bobby@example.com","host":"phobos.example.ts.net","clax":"https://clax.example.ts.net/",
  "upstream":"http://127.0.0.1:9","queue":"basis","owner":"personal-allie-lead","source":"fleet-map",
  "grant_cmd":["op","read","op://Fleet/clax basis grant/token"],"grant_key_cmd":["op","read","op://Fleet/clax basis grant/key"]}"#;

/// Unpadded base64url.
fn b64url(b: &[u8]) -> String {
    const A: &[u8] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789-_";
    let mut o = String::new();
    for c in b.chunks(3) {
        let n = (c[0] as u32) << 16 | (*c.get(1).unwrap_or(&0) as u32) << 8 | *c.get(2).unwrap_or(&0) as u32;
        for k in 0..=c.len() {
            o.push(A[(n >> (18 - 6 * k) & 63) as usize] as char);
        }
    }
    o
}

/// A PKCS#8 Ed25519 key for Clax's test seed [0x42; 32], as Clax gives a grant's key: DER, unpadded base64url. Never a real one.
fn test_key() -> String {
    let mut der = vec![0x30, 0x2e, 0x02, 0x01, 0x00, 0x30, 0x05, 0x06, 0x03, 0x2b, 0x65, 0x70, 0x04, 0x22, 0x04, 0x20];
    der.extend([0x42; 32]);
    b64url(&der)
}

/// The grant as the vault would print it, by which field is asked for (never a real one).
fn vault(argv: &[String]) -> Result<Vec<u8>, String> {
    match argv.last().map(String::as_str) {
        Some(p) if p.ends_with("/token") => Ok(b"cxq_TESTONLY\n".to_vec()),
        Some(p) if p.ends_with("/key") => Ok(format!("{}\n", test_key()).into_bytes()),
        _ => Err("op: no such item".into()),
    }
}

#[test]
fn the_config_names_its_grant_commands_and_never_holds_a_key() {
    let c = Config::parse(CONFIG).unwrap();
    assert_eq!((c.clax.as_str(), c.upstream.as_str(), c.queue.as_str()), ("https://clax.example.ts.net", "http://127.0.0.1:9", QUEUE));
    assert_eq!(c.grant_cmd, ["op", "read", "op://Fleet/clax basis grant/token"]);
    let seen = Mutex::new(vec![]);
    let r = Config::parse(CONFIG)
        .unwrap()
        .with_grant(&|argv: &[String]| -> Result<Vec<u8>, String> {
            seen.lock().unwrap().push(argv.join(" "));
            vault(argv)
        })
        .unwrap();
    assert_eq!(seen.lock().unwrap().len(), 2, "each secret is read once, at start");
    assert_eq!((r.login.as_str(), r.owner.as_str()), (ME, OWNER));
    // a grant command that fails, an empty secret, a token that is not a queue's, a secret with a space: refused
    let with = |f: &Grant<'_>| Config::parse(CONFIG).unwrap().with_grant(f).err().unwrap_or_default();
    assert_eq!(with(&|_: &[String]| -> Result<Vec<u8>, String> { Err("op: not signed in".to_string()) }), "op: not signed in");
    assert!(with(&|_: &[String]| -> Result<Vec<u8>, String> { Ok(b"\n".to_vec()) }).contains("empty or not one printable token"));
    assert!(with(&|_: &[String]| -> Result<Vec<u8>, String> { Ok(b"cxr_x".to_vec()) }).contains("not a cxq_ token"));
    assert!(with(&|_: &[String]| -> Result<Vec<u8>, String> { Ok(b"cxq_a b".to_vec()) }).contains("not one printable token"));
    // a key that is not a PKCS#8 Ed25519 key fails at start, and the error does not hold it
    let bad_key = with(&|a: &[String]| -> Result<Vec<u8>, String> {
        if a.last().is_some_and(|p| p.ends_with("/key")) { Ok(b"MC4CAQAwBQYDK2VwBCIEIAnotakey".to_vec()) } else { vault(a) }
    });
    assert_eq!(bad_key, "the grant's key is not a PKCS#8 Ed25519 key");
    // a key kept in the file is refused, whatever its case
    for k in ["key", "Grant", "secret", "token", "PASSWORD"] {
        let bad = CONFIG.replacen('{', &format!("{{\"{k}\":\"abc\","), 1);
        assert!(Config::parse(&bad).err().unwrap_or_default().contains("never kept in a file"), "{k}");
    }
    // every field is needed
    for k in ["login", "host", "clax", "upstream", "queue", "owner", "source", "grant_cmd", "grant_key_cmd"] {
        let mut v: Value = serde_json::from_str(CONFIG).unwrap();
        v.as_object_mut().unwrap().remove(k);
        assert!(Config::parse(&v.to_string()).err().unwrap_or_default().contains(k), "{k}");
    }
    let field = |k: &str, x: Value| {
        let mut v: Value = serde_json::from_str(CONFIG).unwrap();
        v[k] = x;
        Config::parse(&v.to_string()).err().unwrap_or_default()
    };
    for clax in ["localhost:7480", "http://", "http://localhost:7480/q/basis", "ftp://x", "http://u@x"] {
        assert!(field("clax", clax.into()).contains("not an http(s) origin"), "{clax}");
    }
    // the instance is called on loopback only, in the clear only where nothing leaves the machine
    for up in ["https://127.0.0.1:7480", "http://clax.example.ts.net:7480", "http://127.0.0.1", "http://10.0.0.1:7480", "http://127.0.0.1:0"] {
        assert!(field("upstream", up.into()).contains("not an http origin on loopback"), "{up}");
    }
    for q in ["", "a/b", "a b", "../x"] {
        assert!(!field("queue", q.into()).is_empty(), "{q:?}");
    }
    assert!(!field("grant_cmd", "op read x".into()).is_empty(), "an argv, never a shell line");
    assert_eq!(loopback("http://[::1]:7480"), Some(("::1".to_string(), 7480)));
    assert_eq!(loopback("http://localhost:7480/"), Some(("localhost".to_string(), 7480)));
}

#[test]
fn the_real_minter_makes_the_clax_request_and_sends_nothing_unsigned() {
    let r = Config::parse(CONFIG).unwrap().with_grant(&vault).unwrap();
    // signed, then sent to port 9, where nothing listens: it fails, and says so without either secret
    let e = r.minter.mint(PK, "https://phobos.example.ts.net", ME).unwrap_err();
    assert!(e.contains("127.0.0.1:9"), "{e}");
    assert!(!e.contains("cxq_") && !e.contains(&test_key()), "no secret in an error: {e}");
    let minter = ClaxMinter::new("http://127.0.0.1:7480".into(), QUEUE.into(), "cxq_TESTONLY".into(), "test-only-key".into());
    let m = minter.request(PK, "https://phobos.example.ts.net", ME);
    assert_eq!(m.0, "/api/queues/basis/reporter-tokens");
    assert!(m.1.iter().any(|(k, v)| k == "Authorization" && v == "Clax-Queue cxq_TESTONLY"));
    assert!(!m.1.iter().any(|(k, _)| k.eq_ignore_ascii_case("Origin") || k.eq_ignore_ascii_case("Cookie")), "a service call carries neither");
    let body: Value = serde_json::from_str(&m.2).unwrap();
    assert_eq!(body, serde_json::json!({ "reporter_id": ME, "origin": "https://phobos.example.ts.net", "public_key": PK }));
}

/// A minter that records what it was asked and answers as the instance would.
#[derive(Clone, Default)]
struct Stub(Arc<Mutex<Vec<String>>>);

impl Minter for Stub {
    fn mint(&self, public_key: &str, origin: &str, reporter_id: &str) -> Result<Minted, String> {
        self.0.lock().unwrap().push(format!("{public_key} {origin} {reporter_id}"));
        if public_key.starts_with("down") {
            return Err("clax answered 503: busy".into());
        }
        Ok(Minted { token: "cxr_minted".into(), expires_at: "2026-10-31T18:15:00Z".into() })
    }
}

/// A fake Clax instance on loopback: its overlay (sent chunked) for `/_clax/overlay.js`, a 404 for anything else. The port.
fn instance(overlay: &'static [u8]) -> u16 {
    let l = TcpListener::bind("127.0.0.1:0").unwrap();
    let port = l.local_addr().unwrap().port();
    std::thread::spawn(move || {
        for s in l.incoming() {
            let Ok(mut s) = s else { continue };
            let Ok(req) = parse(&mut BufReader::new(s.try_clone().unwrap()), 4096) else { continue };
            let out = if req.target == "/_clax/overlay.js" && req.method == "GET" {
                let (a, b) = overlay.split_at(overlay.len() / 2);
                let mut o = b"HTTP/1.1 200 OK\r\nContent-Type: text/javascript\r\nTransfer-Encoding: chunked\r\n\r\n".to_vec();
                for c in [a, b] {
                    o.extend(format!("{:x}\r\n", c.len()).as_bytes());
                    o.extend(c);
                    o.extend(b"\r\n");
                }
                o.extend(b"0\r\n\r\n");
                o
            } else {
                b"HTTP/1.1 404 Not Found\r\nContent-Length: 9\r\n\r\nnot found".to_vec()
            };
            let _ = s.write_all(&out);
        }
    });
    port
}

fn site(reporting: bool, upstream: String, stub: &Stub) -> Site {
    let root = std::env::temp_dir().join(format!("fold-report-{}", std::process::id()));
    let reporting = reporting.then(|| Reporting {
        login: ME.into(),
        host: HOST.into(),
        clax: "https://clax.example.ts.net".into(),
        upstream,
        queue: QUEUE.into(),
        owner: OWNER.into(),
        minter: Box::new(stub.clone()),
        source: "fleet-map".into(),
        filer: None,
    });
    Site { root, mount: "/fold".into(), no_store: vec![], acting: None, reporting }
}

/// A filer that records each comment, and refuses one whose words start with "down".
#[derive(Clone, Default)]
struct Filed(Arc<Mutex<Vec<Comment>>>);

impl Filer for Filed {
    fn file(&self, c: &Comment) -> Result<(), String> {
        if c.body.starts_with("down") {
            return Err("clax answered 503: busy".into());
        }
        self.0.lock().unwrap().push(c.clone());
        Ok(())
    }
}

/// A site with one fold page (`needs/`, titled with an `&`), the viewer's shell at the root, and a filer if `filed` is given.
fn comment_site(name: &str, filed: Option<&Filed>) -> Site {
    let mut s = site(true, "http://127.0.0.1:9".into(), &Stub::default());
    s.root = std::env::temp_dir().join(format!("fold-report-{name}-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&s.root);
    std::fs::create_dir_all(s.root.join("needs")).unwrap();
    std::fs::write(s.root.join("index.html"), "<!doctype html><main><h1 tabindex=\"-1\">Loading…</h1></main>\n").unwrap();
    std::fs::write(
        s.root.join("needs/index.html"),
        "<!doctype html><title>Needs</title><main><h1 id=\"needs\" tabindex=\"-1\">Needs &amp; you</h1><ol class=\"rows\" role=\"list\" aria-label=\"Needs &amp; you\"><li class=\"prop\"><span>One thing.</span></li></ol></main>\n",
    )
    .unwrap();
    if let Some(f) = filed {
        s.reporting.as_mut().unwrap().filer = Some(Box::new(f.clone()));
    }
    s
}

#[test]
fn with_js_off_a_fold_offers_comment_in_clax_and_takes_it_as_a_form() {
    let form = |b: &str| from_me("POST", "/comment/needs/", b, &[("Content-Type", Some("application/x-www-form-urlencoded"))]);
    let text = |r: &fold_core::http::Response| String::from_utf8_lossy(&r.body).to_string();

    // the fold's page, served, ends its list with the link; the viewer's shell (no list) is left as it is
    let s = comment_site("off", None);
    let page = text(&answer(&s, &from_me("GET", "/needs/", "", &[]), NOW).0);
    assert!(page.contains("<li class=\"prop\"><a href=\"/fold/comment/needs/\">Comment in Clax</a></li></ol></main>"), "{page}");
    assert!(!text(&answer(&s, &from_me("GET", "/", "", &[]), NOW).0).contains("Comment in Clax"));
    // and not where this server does not report
    let mut quiet = comment_site("quiet", None);
    quiet.reporting = None;
    assert!(!text(&answer(&quiet, &from_me("GET", "/needs/", "", &[]), NOW).0).contains("Comment in Clax"));
    assert_eq!(answer(&quiet, &from_me("GET", "/comment/needs/", "", &[]), NOW).0.status, 404);

    // the comment page: the fold's shape, a named box that is never prefilled, its button, the way back
    let (res, log) = answer(&s, &from_me("GET", "/comment/needs/", "", &[]), NOW);
    let html = text(&res);
    assert_eq!((res.status, log.is_none()), (200, true));
    assert!(html.contains("<h1 tabindex=\"-1\">Comment on Needs &amp; you</h1>"), "{html}");
    assert!(html.contains("<form aria-label=\"Comment\" method=\"post\" action=\"/fold/comment/needs/\">"), "{html}");
    assert!(html.contains("<textarea aria-label=\"Your comment\" name=\"body\" required=\"\" maxlength=\"4000\"></textarea>"), "{html}");
    assert!(html.contains("<button type=\"submit\">Send to Clax</button>"), "{html}");
    // fold-core renders it, so it carries the stamp and passes the fold contract (src/check.rs, 196765f)
    assert_eq!(fold_core::check::page(&html), Ok(()), "{html}");
    assert!(html.contains("<a href=\"/fold/needs/\">Back to Needs &amp; you</a>"), "{html}");
    assert!(html.contains("goes to personal-allie-lead through Clax"), "{html}");
    // a fold with no page: the page of a fold that is not there
    assert_eq!(answer(&s, &from_me("GET", "/comment/gone/", "", &[]), NOW).0.status, 404);

    // no filer yet: 503, said, nothing sent
    let (res, log) = answer(&s, &form("body=Ship+it"), NOW);
    assert_eq!((res.status, log.unwrap()["result"].as_str()), (503, Some("off")));
    assert!(text(&res).contains("Comments from here are not on yet, so nothing was sent."));

    // with a filer: filed about the fold, then back to it
    let filed = Filed::default();
    let s = comment_site("on", Some(&filed));
    let (res, log) = answer(&s, &form("body=Ship+it%2C+please.%0A"), NOW);
    assert_eq!((res.status, res.header("Location")), (303, Some("/fold/needs/")));
    let log = log.unwrap();
    assert_eq!((log["result"].as_str(), log["comment"].as_str()), (Some("filed"), Some("https://ipso.bounded.tools/fleet-map/needs")));
    assert!(!log.to_string().contains("Ship"), "the log never holds the words");
    assert_eq!(
        *filed.0.lock().unwrap(),
        [Comment {
            external_id: "https://ipso.bounded.tools/fleet-map/needs".into(),
            url: "https://phobos.example.ts.net/fold/needs/".into(),
            title: "Needs & you".into(),
            body: "Ship it, please.".into(),
        }]
    );

    // refused, each before anything is filed
    let long = format!("body={}", "a".repeat(4001));
    let refusals: Vec<(&str, Request, u16)> = vec![
        ("cross-site", from_me("POST", "/comment/needs/", "body=x", &[("Content-Type", Some("application/x-www-form-urlencoded")), ("Sec-Fetch-Site", Some("cross-site"))]), 403),
        ("another login", from_me("POST", "/comment/needs/", "body=x", &[("Content-Type", Some("application/x-www-form-urlencoded")), ("Tailscale-User-Login", Some("eve@example.com"))]), 403),
        ("JSON, not a form", from_me("POST", "/comment/needs/", "{\"body\":\"x\"}", &[]), 415),
        ("nothing written", form("body=+%0A"), 400),
        ("no body field", form("words=x"), 400),
        ("too long", form(&long), 400),
        ("Clax does not take it", form("body=down+again"), 502),
        ("no PUT", from_me("PUT", "/comment/needs/", "body=x", &[]), 405),
    ];
    for (what, r, status) in refusals {
        let before = filed.0.lock().unwrap().len();
        assert_eq!(answer(&s, &r, NOW).0.status, status, "{what}");
        assert_eq!(filed.0.lock().unwrap().len(), before, "{what}: nothing filed");
    }
    assert_eq!(limit("POST", "/comment/needs/"), fold_core::serve::COMMENT_BODY);
    for name in ["off", "quiet", "on"] {
        let _ = std::fs::remove_dir_all(std::env::temp_dir().join(format!("fold-report-{name}-{}", std::process::id())));
    }
}

/// A request as tailscale serve hands one on from ME's own page, with `change` applied (None drops a header).
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
    Request { method: method.into(), target: target.into(), headers, body: body.as_bytes().to_vec(), peer: Some("127.0.0.1".parse().unwrap()) }
}

const NOW: i64 = 1_793_361_600;

#[test]
fn report_token_answers_only_the_one_login_same_origin() {
    let stub = Stub::default();
    let s = site(true, "http://127.0.0.1:9".into(), &stub);
    let key = format!("{{\"public_key\":\"{PK}\"}}");
    let json = |res: &fold_core::http::Response| serde_json::from_slice::<Value>(&res.body).unwrap();

    // empty: what the page mounts the overlay with
    let (res, log) = answer(&s, &from_me("POST", REPORT_TOKEN, "", &[]), NOW);
    assert_eq!((res.status, res.header("Cache-Control")), (200, Some("no-store")));
    assert_eq!(json(&res), serde_json::json!({ "instance": "https://clax.example.ts.net", "queue": QUEUE, "owner": OWNER }));
    assert_eq!(log.unwrap()["result"], "mount");
    // a key: the instance's token for it, for this origin and this login
    let (res, log) = answer(&s, &from_me("POST", REPORT_TOKEN, &key, &[]), NOW);
    assert_eq!(res.status, 200);
    assert_eq!(json(&res), serde_json::json!({ "token": "cxr_minted", "expires_at": "2026-10-31T18:15:00Z" }));
    assert_eq!(res.header("ETag"), None, "a token is never revalidated or kept");
    let log = log.unwrap();
    assert_eq!(log["result"], "minted");
    assert!(!log.to_string().contains("cxr_") && !log.to_string().contains(PK), "the log holds no token and no key");
    assert_eq!(*stub.0.lock().unwrap(), [format!("{PK} https://phobos.example.ts.net {ME}")]);
    // an older browser: no Sec-Fetch-Site, the right Origin
    let older = from_me("POST", REPORT_TOKEN, "", &[("Sec-Fetch-Site", None), ("Origin", Some("https://phobos.example.ts.net"))]);
    assert_eq!(answer(&s, &older, NOW).0.status, 200);

    let mut far = from_me("POST", REPORT_TOKEN, &key, &[]);
    far.peer = Some("10.0.0.9".parse().unwrap());
    let refusals: Vec<(&str, Request, u16, &str)> = vec![
        ("not from loopback", far, 403, "not from the tailnet front"),
        ("no login", from_me("POST", REPORT_TOKEN, &key, &[("Tailscale-User-Login", None)]), 403, "not who this server acts for"),
        ("another login", from_me("POST", REPORT_TOKEN, &key, &[("Tailscale-User-Login", Some("eve@example.com"))]), 403, "not who this server acts for"),
        ("a Q-encoded login", from_me("POST", REPORT_TOKEN, &key, &[("Tailscale-User-Login", Some("=?utf-8?q?bobby?="))]), 403, "unexpected login encoding"),
        ("not through serve", from_me("POST", REPORT_TOKEN, &key, &[("X-Forwarded-Proto", None)]), 403, "not through tailscale serve"),
        ("another host", from_me("POST", REPORT_TOKEN, &key, &[("X-Forwarded-Host", Some("evil.example"))]), 403, "not through tailscale serve"),
        ("cross-site", from_me("POST", REPORT_TOKEN, &key, &[("Sec-Fetch-Site", Some("cross-site"))]), 403, "cross-site"),
        ("same-site is not same-origin", from_me("POST", REPORT_TOKEN, &key, &[("Sec-Fetch-Site", Some("same-site"))]), 403, "cross-site"),
        ("same-origin with another Origin", from_me("POST", REPORT_TOKEN, &key, &[("Origin", Some("https://evil.example"))]), 403, "cross-site"),
        ("no Sec-Fetch-Site and no Origin", from_me("POST", REPORT_TOKEN, &key, &[("Sec-Fetch-Site", None)]), 403, "cross-site"),
        ("a form, not JSON", from_me("POST", REPORT_TOKEN, &format!("public_key={PK}"), &[("Content-Type", Some("application/x-www-form-urlencoded"))]), 415, "send the key as application/json"),
        ("no public_key", from_me("POST", REPORT_TOKEN, "{}", &[]), 400, "no public_key, or not one"),
        ("not a key", from_me("POST", REPORT_TOKEN, r#"{"public_key":"a b<script>"}"#, &[]), 400, "no public_key, or not one"),
        ("not JSON", from_me("POST", REPORT_TOKEN, "{", &[]), 400, "no public_key, or not one"),
    ];
    for (what, r, status, why) in refusals {
        let before = stub.0.lock().unwrap().len();
        let (res, log) = answer(&s, &r, NOW);
        assert_eq!(res.status, status, "{what}");
        assert_eq!(String::from_utf8_lossy(&res.body).trim(), why, "{what}");
        assert_eq!(log.unwrap()["result"], format!("refused:{status}"), "{what}");
        assert_eq!(stub.0.lock().unwrap().len(), before, "{what}: nothing minted");
    }
    // the instance did not mint: 502, said
    let down = format!("{{\"public_key\":\"down{}\"}}", &PK[4..]);
    let (res, log) = answer(&s, &from_me("POST", REPORT_TOKEN, &down, &[]), NOW);
    assert_eq!((res.status, log.unwrap()["result"].as_str()), (502, Some("clax:failed")));
    // only POST
    for m in ["GET", "HEAD", "PUT"] {
        let (res, log) = answer(&s, &from_me(m, REPORT_TOKEN, "", &[]), NOW);
        assert_eq!((res.status, res.header("Allow"), log.is_none()), (405, Some("POST"), true), "{m}");
    }
    // a key's body is taken, up to a bound; no other POST body is
    assert_eq!(limit("POST", "/report-token"), REPORT_BODY);
    assert_eq!(limit("POST", "/report-token?x"), REPORT_BODY);
    assert_eq!(limit("GET", "/report-token"), 0);
    // without --report-config both routes say reporting is off
    let off = site(false, String::new(), &stub);
    for (m, t) in [("POST", REPORT_TOKEN), ("GET", OVERLAY)] {
        let (res, log) = answer(&off, &from_me(m, t, "", &[]), NOW);
        assert_eq!((res.status, log.is_none()), (404, true), "{t}");
        assert_eq!(String::from_utf8_lossy(&res.body).trim(), "reporting to Clax isn't on here");
    }
}

#[test]
fn the_overlay_is_the_instances_bytes_served_from_this_origin() {
    const JS: &[u8] = b"var Clax=(function(F){\"use strict\";return F})({});\n";
    let stub = Stub::default();
    let s = site(true, format!("http://127.0.0.1:{}", instance(JS)), &stub);
    let (res, log) = answer(&s, &from_me("GET", OVERLAY, "", &[]), NOW);
    assert_eq!(res.status, 200);
    assert_eq!(res.body, JS, "byte for byte, so the page's SRI digest holds");
    assert_eq!(res.header("Content-Type"), Some("text/javascript; charset=utf-8"));
    assert_eq!(res.header("Cache-Control"), Some("no-cache"));
    assert!(log.is_none());
    assert_eq!(answer(&s, &from_me("POST", OVERLAY, "", &[]), NOW).0.status, 405);
    // the instance down: 502, never a stale or empty script
    let gone = site(true, "http://127.0.0.1:9".into(), &stub);
    assert_eq!(answer(&gone, &from_me("GET", OVERLAY, "", &[]), NOW).0.status, 502);
    // the client itself: never off loopback
    assert!(call("http://example.com:80", "GET", "/", &[], b"").err().unwrap_or_default().contains("not an http origin on loopback"));
}

/// A fake Clax instance's reports route: it records each request whole and answers `status`. The port.
fn reports(status: u16, seen: Arc<Mutex<Vec<Request>>>) -> u16 {
    let l = TcpListener::bind("127.0.0.1:0").unwrap();
    let port = l.local_addr().unwrap().port();
    std::thread::spawn(move || {
        for s in l.incoming() {
            let Ok(mut s) = s else { continue };
            let Ok(req) = parse(&mut BufReader::new(s.try_clone().unwrap()), 1 << 20) else { continue };
            seen.lock().unwrap().push(req);
            let body = if status < 300 { r#"{"thread":{"id":"t1"}}"# } else { r#"{"error":"wrong_origin","echo":"cxr_minted"}"# };
            let _ = s.write_all(format!("HTTP/1.1 {status} X\r\nContent-Type: application/json\r\nContent-Length: {}\r\n\r\n{body}", body.len()).as_bytes());
        }
    });
    port
}

#[test]
fn the_server_side_reporter_files_a_signed_report_about_the_fold() {
    use ed25519_dalek::{Signature, Verifier, VerifyingKey};
    let seen = Arc::new(Mutex::new(vec![]));
    let up = format!("http://127.0.0.1:{}", reports(201, seen.clone()));
    let stub = Stub::default();
    let filer = ClaxFiler::new(up.clone(), QUEUE.into(), "https://phobos.example.ts.net".into(), ME.into(), Box::new(stub.clone()));
    let c = Comment {
        external_id: "https://ipso.bounded.tools/fleet-map/needs".into(),
        url: "https://phobos.example.ts.net/fold/needs/".into(),
        title: "Needs & you".into(),
        body: "Ship it, please.".into(),
    };
    filer.file_with(&c, &[0x42; 32], 1_760_000_000, "AAECAwQFBgcICQoLDA0ODw", "fold-b").unwrap();

    // the token was minted for the reporter key's public half, this origin and this login
    let pk = "IVL40Zt5HSRFMkLhXy6rbLfP-ntqXtMAl5YOBpiB2xI";
    assert_eq!(*stub.0.lock().unwrap(), [format!("{pk} https://phobos.example.ts.net {ME}")]);
    // the report: the route, the reporter's token, the token's origin, a multipart body with the artifact anchor
    let reqs = seen.lock().unwrap();
    let r = &reqs[0];
    assert_eq!((r.method.as_str(), r.target.as_str()), ("POST", "/api/queues/basis/reporter/reports"));
    assert_eq!(r.header("Authorization"), Some("Clax-Reporter cxr_minted"));
    assert_eq!(r.header("Origin"), Some("https://phobos.example.ts.net"));
    assert_eq!(r.header("Content-Type"), Some("multipart/form-data; boundary=fold-b"));
    let body = String::from_utf8(r.body.clone()).unwrap();
    for (k, v) in [("external_id", c.external_id.as_str()), ("url", c.url.as_str()), ("title", "Needs & you"), ("anchor", r#"{"kind":"artifact","file":""}"#), ("body", "Ship it, please.")] {
        assert!(body.contains(&format!("--fold-b\r\nContent-Disposition: form-data; name=\"{k}\"\r\n\r\n{v}\r\n")), "{k}: {body}");
    }
    assert!(body.ends_with("--fold-b--\r\n"));
    // the signature verifies, by the reporter's public key, over the request as it arrived (not as the sender thought it)
    let sig = r.header("Clax-Signature").unwrap();
    let (t, n, s) = {
        let p: std::collections::HashMap<&str, &str> = sig.split(", ").filter_map(|kv| kv.split_once('=')).collect();
        (p["t"].parse::<u64>().unwrap(), p["n"], p["s"])
    };
    let host = r.header("Host").unwrap().to_string();
    let input = fold_core::sig::SignInput { method: &r.method, target: &r.target, host: &host, t, nonce: n, body: &r.body, token: "cxr_minted", agent: &[] };
    let vk = VerifyingKey::from_bytes(&fold_core::sig::unb64url(pk).unwrap().try_into().unwrap()).unwrap();
    let sig = Signature::from_slice(&fold_core::sig::unb64url(s).unwrap()).unwrap();
    assert!(vk.verify(input.canonical().unwrap().as_bytes(), &sig).is_ok(), "the instance can verify it");
    drop(reqs);

    // the instance refuses: said, without the reporter's token
    let refused = ClaxFiler::new(format!("http://127.0.0.1:{}", reports(403, Arc::new(Mutex::new(vec![])))), QUEUE.into(), "https://phobos.example.ts.net".into(), ME.into(), Box::new(Stub::default()));
    let e = refused.file_with(&c, &[0x42; 32], 1_760_000_000, "AAECAwQFBgcICQoLDA0ODw", "fold-b").unwrap_err();
    assert!(e.starts_with("clax answered 403") && !e.contains("cxr_minted"), "{e}");
    // a mint that fails files nothing
    let none = Arc::new(Mutex::new(vec![]));
    let downs = ClaxFiler::new(format!("http://127.0.0.1:{}", reports(201, none.clone())), QUEUE.into(), "https://x".into(), ME.into(), Box::new(Down));
    assert!(downs.file(&c).is_err());
    assert!(none.lock().unwrap().is_empty());
}

/// A minter whose instance never mints.
struct Down;

impl Minter for Down {
    fn mint(&self, _: &str, _: &str, _: &str) -> Result<Minted, String> {
        Err("clax answered 503: busy".into())
    }
}

#[test]
fn an_answer_is_read_by_its_length_or_its_chunks() {
    let a = answer_of(b"HTTP/1.1 201 Created\r\nContent-Length: 2\r\n\r\nokEXTRA").unwrap();
    assert_eq!((a.status, a.body.as_slice()), (201, &b"ok"[..]));
    let c = answer_of(b"HTTP/1.1 200 OK\r\ntransfer-encoding: Chunked\r\n\r\n3\r\nabc\r\n2;x=y\r\nde\r\n0\r\n\r\n").unwrap();
    assert_eq!(c.body, b"abcde");
    assert!(answer_of(b"HTTP/1.1 200 OK\r\nContent-Length: 9\r\n\r\nshort").is_none(), "cut short");
    assert!(answer_of(b"HTTP/1.1 200 OK\r\nTransfer-Encoding: chunked\r\n\r\nzz\r\n").is_none(), "not a chunk");
    assert!(answer_of(b"garbage").is_none());
}
