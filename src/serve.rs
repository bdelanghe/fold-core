//! `fold serve` (brief items 23-24, split by item 27): the fold viewer and its data over HTTP. GET and HEAD of static files, with
//! the headers darkhttpd cannot send (ETag, Cache-Control, Server-Timing). One write, and only when the server is told whom it acts
//! for ([`Acting`]): an act's form (src/ax.rs `act_form`) posts to `act/<id>`, and an ack, a defer or a decline moves that chitin
//! message's state for that one person ([`act`], S2: allie-s2-fold-act-write §2-6, Bobby's boundary 42184457). Every check runs
//! before anything is done: the peer is loopback, the tailscale serve identity is that person, the request came through serve,
//! it is same-origin, and the form's token (src/act.rs) is this server's, current, and for the act as it stands. The act itself
//! is read from the site's act map (`_acts.json`, page.rs), never from the form. Without [`Acting`], such a POST is read only to
//! be dropped and answered by a page saying acting from here isn't on ([`not_yet`]). It names no `fold sink` (sink.rs): the sink
//! is kept but not deployed. Signing goes through neither: the passkey stays on chitin's `/approve`. [`answer`] is pure (a
//! request, a directory and the time in; a response and its log line out), so tests/serve.rs holds every class and every refusal
//! as goldens without a socket; [`run`] is the loopback loop around it (http.rs, src/bin/fold.rs).
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
//! - **Paths:** percent-decoded, as darkhttpd does; a `.` or `..` segment, a NUL or a backslash is refused (400). No listings. The
//!   act map (`_acts.json`, page.rs) is the server's own and never served.
//! - **A page with an act:** served with each posting form's token filled (for the person this server acts for, when it is they
//!   who ask) and its status line said from `?did=`, so its ETag is the filled bytes' and it is `private, no-cache`.

use std::fs;
use std::net::TcpListener;
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::time::{Instant, SystemTime};

use chrono::{DateTime, SecondsFormat, Utc};
use serde_json::{json, Value};
use sha2::{Digest, Sha256};

use crate::act::{self, Actor, Bad, Done};
use crate::html::text;
use crate::{ax, page};

pub use crate::http::{Request, Response};

/// One directory served under one mount (`/fold`, `/fold-data`): tailscale serve strips the mount, so requests arrive without it,
/// and only the URLs this server hands out (a redirect, the report endpoint) need it.
pub struct Site {
    pub root: PathBuf,
    pub mount: String,
    /// Paths under the mount that are live state and never cached (`sign/`, `pending/`): a prefix of the decoded path.
    pub no_store: Vec<String>,
    /// Whom an act's POST acts for, if anyone: without it, the server only reads.
    pub acting: Option<Acting>,
}

/// The one person this server may act for, and how (`fold serve --act-as --act-host --act-key --chitin`).
pub struct Acting {
    /// Their tailnet login, as tailscale serve's `Tailscale-User-Login` carries it (never hard-coded here).
    pub login: String,
    /// The host the pages are served at (`https://<host>` is the one origin a form may post from).
    pub host: String,
    /// The token key (src/act.rs `key`).
    pub key: Vec<u8>,
    /// What moves the chitin message's state once every check holds.
    pub actor: Box<dyn Actor>,
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

/// The most an act's form may send (its token, cap, call and args as fields). Every other body is refused unread.
pub const ACT_BODY: usize = 4096;

/// How many body bytes a request may send: none, but for a POST to an act.
pub fn limit(method: &str, target: &str) -> usize {
    if method == "POST" && act(target).is_some() {
        ACT_BODY
    } else {
        0
    }
}

/// An act's target (`/act/<id>/…/<key>`, src/ax.rs `act_id`) as the path of the fold it is on and the act's own key, each
/// segment split before it is percent-decoded (an id holding `/` is `%2F` within one segment).
fn act(target: &str) -> Option<(Vec<String>, String)> {
    let path = target.split(['?', '#']).next()?.strip_prefix("/act/")?;
    let mut segs: Vec<String> = path.split('/').map(|s| decode(s).filter(|s| !s.is_empty())).collect::<Option<_>>()?;
    let key = segs.pop()?;
    Some((segs, key))
}

/// One request's answer, now.
pub fn respond(site: &Site, req: &Request) -> Response {
    answer(site, req, DateTime::<Utc>::from(SystemTime::now()).timestamp()).0
}

/// One request's answer at `now` (unix seconds), and the line it leaves in the act log, if it reached the act route.
pub fn answer(site: &Site, req: &Request, now: i64) -> (Response, Option<Value>) {
    let path = req.target.split(['?', '#']).next().unwrap_or("");
    let to_act = path.starts_with("/act/");
    match (req.method.as_str(), &site.acting) {
        ("POST", Some(a)) if to_act => post(site, a, req, now),
        (_, Some(_)) if to_act => (Response::text(405, "an act is posted: POST").with("Allow", "POST"), None),
        ("GET" | "HEAD", _) => (file(site, req, path, now), None),
        ("POST", None) => match act(&req.target) {
            Some((fold, _)) => (not_yet(site, &fold), None),
            None => (Response::text(405, "read-only: GET or HEAD").with("Allow", "GET, HEAD"), None),
        },
        _ => (Response::text(405, "read-only: GET or HEAD").with("Allow", "GET, HEAD"), None),
    }
}

/// The site's act map (page.rs `acts`), as it stands on disk now; none when it cannot be read.
fn acts(site: &Site) -> serde_json::Map<String, Value> {
    fs::read(site.root.join(page::ACTS)).ok().and_then(|b| serde_json::from_slice(&b).ok()).unwrap_or_default()
}

/// The empty token field slice 1's forms carry, as html.rs writes it.
fn blank_token() -> String {
    format!("<input type=\"hidden\" name=\"{}\" value=\"\">", ax::TOKEN_FIELD)
}

/// A fold's empty status line, as html.rs writes it.
fn blank_status() -> String {
    format!("<span id=\"{}\" class=\"result\" role=\"status\"></span>", ax::STATUS_ID)
}

/// A page with its acts' tokens filled for `login` (each bound to the act as the map holds it now) and its status line said.
/// A form whose act the map lacks, or whose cap this server never runs, keeps its empty token.
fn fill(page: &str, site: &Site, a: &Acting, login: Option<&str>, now: i64, status: Option<&str>) -> String {
    let map = acts(site);
    let lead = format!("action=\"{}/act/", site.mount);
    let (blank, mut out, mut rest) = (blank_token(), String::new(), page);
    while let Some(i) = rest.find("<form ") {
        let (head, tail) = rest.split_at(i);
        out.push_str(head);
        let end = tail.find("</form>").map_or(tail.len(), |e| e + "</form>".len());
        let (form, after) = tail.split_at(end);
        let open = &form[..form.find('>').unwrap_or(form.len())];
        let id = open.find(&lead).map(|j| &open[j + lead.len()..]).and_then(|x| x.split('"').next());
        let token = match (id, login) {
            (Some(id), Some(l)) => map
                .get(id)
                .filter(|e| e["cap"].as_str().and_then(act::state_of).is_some())
                .and_then(|e| e["fixity"].as_str())
                .map(|fx| act::mint(&a.key, id, fx, l, now)),
            _ => None,
        };
        match token {
            Some(t) => {
                out.push_str(&form.replacen(&blank, &format!("<input type=\"hidden\" name=\"{}\" value=\"{t}\">", ax::TOKEN_FIELD), 1))
            }
            None => out.push_str(form),
        }
        rest = after;
    }
    out.push_str(rest);
    match status {
        Some(said) => out.replacen(
            &blank_status(),
            &format!("<span id=\"{}\" class=\"result\" role=\"status\">{}</span>", ax::STATUS_ID, text(said)),
            1,
        ),
        None => out,
    }
}

/// `?did=<cap>&as=<ok|already>`'s sentence, from the fixed table (src/act.rs `said`): the query only picks a row.
fn did(target: &str) -> Option<&'static str> {
    let q = target.split_once('?')?.1.split('#').next()?;
    let get = |k: &str| q.split('&').find_map(|p| p.strip_prefix(k).and_then(|v| v.strip_prefix('=')));
    act::said(get("did")?, get("as")?)
}

/// The fold page an act is on, re-rendered at the act's URL with fresh tokens and why nothing was done (a stale token: 403; a
/// changed fold: 409), or, when the fold has gone, the page of a fold that is not there, with its way back.
fn again(site: &Site, a: &Acting, path: &[String], login: &str, now: i64, status: u16, said: &str) -> Response {
    let base = format!("{}/", site.mount);
    let body = page::file(path, "folds")
        .ok()
        .and_then(|f| fs::read_to_string(site.root.join(f)).ok())
        .map(|p| fill(&p, site, a, Some(login), now, Some(said)))
        .or_else(|| page::missing(path, &base).ok());
    let mut r = Response::new(status).with("Content-Type", "text/html; charset=utf-8").with("Cache-Control", "no-store");
    r.body = body.unwrap_or_default().into_bytes();
    r
}

/// `POST act/<id>` for the person this server acts for: every check, then at most one ack state move (S2 brief §2).
fn post(site: &Site, a: &Acting, req: &Request, now: i64) -> (Response, Option<Value>) {
    let login = req.header("Tailscale-User-Login");
    let line = |cap: Option<&str>, act: Option<&str>, item: Option<&str>, msg: Option<&str>, result: String| json!({ "ts": now, "login": login, "peer": req.header("X-Forwarded-For"), "cap": cap, "act": act, "item": item, "msg_id": msg, "result": result });
    let no = |status: u16, why: &str| (Response::text(status, why), Some(line(None, None, None, None, format!("refused:{status}"))));
    // Who is asking, and through what: all before the form is read.
    if !req.peer.is_some_and(|p| p.is_loopback()) {
        return no(403, "not from the tailnet front");
    }
    let Some(login) = login else { return no(403, "not who this server acts for") };
    if login.starts_with("=?") || !login.is_ascii() {
        return no(403, "unexpected login encoding");
    }
    if login != a.login {
        return no(403, "not who this server acts for");
    }
    if req.header("X-Forwarded-Proto") != Some("https") || req.header("X-Forwarded-Host") != Some(a.host.as_str()) {
        return no(403, "not through tailscale serve");
    }
    let origin = format!("https://{}", a.host);
    let same = match (req.header("Sec-Fetch-Site"), req.header("Origin")) {
        (Some("same-origin"), o) => o.is_none_or(|o| o == origin),
        (None, Some(o)) => o == origin,
        _ => false,
    };
    if !same {
        return no(403, "cross-site");
    }
    // The form.
    if req.media_type() != Some("application/x-www-form-urlencoded") {
        return no(400, "not a form: send application/x-www-form-urlencoded");
    }
    if req.body.is_empty() {
        return no(400, "no form");
    }
    let Some(fields) = act::form(&req.body) else { return no(400, "not a form body") };
    let field = |k: &str| fields.iter().find(|(n, _)| n == k).map(|(_, v)| v.as_str());
    let Some((path, key)) = act(&req.target) else { return no(400, "not an act id") };
    let Some(token) = field(ax::TOKEN_FIELD) else { return no(400, "no token") };
    // The act, as the site's act map holds it now.
    let id = ax::act_id(&path, &key);
    let map = acts(site);
    let Some(entry) = map.get(&id) else {
        // A token this server minted for this act means the act was there when the page was served: it has gone (409).
        return match act::check(&a.key, token, &id, login, now) {
            Ok(_) | Err(Bad::Expired) => {
                let r = again(site, a, &path, login, now, 409, "This changed since you opened it. Read it again before acting.");
                (r, Some(line(None, Some(&id), None, None, "changed".into())))
            }
            _ => (Response::text(404, "no such act in this fold"), Some(line(None, Some(&id), None, None, "refused:404".into()))),
        };
    };
    let s = |k: &str| entry[k].as_str();
    let (cap, item) = (s("cap").unwrap_or(""), s("item"));
    let msg = entry["args"]["msg_id"].as_str();
    let rec = |result: String| Some(line(Some(cap), Some(&id), item, msg, result));
    let Some((state, why)) = act::state_of(cap) else {
        return (Response::text(422, "this act is not one this server may run"), rec("refused:422".into()));
    };
    if field("cap").is_some_and(|c| c != cap) {
        return (Response::text(400, "the form does not match the fold"), rec("refused:400".into()));
    }
    // The token: this server's, for this act and person, current, and for the act as it stands.
    let stale = || {
        let r = again(site, a, &path, login, now, 403, "That page had gone stale, so here it is again. Nothing was done.");
        (r, rec("stale".into()))
    };
    if token.is_empty() {
        // a form no server filled (the viewer's own page renders acts from the data): the same as a stale one
        return stale();
    }
    match act::check(&a.key, token, &id, login, now) {
        Err(Bad::Malformed) => return (Response::text(400, "not a token"), rec("refused:400".into())),
        Err(Bad::Forged) => return (Response::text(403, "the token is not this server's"), rec("refused:403".into())),
        Err(Bad::Expired) => return stale(),
        Ok(fx) if Some(fx.as_str()) != s("fixity") => {
            let r = again(site, a, &path, login, now, 409, "This changed since you opened it. Read it again before acting.");
            return (r, rec("changed".into()));
        }
        Ok(_) => {}
    }
    let (Some(msg_id), Some(from)) = (msg, entry["args"]["from"].as_str()) else {
        return (Response::text(422, "this act names no message to answer"), rec("refused:422".into()));
    };
    let how = match a.actor.ack(msg_id, state, from, why) {
        Ok(Done::Ok) => "ok",
        Ok(Done::Already) => "already",
        Err(e) => {
            let e: String = e.chars().filter(|c| !c.is_control()).take(200).collect();
            return (Response::text(502, &format!("chitin did not take it: {e}")), rec(format!("chitin:{e}")));
        }
    };
    let to = format!("{}{}?did={cap}&as={how}", site.mount, page::href("/", &path, "folds"));
    (Response::new(303).with("Location", to).with("Cache-Control", "no-store"), rec(how.into()))
}

/// An act's form submitted with no script to take it (BRIEF-basis-forms-push-signing-2026-10-31 §1, render only): this server
/// only reads, so it does nothing and says so on a page of its own, with the way back to the fold the act is on and to the root;
/// never a bare 405. The form's fields were read only to be dropped.
fn not_yet(site: &Site, fold: &[String]) -> Response {
    let base = format!("{}/", site.mount);
    let link = |name: String, to: &[String]| json!({ "role": "listitem", "cls": "prop", "children": [{ "role": "link", "name": name, "href": page::href(&base, to, "folds"), "local": true }] });
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

fn file(site: &Site, req: &Request, path: &str, now: i64) -> Response {
    let Some(rel) = decode(path).filter(|p| p.starts_with('/')) else {
        return Response::text(400, "not a path");
    };
    let segs: Vec<&str> = rel[1..].split('/').collect();
    if segs.iter().any(|s| *s == "." || *s == ".." || s.contains('\0') || s.contains('\\')) {
        return Response::text(400, "no . or .. segments");
    }
    if segs == [page::ACTS] {
        return Response::text(404, "no such fold or file");
    }
    let mut fp = site.root.clone();
    fp.extend(segs.iter().filter(|s| !s.is_empty()));
    if fp.is_dir() {
        if !path.ends_with('/') {
            return Response::new(301).with("Location", format!("{}{path}/", site.mount)).with("Cache-Control", "no-store");
        }
        fp.push("index.html");
    }
    let (Ok(mut bytes), Ok(meta)) = (fs::read(&fp), fs::metadata(&fp)) else {
        return Response::text(404, "no such fold or file");
    };
    let page = content_type(&fp).starts_with("text/html");
    // A page with an act: its tokens filled for whoever asks, if it is the person this server acts for, and its status said.
    let mut private = false;
    if let (Some(a), true) = (&site.acting, page) {
        let lead = format!("action=\"{}/act/", site.mount);
        if let Some(p) = std::str::from_utf8(&bytes).ok().filter(|p| p.contains(&lead)) {
            let login = req.header("Tailscale-User-Login").filter(|l| *l == a.login);
            bytes = fill(p, site, a, login, now, did(&req.target)).into_bytes();
            private = true;
        }
    }
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
    let cache = if private && class == Class::Page { "private, no-cache" } else { class.cache_control() };
    let mut r = Response::new(200).with("Content-Type", content_type(&fp)).with("Cache-Control", cache);
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
/// Serve `site` on `listener` for ever; with `log`, each act's line (with its `ms`) appended to it (S2 brief §5).
pub fn run(listener: TcpListener, site: Site, log: Option<PathBuf>) -> std::io::Result<()> {
    crate::http::run(
        listener,
        Arc::new(limit),
        log,
        Arc::new(move |req: &Request, t: DateTime<Utc>| {
            let t0 = Instant::now();
            let (r, mut rec) = answer(&site, req, t.timestamp());
            if let Some(Value::Object(m)) = rec.as_mut() {
                m.insert("ts".into(), iso(t).into());
                m.insert("ms".into(), json!((t0.elapsed().as_secs_f64() * 10000.0).round() / 10.0));
            }
            (r, rec)
        }),
    )
}
