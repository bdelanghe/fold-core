//! `fold serve` (brief items 23-24, split by item 27): the fold viewer and its data over HTTP. GET and HEAD of static files, with
//! the headers darkhttpd cannot send (ETag, Cache-Control, Server-Timing). One write, and only when the server is told whom it acts
//! for ([`Acting`]): an act's form (src/ax.rs `act_form`) posts to `act/<id>`, and an ack, a defer or a decline moves that chitin
//! message's state for that one person ([`act`], S2: allie-s2-fold-act-write §2-6, Bobby's boundary 42184457), or a say sends its
//! one typed line (Bobby 07c5f8dc widened the boundary for it; the line is checked against the act map's `input`). Every check runs
//! before anything is done: the peer is loopback, the tailscale serve identity is that person, the request came through serve,
//! it is same-origin, and the form's token (src/act.rs) is this server's, current, and for the act as it stands. The act itself
//! is read from the site's act map (`_acts.json`, page.rs), never from the form. Without [`Acting`], such a POST is read only to
//! be dropped and answered by a page saying acting from here isn't on ([`not_yet`]). It names no `fold sink` (sink.rs): the sink
//! is kept but not deployed. With `--needs-as <title>` it also reads Needs you live ([`NEEDS`]: that title's waiting intents, from
//! chitin, on each request), under the same checks. With `--notify-as <seat>` too, it sends Needs you's notifications (BRIEF
//! addendum 2) through chitin, which keeps the subscriptions and VAPID key and sends the Web Push: [`PUSH`]'s three routes hand a
//! device's subscription to `chitin push`, and [`watch`] emits one `chitin notify` Needs-you record per new waiting intent. A verdict never comes here, approve or decline: the viewer runs the passkey in
//! the page against chitin-approve's own `/approve/v1/*` (viewer/sign.js), which binds it, and the seats verify it. [`answer`] is pure (a
//! request, a directory and the time in; a response and its log line out), so tests/serve.rs holds every class and every refusal
//! as goldens without a socket; [`run`] is the loopback loop around it (http.rs, src/bin/fold.rs).
//!
//! - **Cache classes (item 23):** a path holding `@sha256:` (a locator, not an identity) is immutable; a page (an `.html` file, or
//!   a directory's `index.html`) and the viewer's other files are `no-cache`, always revalidated; a data document (`.json`) is
//!   `max-age=60, stale-while-revalidate=240, stale-if-error=86400`; a `--no-store` path (the Sign fold, pending approvals,
//!   presence) is `no-store` with no validator, and so never bfcache-eligible. `sw.js` is `no-cache`. A fold's live node
//!   (`nodes/<id>.json`) and the nodes' index (`nodes/`, [`nodes_index`]) are `no-cache` too: the viewer revalidates each fold's
//!   own node with `If-None-Match`, so an unchanged fold costs one 304 and a changed one is read at once.
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

use crate::act::{self, Actor, Bad, Done, Push};
use crate::html::text;
use crate::id::{haecceity, segment_of};
use crate::report::{self, Reporting};
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
    /// Whom `POST report-token` mints Clax reporter tokens for (report.rs), if anyone: without it, the route says reporting is off.
    pub reporting: Option<Reporting>,
}

/// The one person this server may act for, and how (`fold serve --act-as --act-host --act-key --chitin`).
pub struct Acting {
    /// Their tailnet login, as tailscale serve's `Tailscale-User-Login` carries it (never hard-coded here).
    pub login: String,
    /// The host the pages are served at (`https://<host>` is the one origin a form may post from).
    pub host: String,
    /// The token key (src/act.rs `key`).
    pub key: Vec<u8>,
    /// What moves the chitin message's state, or says the typed line, once every check holds.
    pub actor: Box<dyn Actor>,
    /// The tokens a say has spent: one form sends once (a double tap, a replay or a back-button resubmit is "Already sent."). Kept
    /// for the server's life; a token outlives a restart only by its [`act::TTL`].
    pub spent: std::sync::Mutex<std::collections::HashSet<String>>,
    /// The chitin title whose waiting intents `needs.json` lists (`--needs-as`), if any: without it, the route says Needs you
    /// isn't read live here.
    pub needs_as: Option<String>,
    /// The chitin seat Needs-you records are published as (`--notify-as`, a producer chitin's notify-sink pushes for), if any:
    /// without it, no notification is sent and the push routes say so.
    pub notify_as: Option<String>,
}

/// How a resource may be cached (item 23's table), by its path.
#[derive(Clone, Copy, PartialEq, Debug)]
enum Class {
    Immutable,
    Page,
    Data,
    NoStore,
    Asset,
    /// A fold's live node (`nodes/<id>.json`, and their index `nodes/`): revalidated every time, as a page is, so a 304 is the
    /// whole cost of asking and a changed node is never a minute late (BRIEF-stint-2026-11-03-basis-live-folds Part A).
    Node,
}

impl Class {
    fn cache_control(self) -> &'static str {
        match self {
            Class::Immutable => "public, max-age=31536000, immutable",
            Class::Page | Class::Asset | Class::Node => "no-cache",
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
    } else if method == "POST" && target.split(['?', '#']).next() == Some(REPORT_TOKEN) {
        REPORT_BODY
    } else if method == "POST" && target.starts_with(COMMENT) {
        COMMENT_BODY
    } else if method == "POST" && target.split(['?', '#']).next().is_some_and(|p| p.starts_with(PUSH)) {
        PUSH_BODY
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
    if path == REPORT_TOKEN {
        return reporter_token(site, req, now);
    }
    if path == OVERLAY {
        return (overlay(site, req), None);
    }
    if path.starts_with(COMMENT) {
        return comment(site, req, now);
    }
    if path == NEEDS {
        return (needs(site, req), None);
    }
    if path.starts_with(PUSH) {
        return push(site, req, now);
    }
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
/// A form whose act the map lacks, or whose cap and call this server never runs, keeps its empty token.
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
                .filter(|e| act::run_of(e).is_some())
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

/// `?did=<call>&as=<ok|already>`'s sentence, from the fixed table (src/act.rs `said`): the query only picks a row.
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

/// Who is asking, and through what, before anything is read: loopback (tailscale serve's front), the one `login` this server is
/// for, through serve to `host`, and same-origin. The login on success; the refusal (always 403) and why, otherwise.
fn front<'r>(req: &'r Request, login: &str, host: &str) -> Result<&'r str, &'static str> {
    if !req.peer.is_some_and(|p| p.is_loopback()) {
        return Err("not from the tailnet front");
    }
    let Some(who) = req.header("Tailscale-User-Login") else { return Err("not who this server acts for") };
    if who.starts_with("=?") || !who.is_ascii() {
        return Err("unexpected login encoding");
    }
    if who != login {
        return Err("not who this server acts for");
    }
    if req.header("X-Forwarded-Proto") != Some("https") || req.header("X-Forwarded-Host") != Some(host) {
        return Err("not through tailscale serve");
    }
    let origin = format!("https://{host}");
    let same = match (req.header("Sec-Fetch-Site"), req.header("Origin")) {
        (Some("same-origin"), o) => o.is_none_or(|o| o == origin),
        (None, Some(o)) => o == origin,
        _ => false,
    };
    if !same {
        return Err("cross-site");
    }
    Ok(who)
}

/// The reporter-token route's path (report.rs): what the Clax overlay on a fold page posts to, under the mount.
pub const REPORT_TOKEN: &str = "/report-token";

/// The overlay's path under the mount: the Clax instance's `/_clax/overlay.js`, served from this origin (report.rs).
pub const OVERLAY: &str = "/clax/overlay.js";

/// The most a report-token request's body may be: `{"public_key": …}`.
pub const REPORT_BODY: usize = 1024;

fn json_answer(status: u16, v: &Value) -> Response {
    let mut r = Response::new(status).with("Content-Type", "application/json").with("Cache-Control", "no-store");
    r.body = (v.to_string() + "\n").into_bytes();
    r
}

/// `POST report-token`, for the one login this server reports for, same-origin only (report.rs): with no body, what the page mounts
/// the overlay with, `{instance, queue, owner}`; with `{"public_key"}` (JSON), a reporter token for that key minted by the Clax
/// instance, `{token, expires_at}`. Never stored; each answer leaves a log line naming no token and no key.
fn reporter_token(site: &Site, req: &Request, now: i64) -> (Response, Option<Value>) {
    let Some(r) = &site.reporting else {
        return (Response::text(404, "reporting to Clax isn't on here"), None);
    };
    if req.method != "POST" {
        return (Response::text(405, "a reporter token is posted for: POST").with("Allow", "POST"), None);
    }
    let line = |result: String| Some(json!({ "ts": now, "login": req.header("Tailscale-User-Login"), "peer": req.header("X-Forwarded-For"), "report": r.queue, "result": result }));
    let who = match front(req, &r.login, &r.host) {
        Ok(who) => who,
        Err(why) => return (Response::text(403, why), line("refused:403".into())),
    };
    if req.body.is_empty() {
        let v = json!({ "instance": r.clax, "queue": r.queue, "owner": r.owner });
        return (json_answer(200, &v), line("mount".into()));
    }
    // JSON only: a page's fetch can send it, a cross-site form cannot.
    if req.media_type() != Some("application/json") {
        return (Response::text(415, "send the key as application/json"), line("refused:415".into()));
    }
    let key = serde_json::from_slice::<Value>(&req.body).ok().and_then(|v| v["public_key"].as_str().map(str::to_string));
    let Some(key) = key.filter(|k| report::public_key_ok(k)) else {
        return (Response::text(400, "no public_key, or not one"), line("refused:400".into()));
    };
    // The token is for this page's origin (the one front() let through) and for the one login, as Clax's reporter.
    match r.minter.mint(&key, &format!("https://{}", r.host), who) {
        Ok(m) => (json_answer(200, &json!({ "token": m.token, "expires_at": m.expires_at })), line("minted".into())),
        Err(e) => (Response::text(502, &format!("clax did not mint: {e}")), line("clax:failed".into())),
    }
}

/// The comment pages' prefix under the mount: `comment/<fold path>/`, a fold's "Comment in Clax" with JS off.
pub const COMMENT: &str = "/comment/";

/// The most a comment form's body may be, and the most characters a comment may have.
pub const COMMENT_BODY: usize = 16 * 1024;
pub const COMMENT_MAX: usize = 4000;

/// A comment page's fold path (`/comment/a/b%2Fc/` is `[a, b/c]`, `/comment/` the root), each segment decoded on its own.
fn comment_path(target: &str) -> Option<Vec<String>> {
    let p = target.split(['?', '#']).next()?.strip_prefix(COMMENT)?;
    let p = p.strip_suffix('/').unwrap_or(p);
    if p.is_empty() {
        return Some(vec![]);
    }
    // page::file refuses an id no file can hold (`.`, `..`, `/`, NUL), so such a path has no page and is a 404.
    p.split('/').map(|s| decode(s).filter(|s| !s.is_empty())).collect()
}

/// A fold path as it stands in a URL and a haecceity: each id one segment (id.rs `segment_of`).
fn under(path: &[String]) -> String {
    path.iter().map(|p| segment_of(p)).collect::<Vec<_>>().join("/")
}

/// A comment page's own path under the mount.
fn comment_href(mount: &str, path: &[String]) -> String {
    let u = under(path);
    if u.is_empty() {
        format!("{mount}{COMMENT}")
    } else {
        format!("{mount}{COMMENT}{u}/")
    }
}

/// HTML's five escapes undone: a fold page's `<h1>` text as the words it shows.
fn unescape(s: &str) -> String {
    s.replace("&lt;", "<").replace("&gt;", ">").replace("&quot;", "\"").replace("&nbsp;", "\u{a0}").replace("&amp;", "&")
}

/// The fold's title, from its page's `<h1>`; None when it has no page here.
fn fold_title(site: &Site, path: &[String]) -> Option<String> {
    let html = fs::read_to_string(site.root.join(page::file(path, "folds").ok()?)).ok()?;
    let h = &html[html.find("<h1")?..];
    let h = &h[h.find('>')? + 1..];
    Some(unescape(&h[..h.find("</h1>")?]))
}

/// A fold's comment page: what it is for, the form (a box and its button), and the way back; `said` above the form when an earlier
/// post was refused. The fold's own page shape (a heading and one list), so lone reads it as it reads a fold.
fn comment_page(site: &Site, r: &Reporting, path: &[String], title: &str, said: Option<&str>, status: u16) -> Response {
    let base = format!("{}/", site.mount);
    let name = format!("Comment on {title}");
    let prop = |t: String| json!({ "role": "listitem", "cls": "prop", "children": [{ "role": "text", "name": t }] });
    let mut rows = vec![prop(format!("What you write goes to {} through Clax, about this fold. Nothing else on the page is sent.", r.owner))];
    if let Some(s) = said {
        rows.push(json!({ "role": "listitem", "cls": "prop", "children": [{ "role": "status", "name": s }] }));
    }
    rows.push(json!({ "role": "listitem", "cls": "act", "children": [{
        "role": "form", "name": "Comment", "method": "post", "action": comment_href(&site.mount, path), "fields": [],
        "children": [
            { "role": "textbox", "name": "Your comment", "field": "body", "maxlength": COMMENT_MAX },
            { "role": "button", "name": "Send to Clax" },
        ],
    }] }));
    rows.push(json!({ "role": "listitem", "cls": "prop", "children": [
        { "role": "link", "name": format!("Back to {title}"), "href": page::href(&base, path, "folds"), "local": true },
    ] }));
    let tree = json!({ "role": "main", "children": [
        { "role": "heading", "name": name, "focus": true },
        { "role": "list", "name": name, "cls": "rows", "children": rows },
    ] });
    match page::page(&tree, &base) {
        Ok(h) => {
            let mut res = Response::new(status).with("Content-Type", "text/html; charset=utf-8").with("Cache-Control", "no-store");
            res.body = h.into_bytes();
            res
        }
        Err(e) => Response::text(500, &e),
    }
}

/// `comment/<fold path>/`: GET, the fold's comment page; POST, the comment, for the one login, same-origin (as a report token),
/// filed as a Clax report about the fold (report.rs `Filer`), then 303 back to the fold. With no filer, a 503 page says comments
/// from here are not on and nothing was sent. The log line names the fold, never the words.
fn comment(site: &Site, req: &Request, now: i64) -> (Response, Option<Value>) {
    let Some(r) = &site.reporting else {
        return (Response::text(404, "reporting to Clax isn't on here"), None);
    };
    let Some(path) = comment_path(&req.target) else { return (Response::text(400, "not a fold path"), None) };
    let Some(title) = fold_title(site, &path) else {
        let base = format!("{}/", site.mount);
        let mut res = Response::new(404).with("Content-Type", "text/html; charset=utf-8").with("Cache-Control", "no-store");
        res.body = page::missing(&path, &base).unwrap_or_default().into_bytes();
        return (res, None);
    };
    match req.method.as_str() {
        "GET" | "HEAD" => return (comment_page(site, r, &path, &title, None, 200), None),
        "POST" => {}
        _ => return (Response::text(405, "a comment is posted: GET or POST").with("Allow", "GET, POST"), None),
    }
    let about = haecceity(&r.source, &under(&path));
    let line = |result: &str| Some(json!({ "ts": now, "login": req.header("Tailscale-User-Login"), "comment": about, "result": result }));
    if let Err(why) = front(req, &r.login, &r.host) {
        return (Response::text(403, why), line("refused:403"));
    }
    if req.media_type() != Some("application/x-www-form-urlencoded") {
        return (Response::text(415, "send the comment as a form"), line("refused:415"));
    }
    let body = act::form(&req.body).and_then(|f| f.into_iter().find(|(k, _)| k == "body").map(|(_, v)| v.trim().to_string()));
    let body = match body {
        Some(b) if !b.is_empty() && b.chars().count() <= COMMENT_MAX => b,
        Some(b) if !b.is_empty() => {
            let said = format!("That is longer than {COMMENT_MAX} characters, so nothing was sent.");
            return (comment_page(site, r, &path, &title, Some(&said), 400), line("refused:400"));
        }
        _ => return (comment_page(site, r, &path, &title, Some("Write your comment first: nothing was sent."), 400), line("refused:400")),
    };
    let Some(filer) = &r.filer else {
        let said = "Comments from here are not on yet, so nothing was sent. With JavaScript on, the Clax button on the fold works.";
        return (comment_page(site, r, &path, &title, Some(said), 503), line("off"));
    };
    let back = page::href(&format!("{}/", site.mount), &path, "folds");
    let c = report::Comment { external_id: about.clone(), url: format!("https://{}{back}", r.host), title: title.clone(), body };
    match filer.file(&c) {
        Ok(()) => (Response::new(303).with("Location", back).with("Cache-Control", "no-store"), line("filed")),
        Err(e) => {
            let said = format!("Clax did not take it, so nothing was sent: {}", e.chars().filter(|c| !c.is_control()).take(200).collect::<String>());
            (comment_page(site, r, &path, &title, Some(&said), 502), line("clax:failed"))
        }
    }
}

/// Needs you, live (src/needs.rs): what waits for the title's signature, read from chitin on each request, under the mount.
pub const NEEDS: &str = "/needs.json";

/// Whom the live Needs you reads for, if anyone: the acting login and the `--needs-as` title.
fn needs_for(site: &Site) -> Option<(&Acting, &str)> {
    let a = site.acting.as_ref()?;
    Some((a, a.needs_as.as_deref()?))
}

/// `GET needs.json`, for the one login this server acts for, same-origin (the act's checks, before chitin is run): `{as, waiting}`,
/// the title's waiting intents (src/needs.rs `waiting`) as chitin lists them now. Never stored, no validator: it is live state.
fn needs(site: &Site, req: &Request) -> Response {
    let Some((a, title)) = needs_for(site) else {
        return Response::text(404, "Needs you isn't read live here");
    };
    if req.method != "GET" && req.method != "HEAD" {
        return Response::text(405, "read-only: GET or HEAD").with("Allow", "GET, HEAD");
    }
    if let Err(why) = front(req, &a.login, &a.host) {
        return Response::text(403, why).with("Cache-Control", "no-store");
    }
    match a.actor.pending(title) {
        Ok(p) => json_answer(200, &crate::needs::answer(title, &crate::needs::waiting(&p, title), a.notify_as.is_some())),
        Err(e) => json_answer(502, &json!({ "error": format!("chitin did not list them: {}", e.chars().filter(|c| !c.is_control()).take(200).collect::<String>()) })),
    }
}

/// Needs you's notifications, under the mount: `push/key` (GET: the VAPID public key a device subscribes with), `push/subscribe`
/// and `push/unsubscribe` (POST, JSON).
pub const PUSH: &str = "/push/";

/// The most a push route's body may be: one PushSubscription.
pub const PUSH_BODY: usize = 4096;

/// `push/<verb>` for the one login, same-origin, JSON only (a cross-site form cannot send it): the key, a device's subscription
/// handed to `chitin push subscribe` cut to `{endpoint, keys: {p256dh, auth}}` (chitin checks the rest), or one taken off. The
/// log line names the push service's host, never the endpoint's path or keys.
fn push(site: &Site, req: &Request, now: i64) -> (Response, Option<Value>) {
    let Some(a) = site.acting.as_ref().filter(|a| a.notify_as.is_some() && a.needs_as.is_some()) else {
        return (Response::text(404, "Needs you sends no notifications here"), None);
    };
    let verb = req.target.split(['?', '#']).next().and_then(|p| p.strip_prefix(PUSH)).unwrap_or("");
    let host = |e: &str| e.strip_prefix("https://").and_then(|r| r.split('/').next()).map(str::to_string);
    let line = |result: &str, endpoint: Option<&str>| Some(json!({ "ts": now, "login": req.header("Tailscale-User-Login"), "push": verb, "service": endpoint.and_then(host), "result": result }));
    let no = |status: u16, reason: &str, result: &str| (json_answer(status, &json!({ "ok": false, "reason": reason })), line(result, None));
    let want = if verb == "key" { "GET" } else { "POST" };
    if !matches!(verb, "key" | "subscribe" | "unsubscribe") {
        return (Response::text(404, "no such push route"), None);
    }
    if req.method != want && !(want == "GET" && req.method == "HEAD") {
        return (Response::text(405, &format!("push/{verb} is {want}")).with("Allow", want), None);
    }
    if let Err(why) = front(req, &a.login, &a.host) {
        return no(403, why, "refused:403");
    }
    let chitin = |op: Push, endpoint: Option<&str>| match a.actor.push(&op) {
        Ok(said) => (json_answer(200, &json!({ "ok": true, "said": said })), line("ok", endpoint)),
        Err(e) => {
            let e: String = e.chars().filter(|c| !c.is_control()).take(200).collect();
            (json_answer(502, &json!({ "ok": false, "reason": format!("chitin did not take it: {e}") })), line("chitin:failed", endpoint))
        }
    };
    if verb == "key" {
        return match a.actor.push(&Push::Key) {
            Ok(k) if !k.is_empty() && k.bytes().all(|b| b.is_ascii_alphanumeric() || b == b'-' || b == b'_') => (json_answer(200, &json!({ "key": k })), None),
            Ok(_) => (json_answer(502, &json!({ "error": "chitin's key is not base64url" })), None),
            Err(e) => (json_answer(503, &json!({ "error": format!("notifications are off: {}", e.chars().filter(|c| !c.is_control()).take(200).collect::<String>()) })), None),
        };
    }
    if req.media_type() != Some("application/json") {
        return no(415, "send it as application/json", "refused:415");
    }
    let body: Option<Value> = serde_json::from_slice(&req.body).ok();
    let s = |v: Option<&Value>| v.and_then(Value::as_str).map(str::to_string);
    let endpoint = s(body.as_ref().and_then(|b| b.get("endpoint"))).filter(|e| e.starts_with("https://") && e.len() <= 512 && !e.chars().any(char::is_control));
    let Some(endpoint) = endpoint else { return no(400, "no https endpoint", "refused:400") };
    if verb == "unsubscribe" {
        return chitin(Push::Unsubscribe(endpoint.clone()), Some(&endpoint));
    }
    let keys = body.as_ref().and_then(|b| b.get("keys"));
    let (Some(p256dh), Some(auth)) = (s(keys.and_then(|k| k.get("p256dh"))), s(keys.and_then(|k| k.get("auth")))) else {
        return no(400, "no keys.p256dh and keys.auth", "refused:400");
    };
    let sub = json!({ "endpoint": endpoint, "keys": { "p256dh": p256dh, "auth": auth } }).to_string();
    chitin(Push::Subscribe(sub), Some(&endpoint))
}

/// What [`watch`] keeps between looks: the intents notified, and the last result said for each look's failures, so a failure is
/// logged when it starts or changes, not every look.
#[derive(Default)]
pub struct Watched {
    pub seen: std::collections::HashSet<String>,
    said: std::collections::HashMap<String, String>,
}

/// One look at what waits (BRIEF addendum 2's sender): each waiting intent not yet notified gets one `chitin notify` Needs-you record
/// (src/needs.rs `notice`) and is remembered; one that has left is forgotten, so nothing is sent for an item that leaves. A record
/// chitin did not take is tried again at the next look. A log line is returned only for a send, or when a failure starts or
/// changes. They name the intent, never its text.
pub fn watch(site: &Site, w: &mut Watched, now: i64) -> Vec<Value> {
    let (Some((a, title)), Some(seat)) = (needs_for(site), site.acting.as_ref().and_then(|a| a.notify_as.as_deref())) else { return vec![] };
    let mut out = vec![];
    fn once(w: &mut Watched, out: &mut Vec<Value>, key: &str, line: Value) {
        let r = line["result"].as_str().unwrap_or("").to_string();
        if w.said.get(key) != Some(&r) {
            w.said.insert(key.to_string(), r);
            out.push(line);
        }
    }
    let waiting = match a.actor.pending(title) {
        Ok(p) => {
            w.said.remove("");
            crate::needs::waiting(&p, title)
        }
        Err(e) => {
            once(w, &mut out, "", json!({ "ts": now, "notify": null, "result": format!("chitin:pending {}", e.chars().filter(|c| !c.is_control()).take(120).collect::<String>()) }));
            return out;
        }
    };
    let ids: std::collections::HashSet<String> = waiting.iter().filter_map(|x| x["id"].as_str().map(str::to_string)).collect();
    w.seen.retain(|id| ids.contains(id));
    w.said.retain(|id, _| id.is_empty() || ids.contains(id));
    for x in &waiting {
        let Some(id) = x["id"].as_str().filter(|id| !w.seen.contains(*id)) else { continue };
        let Some((line, url, dedupe)) = crate::needs::notice(x, &a.host, &site.mount) else { continue };
        match a.actor.notify(seat, &line, &url, &dedupe) {
            Ok(()) => {
                w.seen.insert(id.to_string());
                w.said.remove(id);
                out.push(json!({ "ts": now, "notify": id, "as": seat, "result": "ok" }));
            }
            Err(e) => {
                let r = format!("chitin:{}", e.chars().filter(|c| !c.is_control()).take(120).collect::<String>());
                once(w, &mut out, id, json!({ "ts": now, "notify": id, "as": seat, "result": r }));
            }
        }
    }
    out
}

/// How often [`watch`] looks.
pub const WATCH_EVERY: std::time::Duration = std::time::Duration::from_secs(30);

/// `GET clax/overlay.js`: the Clax instance's overlay, fetched over loopback each time and served from this origin, so a page can
/// load it pinned by its SRI digest with no CORS on the instance. Revalidated always; the page's digest is what pins its bytes.
fn overlay(site: &Site, req: &Request) -> Response {
    let Some(r) = &site.reporting else {
        return Response::text(404, "reporting to Clax isn't on here");
    };
    if req.method != "GET" && req.method != "HEAD" {
        return Response::text(405, "read-only: GET or HEAD").with("Allow", "GET, HEAD");
    }
    match r.overlay() {
        Ok(b) => {
            let mut res = Response::new(200).with("Content-Type", "text/javascript; charset=utf-8").with("Cache-Control", "no-cache");
            res.body = b;
            res
        }
        Err(e) => Response::text(502, &format!("the Clax overlay could not be read: {e}")),
    }
}

/// `POST act/<id>` for the person this server acts for: every check, then at most one ack state move (S2 brief §2).
fn post(site: &Site, a: &Acting, req: &Request, now: i64) -> (Response, Option<Value>) {
    let login = req.header("Tailscale-User-Login");
    let line = |cc: Option<(&str, &str)>, act: Option<&str>, item: Option<&str>, msg: Option<&str>, result: String| json!({ "ts": now, "login": login, "peer": req.header("X-Forwarded-For"), "cap": cc.map(|c| c.0), "call": cc.map(|c| c.1), "act": act, "item": item, "msg_id": msg, "result": result });
    let no = |status: u16, why: &str| (Response::text(status, why), Some(line(None, None, None, None, format!("refused:{status}"))));
    // Who is asking, and through what: all before the form is read.
    let login = match front(req, &a.login, &a.host) {
        Ok(login) => login,
        Err(why) => return no(403, why),
    };
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
    let (cap, call, item) = (s("cap").unwrap_or(""), s("call").unwrap_or(""), s("item"));
    let msg = entry["args"]["msg_id"].as_str().or(entry["args"]["in_reply_to"].as_str());
    let rec = |result: String| Some(line(Some((cap, call)), Some(&id), item, msg, result));
    let Some(run) = act::run_of(entry) else {
        return (Response::text(422, "this act is not one this server may run"), rec("refused:422".into()));
    };
    if field("cap").is_some_and(|c| c != cap) || field("call").is_some_and(|c| c != call) {
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
    let (state, why) = match run {
        act::Run::Ack { state, why } => (state, why),
        act::Run::Say(input) => return say(site, a, &path, &id, entry, &input, &fields, token, login, now, rec),
    };
    let (Some(msg_id), Some(from)) = (msg, entry["args"]["from"].as_str()) else {
        return (Response::text(422, "this act names no message to answer"), rec("refused:422".into()));
    };
    let how = match a.actor.ack(msg_id, state, from, why, &id) {
        Ok(Done::Ok) => "ok",
        Ok(Done::Already) => "already",
        Err(e) => {
            let e: String = e.chars().filter(|c| !c.is_control()).take(200).collect();
            return (Response::text(502, &format!("chitin did not take it: {e}")), rec(format!("chitin:{e}")));
        }
    };
    let to = format!("{}{}?did={call}&as={how}", site.mount, page::href("/", &path, "folds"));
    (Response::new(303).with("Location", to).with("Cache-Control", "no-store"), rec(how.into()))
}

/// A say, its token good for the act as it stands: the one typed field, read from the form only as the act map's `input` names
/// it and checked against that (src/act.rs `typed`), then said once per token. The log keeps the text's length and digest, never
/// the text.
#[allow(clippy::too_many_arguments)]
fn say(
    site: &Site,
    a: &Acting,
    path: &[String],
    id: &str,
    entry: &Value,
    input: &act::Input,
    fields: &[(String, String)],
    token: &str,
    login: &str,
    now: i64,
    rec: impl Fn(String) -> Option<Value>,
) -> (Response, Option<Value>) {
    let name = format!("args.{}", input.field);
    let typed: Vec<&str> = fields.iter().filter(|(n, _)| *n == name).map(|(_, v)| v.as_str()).collect();
    let [text] = typed.as_slice() else {
        return (Response::text(400, "the form carries no one typed line"), rec("refused:400".into()));
    };
    let with = |r: Option<Value>| {
        r.map(|mut v| {
            v["text_len"] = text.len().into();
            v["text_sha256"] = act::hex(&Sha256::digest(text.as_bytes())).into();
            v
        })
    };
    if let Err(why) = act::typed(input, text) {
        return (again(site, a, path, login, now, 422, &format!("Nothing was sent: {why}.")), with(rec("refused:422".into())));
    }
    let Some(to) = entry["args"]["to"].as_str().filter(|t| !t.trim().is_empty()) else {
        return (Response::text(422, "this act names no one to say it to"), with(rec("refused:422".into())));
    };
    let reply = entry["args"]["in_reply_to"].as_str().filter(|r| !r.trim().is_empty());
    let back = |how: &str| {
        let to = format!("{}{}?did=say&as={how}", site.mount, page::href("/", path, "folds"));
        Response::new(303).with("Location", to).with("Cache-Control", "no-store")
    };
    if !a.spent.lock().map(|mut s| s.insert(token.to_string())).unwrap_or(false) {
        return (back("already"), with(rec("already".into())));
    }
    match a.actor.say(to, text, reply, id) {
        Ok(Done::Ok) => (back("ok"), with(rec("ok".into()))),
        Ok(Done::Already) => (back("already"), with(rec("already".into()))),
        Err(e) => {
            // not sent: the same form may try again
            if let Ok(mut s) = a.spent.lock() {
                s.remove(token);
            }
            let e: String = e.chars().filter(|c| !c.is_control()).take(200).collect();
            (Response::text(502, &format!("chitin did not take it: {e}")), with(rec(format!("chitin:{e}"))))
        }
    }
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
    if segs == [NODES, ""] && fp.is_dir() && !fp.join("index.html").exists() {
        return validated(req, Class::Node, "application/json", nodes_index(&fp).into_bytes(), dir_modified(&fp));
    }
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
    // A fold's page, where this server reports to Clax: its list ends with "Comment in Clax", a plain link to the fold's comment
    // page (close condition 4: with JS off, the page still offers it). Added as it is served, so the pages as written are unchanged.
    if let (Some(_), true) = (&site.reporting, page) {
        if let Some(p) = std::str::from_utf8(&bytes).ok().filter(|p| p.contains("</ol></main>")) {
            let fold: Vec<String> =
                segs.iter().filter(|s| !s.is_empty() && **s != "index.html" && **s != page::SLIDES).map(|s| s.to_string()).collect();
            let row = format!("<li class=\"prop\"><a href=\"{}\">Comment in Clax</a></li></ol></main>", text(&comment_href(&site.mount, &fold)));
            bytes = p.replacen("</ol></main>", &row, 1).into_bytes();
        }
    }
    let class = if site.no_store.iter().any(|p| rel[1..].starts_with(p.as_str())) {
        Class::NoStore
    } else if rel.contains("@sha256:") {
        Class::Immutable
    } else if page {
        Class::Page
    } else if segs.first() == Some(&NODES) && fp.extension().is_some_and(|e| e == "json") {
        Class::Node
    } else if fp.extension().is_some_and(|e| e == "json") {
        Class::Data
    } else {
        Class::Asset
    };
    let asof: DateTime<Utc> = meta.modified().unwrap_or(SystemTime::UNIX_EPOCH).into();
    let mut r = validated(req, class, content_type(&fp), bytes, asof);
    if private && class == Class::Page {
        r.headers.iter_mut().filter(|(k, _)| k == "Cache-Control").for_each(|(_, v)| *v = "private, no-cache".into());
    }
    r
}

/// The live nodes' directory under the root (`nodes/<id>.json`, one per fold, each written whole by its own handler): what a viewer
/// revalidates fold by fold. `GET nodes/` is their index ([`nodes_index`]).
pub const NODES: &str = "nodes";

/// The newest modification time in `dir` and below: when the index last changed.
fn dir_modified(dir: &Path) -> DateTime<Utc> {
    fn walk(d: &Path, t: &mut SystemTime) {
        for e in fs::read_dir(d).into_iter().flatten().flatten() {
            let p = e.path();
            if let Ok(m) = e.metadata() {
                *t = (*t).max(m.modified().unwrap_or(SystemTime::UNIX_EPOCH));
                if m.is_dir() {
                    walk(&p, t);
                }
            }
        }
    }
    let mut t = fs::metadata(dir).and_then(|m| m.modified()).unwrap_or(SystemTime::UNIX_EPOCH);
    walk(dir, &mut t);
    t.into()
}

/// `GET nodes/`: every node under it (`<id>.json`, an id holding `/` a subdirectory), sorted by id, as `{nodes: [{id, href, etag,
/// source, updated, ok, stale_since, cadence_s}]}`. The ETag listed is the one `GET <href>` answers with, so a viewer that holds it
/// knows without asking whether that node changed; the index's own ETag changes when any node does. A file that is not a JSON object
/// is listed with `ok: false` and why, never left out, so a broken handler shows as one.
pub fn nodes_index(dir: &Path) -> String {
    fn walk(d: &Path, under: &str, out: &mut Vec<Value>) {
        let mut es: Vec<_> = fs::read_dir(d).into_iter().flatten().flatten().collect();
        es.sort_by_key(|e| e.file_name());
        for e in es {
            let Some(name) = e.file_name().to_str().map(str::to_string) else { continue };
            if name.starts_with('.') {
                continue; // a handler's temporary file, before its rename
            }
            let p = e.path();
            if p.is_dir() {
                walk(&p, &format!("{under}{name}/"), out);
                continue;
            }
            let Some(stem) = name.strip_suffix(".json") else { continue };
            let Ok(bytes) = fs::read(&p) else { continue };
            let hex: String = Sha256::digest(&bytes).iter().map(|b| format!("{b:02x}")).collect();
            let id = format!("{under}{stem}");
            let href = format!("{under}{name}").split('/').map(|s| segment_of(s)).collect::<Vec<_>>().join("/");
            let mut row = json!({ "id": id, "href": href, "etag": format!("\"sha-256:{hex}\"") });
            match serde_json::from_slice::<Value>(&bytes) {
                Ok(Value::Object(n)) => {
                    for k in ["source", "updated", "ok", "stale_since", "cadence_s"] {
                        if let Some(v) = n.get(k) {
                            row[k] = v.clone();
                        }
                    }
                }
                _ => {
                    row["ok"] = false.into();
                    row["error"] = "not a JSON object".into();
                }
            }
            out.push(row);
        }
    }
    let mut nodes = vec![];
    walk(dir, "", &mut nodes);
    json!({ "nodes": nodes }).to_string() + "\n"
}

/// `bytes` as one 200 of `class`, with its strong ETag (their sha-256), Last-Modified `asof`, Content-Digest and Server-Timing; a 304
/// with no body when `If-None-Match` names that ETag. A `NoStore` answer carries no validator and is never a 304.
fn validated(req: &Request, class: Class, ctype: &str, bytes: Vec<u8>, asof: DateTime<Utc>) -> Response {
    let hash = Sha256::digest(&bytes);
    let hex: String = hash.iter().map(|b| format!("{b:02x}")).collect();
    let etag = format!("\"sha-256:{hex}\"");
    let mut r = Response::new(200).with("Content-Type", ctype).with("Cache-Control", class.cache_control());
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
    let site = Arc::new(site);
    if site.acting.as_ref().is_some_and(|a| a.notify_as.is_some() && a.needs_as.is_some()) {
        let (w, log) = (site.clone(), log.clone());
        std::thread::spawn(move || {
            let mut seen = Watched::default();
            loop {
                let now = DateTime::<Utc>::from(SystemTime::now());
                for mut l in watch(&w, &mut seen, now.timestamp()) {
                    l["ts"] = iso(now).into();
                    if let Some(p) = &log {
                        crate::http::append(p, &l);
                    }
                }
                std::thread::sleep(WATCH_EVERY);
            }
        });
    }
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
