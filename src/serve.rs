//! `fold serve` (brief items 23-24, split by item 27): the fold viewer and its data over HTTP, read-only. GET and HEAD of static
//! files, with the headers darkhttpd cannot send (ETag, Cache-Control, Server-Timing), and nothing else: it writes nothing, takes
//! no body, and every other method is 405. The one exception is an act's form (src/ax.rs `act_form`) submitted with no script:
//! its POST to `act/<id>` is read only to be dropped, and answered by a page saying acting from here isn't on yet ([`not_yet`]). It names no `fold sink` (sink.rs): the sink is kept but not deployed. Approvals go through neither; they stay chitin's `/approve`. [`respond`] is
//! pure (a request and a directory in, a response out), so tests/serve.rs holds every resource class's headers as goldens without
//! a socket; [`run`] is the loopback loop around it (http.rs, src/bin/fold.rs).
//!
//! - **Cache classes (item 23):** a path holding `@sha256:` (a locator, not an identity) is immutable; a page (an `.html` file, or
//!   a directory's `index.html`) and the viewer's other files are `no-cache`, always revalidated; a data document (`.json`) is
//!   `max-age=60, stale-while-revalidate=240, stale-if-error=86400`; a `--no-store` path (the Sign fold, pending approvals,
//!   presence) is `no-store` with no validator, and so never bfcache-eligible. `sw.js` is `no-cache`.
//! - **Validators:** the ETag is strong, `"sha-256:<hex>"`, the file digest (raw bytes, no canonical form: it is the bytes served),
//!   and `If-None-Match` answers 304 with no body. `Content-Digest` carries the same digest. No weak ETag is ever sent. A path is
//!   one file here, so nothing varies by `Accept` and no `Vary` is sent.
//! - **`Server-Timing`:** `digest;desc="sha-256:…"` and `asof;desc="<Last-Modified, ISO>"`; [`run`] adds `serve;dur=<ms>`.
//!   Nothing is rendered per request (the pages are rendered at publish time, page.rs), so there is no `render` entry.
//! - **No Reporting:** no `Reporting-Endpoints` and no report-only CSP, since no sink is deployed to take them (Bobby f2717c12).
//! - **Paths:** percent-decoded, as darkhttpd does; a `.` or `..` segment, a NUL or a backslash is refused (400). No listings.

use std::fs;
use std::net::TcpListener;
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::time::SystemTime;

use chrono::{DateTime, SecondsFormat, Utc};
use serde_json::json;
use sha2::{Digest, Sha256};

use crate::{ax, page};

pub use crate::http::{Request, Response};

/// One directory served under one mount (`/fold`, `/fold-data`): tailscale serve strips the mount, so requests arrive without it,
/// and only the URLs this server hands out (a redirect, the report endpoint) need it.
pub struct Site {
    pub root: PathBuf,
    pub mount: String,
    /// Paths under the mount that are live state and never cached (`sign/`, `pending/`): a prefix of the decoded path.
    pub no_store: Vec<String>,
}

/// How a resource may be cached (item 23's table), by its path.
#[derive(Clone, Copy, PartialEq, Debug)]
enum Class {
    Immutable,
    Page,
    Data,
    NoStore,
    Asset,
}

impl Class {
    fn cache_control(self) -> &'static str {
        match self {
            Class::Immutable => "public, max-age=31536000, immutable",
            Class::Page | Class::Asset => "no-cache",
            Class::Data => "max-age=60, stale-while-revalidate=240, stale-if-error=86400",
            Class::NoStore => "no-store",
        }
    }
}

fn content_type(p: &Path) -> &'static str {
    match p.extension().and_then(|e| e.to_str()).unwrap_or("") {
        "html" => "text/html; charset=utf-8",
        "js" | "mjs" => "text/javascript; charset=utf-8",
        "css" => "text/css; charset=utf-8",
        "json" => "application/json",
        "wasm" => "application/wasm",
        "woff2" => "font/woff2",
        "svg" => "image/svg+xml",
        "png" => "image/png",
        "txt" => "text/plain; charset=utf-8",
        _ => "application/octet-stream",
    }
}

/// `%XX` decoded, or None when it is not UTF-8 or a `%` has no two hex digits after it.
fn decode(s: &str) -> Option<String> {
    let b = s.as_bytes();
    let mut o = Vec::with_capacity(b.len());
    let mut i = 0;
    while i < b.len() {
        if b[i] == b'%' {
            let h = std::str::from_utf8(b.get(i + 1..i + 3)?).ok()?;
            o.push(u8::from_str_radix(h, 16).ok()?);
            i += 3;
        } else {
            o.push(b[i]);
            i += 1;
        }
    }
    String::from_utf8(o).ok()
}

fn base64(b: &[u8]) -> String {
    const A: &[u8; 64] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/";
    let mut o = String::new();
    for c in b.chunks(3) {
        let n = (c[0] as u32) << 16 | (*c.get(1).unwrap_or(&0) as u32) << 8 | *c.get(2).unwrap_or(&0) as u32;
        for k in 0..4 {
            o.push(if k <= c.len() { A[(n >> (18 - 6 * k) & 63) as usize] as char } else { '=' });
        }
    }
    o
}

fn http_date(t: DateTime<Utc>) -> String {
    t.format("%a, %d %b %Y %H:%M:%S GMT").to_string()
}

fn iso(t: DateTime<Utc>) -> String {
    t.to_rfc3339_opts(SecondsFormat::Secs, true)
}

/// Whether an `If-None-Match` names this ETag (or is `*`). A weak `W/` form of it matches too, as RFC 9110's weak comparison says.
fn matches(inm: &str, etag: &str) -> bool {
    inm.split(',').map(str::trim).any(|t| t == "*" || t == etag || t.strip_prefix("W/") == Some(etag))
}

/// The most an act's form may send (its cap, call and args as fields), read only to be dropped. Every other body is refused unread.
pub const ACT_BODY: usize = 8192;

/// How many body bytes a request may send: none, but for a POST to an act.
pub fn limit(method: &str, target: &str) -> usize {
    if method == "POST" && act(target).is_some() {
        ACT_BODY
    } else {
        0
    }
}

/// An act's target (`/act/<id>/…/<index>`, src/ax.rs `act_id`) as the path of the fold it is on, each id percent-decoded.
fn act(target: &str) -> Option<Vec<String>> {
    let path = target.split(['?', '#']).next()?.strip_prefix("/act/")?;
    let mut segs: Vec<&str> = path.split('/').collect();
    segs.pop()?.parse::<usize>().ok()?;
    segs.into_iter().map(|s| decode(s).filter(|s| !s.is_empty())).collect()
}

/// One request's answer.
pub fn respond(site: &Site, req: &Request) -> Response {
    let path = req.target.split(['?', '#']).next().unwrap_or("");
    match req.method.as_str() {
        "GET" | "HEAD" => file(site, req, path),
        "POST" => match act(&req.target) {
            Some(fold) => not_yet(site, &fold),
            None => Response::text(405, "read-only: GET or HEAD").with("Allow", "GET, HEAD"),
        },
        _ => Response::text(405, "read-only: GET or HEAD").with("Allow", "GET, HEAD"),
    }
}

/// An act's form submitted with no script to take it (BRIEF-basis-forms-push-signing-2026-10-31 §1, render only): this server
/// only reads, so it does nothing and says so on a page of its own, with the way back to the fold the act is on and to the root;
/// never a bare 405. The form's fields were read only to be dropped.
fn not_yet(site: &Site, fold: &[String]) -> Response {
    let base = format!("{}/", site.mount);
    let link = |name: String, to: &[String]| {
        json!({ "role": "listitem", "cls": "prop", "children": [{ "role": "link", "name": name, "href": page::href(&base, to, "folds"), "local": true }] })
    };
    let said = "Nothing was done: this server only reads. Acting from here comes once it can tell who is asking.";
    let mut rows = vec![json!({ "role": "listitem", "cls": "prop", "children": [{ "role": "text", "name": said }] })];
    if let Some(id) = fold.last() {
        rows.push(link(format!("Back to {id}"), fold));
    }
    rows.push(link(format!("Back to {}", ax::ROOT), &[]));
    let title = "Acting from here isn't on yet";
    let tree = json!({ "role": "main", "children": [
        { "role": "heading", "name": title, "focus": true },
        { "role": "list", "name": title, "cls": "rows", "children": rows },
    ] });
    let mut r = Response::new(405).with("Allow", "GET, HEAD").with("Cache-Control", "no-store");
    match page::page(&tree, &base) {
        Ok(h) => {
            r = r.with("Content-Type", "text/html; charset=utf-8");
            r.body = h.into_bytes();
        }
        Err(e) => r = Response::text(405, &e).with("Allow", "GET, HEAD"),
    }
    r
}

fn file(site: &Site, req: &Request, path: &str) -> Response {
    let Some(rel) = decode(path).filter(|p| p.starts_with('/')) else {
        return Response::text(400, "not a path");
    };
    let segs: Vec<&str> = rel[1..].split('/').collect();
    if segs.iter().any(|s| *s == "." || *s == ".." || s.contains('\0') || s.contains('\\')) {
        return Response::text(400, "no . or .. segments");
    }
    let mut fp = site.root.clone();
    fp.extend(segs.iter().filter(|s| !s.is_empty()));
    if fp.is_dir() {
        if !path.ends_with('/') {
            return Response::new(301).with("Location", format!("{}{path}/", site.mount)).with("Cache-Control", "no-store");
        }
        fp.push("index.html");
    }
    let (Ok(bytes), Ok(meta)) = (fs::read(&fp), fs::metadata(&fp)) else {
        return Response::text(404, "no such fold or file");
    };
    let page = content_type(&fp).starts_with("text/html");
    let class = if site.no_store.iter().any(|p| rel[1..].starts_with(p.as_str())) {
        Class::NoStore
    } else if rel.contains("@sha256:") {
        Class::Immutable
    } else if page {
        Class::Page
    } else if fp.extension().is_some_and(|e| e == "json") {
        Class::Data
    } else {
        Class::Asset
    };
    let hash = Sha256::digest(&bytes);
    let hex: String = hash.iter().map(|b| format!("{b:02x}")).collect();
    let etag = format!("\"sha-256:{hex}\"");
    let asof: DateTime<Utc> = meta.modified().unwrap_or(SystemTime::UNIX_EPOCH).into();
    let mut r = Response::new(200).with("Content-Type", content_type(&fp)).with("Cache-Control", class.cache_control());
    if class != Class::NoStore {
        r = r.with("ETag", etag.clone()).with("Last-Modified", http_date(asof));
    }
    r = r
        .with("Content-Digest", format!("sha-256=:{}:", base64(&hash)))
        .with("Server-Timing", format!("digest;desc=\"sha-256:{hex}\", asof;desc=\"{}\"", iso(asof)));
    let fresh = class != Class::NoStore && req.header("If-None-Match").is_some_and(|v| matches(v, &etag));
    if fresh {
        r.status = 304;
        r.headers.retain(|(k, _)| k != "Content-Type" && k != "Content-Digest");
    } else {
        // a HEAD's too, so its Content-Length is the GET's: only the writer leaves it out
        r.body = bytes;
    }
    r
}

/// Serve `site` on `listener` for ever.
pub fn run(listener: TcpListener, site: Site) -> std::io::Result<()> {
    crate::http::run(listener, Arc::new(limit), None, Arc::new(move |req: &Request, _| (respond(&site, req), None)))
}
