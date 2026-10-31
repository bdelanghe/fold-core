//! Fuzz for both of fold's processes (brief item 27): bytes shaped like requests, half well-formed and half not, through http.rs's
//! `parse` and then `fold serve` and `fold sink`. Neither may panic or answer outside its statuses; serve never serves a byte from
//! outside its root and never writes there; the sink logs only what it accepted, from a tailnet identity. Seeded, so a failure
//! repeats: `FOLD_FUZZ=<n>` runs n cases a side (default 20 000).

use std::collections::HashSet;
use std::fs;
use std::io::BufReader;

use chrono::{DateTime, Utc};
use fold_core::http::parse;
use fold_core::serve::{respond, Site};
use fold_core::sink::{accept, Limiter, MAX_BODY};

/// xorshift64*: enough to pick from lists, and the same every run.
struct Rng(u64);

impl Rng {
    fn next(&mut self) -> u64 {
        self.0 ^= self.0 >> 12;
        self.0 ^= self.0 << 25;
        self.0 ^= self.0 >> 27;
        self.0.wrapping_mul(0x2545_f491_4f6c_dd1d)
    }
    fn below(&mut self, n: usize) -> usize {
        (self.next() % n as u64) as usize
    }
    fn pick<'a>(&mut self, xs: &[&'a str]) -> &'a str {
        xs[self.below(xs.len())]
    }
    fn bytes(&mut self, n: usize) -> Vec<u8> {
        (0..n).map(|_| self.next() as u8).collect()
    }
}

const METHODS: &[&str] = &["GET", "HEAD", "POST", "PUT", "DELETE", "OPTIONS", "get", "", "GET GET", "\u{0}"];
const TARGETS: &[&str] = &[
    "/",
    "/index.html",
    "/a%20b/",
    "/a%20b",
    "/v",
    "/report",
    "/_v",
    "/%2e%2e/secret",
    "/../secret",
    "/a/./b",
    "/%00",
    "/%zz",
    "/%c3%28",
    "/a\\b",
    "//",
    "/sign/x",
    "*",
    "",
    "/?q=1#f",
    "/fold@sha256:00.css",
    "/%2fsecret",
];
const NAMES: &[&str] = &[
    "Content-Type",
    "Content-Length",
    "Transfer-Encoding",
    "If-None-Match",
    "Tailscale-User-Login",
    "Ping-To",
    "Sec-Purpose",
    "content-length",
    "X",
];
const VALUES: &[&str] = &[
    "text/ping",
    "application/reports+json",
    "0",
    "4",
    "-1",
    "99999999999999999999",
    "65537",
    "chunked",
    "*",
    "W/\"x\"",
    "bobby@example.com",
    "",
    "prefetch",
    "a, b",
];

/// One request's bytes.
fn request(r: &mut Rng) -> Vec<u8> {
    if r.below(8) == 0 {
        let n = r.below(300);
        return r.bytes(n);
    }
    if r.below(4) == 0 {
        // near a request the sink accepts: a ping or a report, one header perhaps wrong, the body perhaps not JSON
        let (target, ct, body) =
            if r.below(2) == 0 { ("/v", "text/ping", "PING") } else { ("/report", "application/reports+json", "[{}]") };
        let who = r.pick(&["bobby@example.com", "bobby@example.com", ""]);
        let body = if r.below(4) == 0 { "{" } else { body };
        return format!(
            "POST {target} HTTP/1.1\r\nTailscale-User-Login: {who}\r\nContent-Type: {ct}\r\nContent-Length: {}\r\n\r\n{body}",
            body.len()
        )
        .into_bytes();
    }
    let mut o = format!("{} {} HTTP/1.1\r\n", r.pick(METHODS), r.pick(TARGETS)).into_bytes();
    for _ in 0..r.below(6) {
        o.extend(format!("{}: {}\r\n", r.pick(NAMES), r.pick(VALUES)).bytes());
    }
    if r.below(10) == 0 {
        let n = r.below(40);
        o.extend(r.bytes(n));
    }
    o.extend(b"\r\n");
    let n = r.below(80);
    o.extend(r.bytes(n));
    o
}

fn cases() -> usize {
    std::env::var("FOLD_FUZZ").ok().and_then(|v| v.parse().ok()).unwrap_or(20_000)
}

#[test]
fn serve_answers_anything_and_never_leaves_its_root() {
    let base = std::env::temp_dir().join(format!("fold-fuzz-{}", std::process::id()));
    let _ = fs::remove_dir_all(&base);
    let root = base.join("root");
    for (p, body) in [("index.html", "root"), ("a b/index.html", "ab"), ("sign/x", "live"), ("fold@sha256:00.css", "css")] {
        fs::create_dir_all(root.join(p).parent().unwrap()).unwrap();
        fs::write(root.join(p), body).unwrap();
    }
    fs::write(base.join("secret"), "SECRET").unwrap();
    let inside: HashSet<Vec<u8>> = ["root", "ab", "live", "css"].iter().map(|s| s.as_bytes().to_vec()).collect();
    let site = Site { root: root.clone(), mount: "/fold".into(), no_store: vec!["sign/".into()] };
    let listing = |p: &std::path::Path| {
        let mut v: Vec<_> = fs::read_dir(p).unwrap().map(|e| e.unwrap().file_name()).collect();
        v.sort();
        v
    };
    let before = listing(&root);
    let mut r = Rng(0x5eed_f01d);
    for i in 0..cases() {
        let raw = request(&mut r);
        let res = match parse(&mut BufReader::new(&raw[..]), 0) {
            Ok(req) => respond(&site, &req),
            Err(res) => res,
        };
        assert!([200, 301, 304, 400, 404, 405, 413].contains(&res.status), "case {i}: {} for {raw:?}", res.status);
        if res.status == 200 {
            assert!(inside.contains(&res.body), "case {i}: served bytes from outside its root for {raw:?}");
        }
    }
    assert_eq!(listing(&root), before, "serve wrote into its root");
    let _ = fs::remove_dir_all(&base);
}

#[test]
fn the_sink_answers_anything_and_logs_only_what_it_accepted() {
    let at: DateTime<Utc> = "2026-10-31T01:00:00Z".parse().unwrap();
    let mut lim = Limiter::new(u32::MAX);
    let mut r = Rng(0x0dd_ba11);
    let mut accepted = 0;
    for i in 0..cases() {
        let raw = request(&mut r);
        let (res, rec) = match parse(&mut BufReader::new(&raw[..]), MAX_BODY) {
            Ok(req) => accept(&mut lim, &req, at),
            Err(res) => (res, None),
        };
        assert!([204, 400, 403, 404, 405, 413, 415, 429].contains(&res.status), "case {i}: {} for {raw:?}", res.status);
        assert_eq!(rec.is_some(), res.status == 204, "case {i}: a record without a 204, or the reverse, for {raw:?}");
        if let Some(rec) = rec {
            accepted += 1;
            assert!(rec["viewer"].as_str().is_some_and(|v| !v.is_empty()), "case {i}: logged with no identity");
        }
    }
    assert!(accepted > 0, "the fuzz never reached an accepted request: widen it");
}
