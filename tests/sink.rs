//! fold sink (src/sink.rs, brief items 21, 24 and 27): its answers and the records they append, held in
//! goldens/sink/answers.json. `FOLD_BLESS=1 cargo test --test sink` writes it. One more test runs the real loop on a loopback
//! port, for what only it adds (the log line, appended).

use std::fs;
use std::io::{Read, Write};
use std::net::{TcpListener, TcpStream};
use std::path::PathBuf;
use std::time::Duration;

use chrono::{DateTime, Utc};
use fold_core::http::Request;
use fold_core::sink::{accept, run, Limiter};
use serde_json::{json, Value};

fn req(method: &str, target: &str, headers: &[(&str, &str)], body: &str) -> Request {
    Request {
        method: method.into(),
        target: target.into(),
        headers: headers.iter().map(|(k, v)| (k.to_string(), v.to_string())).collect(),
        body: body.as_bytes().to_vec(),
    }
}

#[test]
fn every_answer_and_what_it_appends() {
    let at: DateTime<Utc> = "2026-10-31T01:00:15Z".parse().unwrap();
    let me = ("Tailscale-User-Login", "bobby@example.com");
    let ping = [me, ("Content-Type", "text/ping"), ("Ping-From", "https://phobos/fold/"), ("Ping-To", "https://phobos/fold/a%20b/#a%20b")];
    let reports = r#"[{"type":"csp-violation","age":0,"url":"https://phobos/fold/","body":{"effectiveDirective":"script-src"}}]"#;
    let rj = ("Content-Type", "application/reports+json");
    let cases: Vec<(&str, Request)> = vec![
        ("a ping: the row followed, from and to; no body", req("POST", "/v", &ping, "PING")),
        ("reports from the Reporting API", req("POST", "/report", &[me, rj], reports)),
        ("no tailnet identity: refused, nothing logged", req("POST", "/v", &ping[1..], "PING")),
        ("an empty identity is none", req("POST", "/v", &[("Tailscale-User-Login", ""), ping[1]], "PING")),
        ("not a ping", req("POST", "/v", &[me, ("Content-Type", "application/json")], "{}")),
        ("reports that are not a JSON array", req("POST", "/report", &[me, rj], "{}")),
        ("reports that are not JSON", req("POST", "/report", &[me, rj], "[")),
        ("nothing else to post to", req("POST", "/approve", &[me], "")),
        ("nothing to read: append-only", req("GET", "/v", &[me], "")),
    ];
    let mut lim = Limiter::new(60);
    let mut got: Vec<Value> = cases
        .iter()
        .map(|(what, r)| {
            let (res, rec) = accept(&mut lim, r, at);
            json!({
                "what": what,
                "request": { "method": r.method, "target": r.target, "headers": r.headers },
                "status": res.status,
                "headers": res.headers,
                "record": rec,
            })
        })
        .collect();
    // the rate: two accepted above, so 58 more pass this minute and the 61st is refused until the next one
    let mut tight = Limiter::new(60);
    let statuses: Vec<u16> = (0..61).map(|_| accept(&mut tight, &cases[0].1, at).0.status).collect();
    assert!(statuses[..60].iter().all(|s| *s == 204) && statuses[60] == 429, "{statuses:?}");
    let (res, rec) = accept(&mut tight, &cases[0].1, at);
    got.push(json!({ "what": "the 62nd ping in a minute: refused, nothing logged", "status": res.status, "headers": res.headers, "record": rec }));
    let next: DateTime<Utc> = "2026-10-31T01:01:00Z".parse().unwrap();
    assert_eq!(accept(&mut tight, &cases[0].1, next).0.status, 204, "the next minute starts over");
    let another = req("POST", "/v", &[("Tailscale-User-Login", "allie@example.com"), ping[1]], "PING");
    assert_eq!(accept(&mut tight, &another, at).0.status, 204, "a viewer's rate is their own");

    let path = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("goldens/sink/answers.json");
    let text = serde_json::to_string_pretty(&got).unwrap() + "\n";
    if std::env::var("FOLD_BLESS").is_ok() {
        fs::create_dir_all(path.parent().unwrap()).unwrap();
        fs::write(&path, &text).unwrap();
    }
    assert!(
        text == fs::read_to_string(&path).unwrap_or_default(),
        "goldens/sink/answers.json is stale (FOLD_BLESS=1 cargo test --test sink)"
    );
}

#[test]
fn the_loop_appends_each_record() {
    let dir = std::env::temp_dir().join(format!("fold-sink-loop-{}", std::process::id()));
    let _ = fs::remove_dir_all(&dir);
    fs::create_dir_all(&dir).unwrap();
    let log = dir.join("signals.jsonl");
    fs::write(&log, "{\"kept\":true}\n").unwrap();
    let l = TcpListener::bind("127.0.0.1:0").unwrap();
    let port = l.local_addr().unwrap().port();
    let lg = log.clone();
    std::thread::spawn(move || run(l, 60, lg));
    let mut c = TcpStream::connect(("127.0.0.1", port)).unwrap();
    c.write_all(b"POST /v HTTP/1.1\r\nTailscale-User-Login: bobby@example.com\r\nContent-Type: text/ping\r\nPing-To: /fold/x/\r\nContent-Length: 4\r\n\r\nPING").unwrap();
    let mut out = String::new();
    c.read_to_string(&mut out).unwrap();
    assert!(out.starts_with("HTTP/1.1 204 No Content\r\n"), "{out}");
    // the record is written after the answer: wait for it
    let mut text = String::new();
    for _ in 0..100 {
        text = fs::read_to_string(&log).unwrap_or_default();
        if text.lines().count() == 2 {
            break;
        }
        std::thread::sleep(Duration::from_millis(10));
    }
    let lines: Vec<&str> = text.lines().collect();
    assert_eq!(lines[0], "{\"kept\":true}", "appended, never rewritten");
    let v: Value = serde_json::from_str(lines[1]).unwrap();
    assert_eq!((v["kind"].as_str(), v["to"].as_str()), (Some("ping"), Some("/fold/x/")));
    let _ = fs::remove_dir_all(&dir);
}
