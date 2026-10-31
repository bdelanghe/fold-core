//! fold serve (src/serve.rs, brief items 23, 24 and 27): every resource class's headers, and every write refused, held in
//! goldens/serve/headers.json. `FOLD_BLESS=1 cargo test --test serve` writes it. One more test runs the real loop on a loopback
//! port, for what only it adds (`serve;dur`, a body refused before it is read).

use std::fs;
use std::io::{Read, Write};
use std::net::{TcpListener, TcpStream};
use std::path::PathBuf;
use std::time::{Duration, SystemTime};

use fold_core::serve::{respond, run, Request, Site};
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
    Site { root, mount: "/fold".into(), no_store: vec!["sign/".into()] }
}

fn req(method: &str, target: &str, headers: &[(&str, &str)], body: &str) -> Request {
    Request {
        method: method.into(),
        target: target.into(),
        headers: headers.iter().map(|(k, v)| (k.to_string(), v.to_string())).collect(),
        body: body.as_bytes().to_vec(),
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
    std::thread::spawn(move || run(l, s));
    let out = over_the_wire(port, b"GET /a%20b/ HTTP/1.1\r\nHost: x\r\n\r\n");
    assert!(out.starts_with("HTTP/1.1 200 OK\r\n"), "{out}");
    assert!(out.lines().any(|l| l.starts_with("Server-Timing: digest;desc=\"sha-256:") && l.contains(", serve;dur=")), "{out}");
    assert!(out.ends_with("<h1>Desk</h1></main>\n"), "{out}");
    let out = over_the_wire(port, b"POST /_v HTTP/1.1\r\nContent-Type: text/ping\r\nContent-Length: 4\r\n\r\nPING");
    assert!(out.starts_with("HTTP/1.1 413 "), "{out}");
    // it wrote nothing: the directory is the one site() made
    let mut names: Vec<String> = fs::read_dir(&root).unwrap().map(|e| e.unwrap().file_name().into_string().unwrap()).collect();
    names.sort();
    assert_eq!(names, ["a b", "fleet", "fold@sha256:5e1f.css", "index.html", "sign", "sw.js", "view.js"]);
    let _ = fs::remove_dir_all(&root);
}
