//! `fold sink` (brief items 21, 24 and 27): the no-JS signals' one write path, split from `fold serve` so the process that serves
//! every page holds no write. Its own port, at `<mount>/_sink` under tailscale serve (which strips that, so requests arrive as
//! `/v`, `/report`). It only appends: each accepted request is one JSON line in the log, and nothing is ever read back, changed or
//! served. Approvals never come here; they stay chitin's `/approve`. [`accept`] is pure (a request, its time and the limiter in;
//! the answer and the record out), so tests/sink.rs holds its answers as goldens; [`run`] is the loopback loop (http.rs).
//!
//! - **Identity:** a request with no `Tailscale-User-Login` is refused (403) and leaves nothing. Bound to loopback, only
//!   tailscale serve reaches it, and it sets that header from the tailnet's own identity.
//! - **Rate:** each viewer has [`Limiter::per_minute`] accepted requests a clock minute; past that, 429 with `Retry-After`.
//! - **`POST /v`:** a browser's `<a ping>` (`text/ping`, `Ping-From`, `Ping-To`; page.rs gives links no `ping` until a sink is
//!   deployed, Bobby 10ee9ea8) logs a `ping`: the row it was followed from, to the fold it goes to. A prefetch never pings, so it is never a view.
//! - **`POST /report`:** `application/reports+json` from the Reporting API (serve.rs names this endpoint on every page) logs a
//!   `report`.
//! - Bodies are at most [`MAX_BODY`]; any other path or method is refused, and nothing here answers a GET.

use std::collections::HashMap;
use std::net::TcpListener;
use std::path::PathBuf;
use std::sync::{Arc, Mutex};

use chrono::{DateTime, SecondsFormat, Utc};
use serde_json::{json, Value};

use crate::http::{Request, Response};

/// The most a body may be: a ping is `PING`, a batch of reports a few KB.
pub const MAX_BODY: usize = 64 * 1024;

/// Accepted requests per viewer per clock minute.
pub struct Limiter {
    pub per_minute: u32,
    seen: HashMap<String, (i64, u32)>,
}

impl Limiter {
    pub fn new(per_minute: u32) -> Limiter {
        Limiter { per_minute, seen: HashMap::new() }
    }

    /// Whether `viewer` may have one more at `at`, counting it if so.
    fn take(&mut self, viewer: &str, at: DateTime<Utc>) -> bool {
        let minute = at.timestamp().div_euclid(60);
        // only this minute's counts matter: forget the rest so the map holds at most one minute's viewers
        self.seen.retain(|_, (m, _)| *m == minute);
        let (_, n) = self.seen.entry(viewer.to_string()).or_insert((minute, 0));
        if *n >= self.per_minute {
            return false;
        }
        *n += 1;
        true
    }
}

fn refuse(status: u16, why: &str) -> (Response, Option<Value>) {
    (Response::text(status, why), None)
}

/// One request's answer, and the record it appends, if any.
pub fn accept(lim: &mut Limiter, req: &Request, at: DateTime<Utc>) -> (Response, Option<Value>) {
    let path = req.target.split(['?', '#']).next().unwrap_or("");
    if req.method != "POST" {
        return (Response::text(405, "append-only: POST").with("Allow", "POST"), None);
    }
    if path != "/v" && path != "/report" {
        return refuse(404, "only /v and /report");
    }
    let Some(viewer) = req.header("Tailscale-User-Login").filter(|v| !v.is_empty()) else {
        return refuse(403, "who are you? (no tailnet identity)");
    };
    let mut r = json!({ "at": at.to_rfc3339_opts(SecondsFormat::Secs, true), "kind": "", "viewer": viewer });
    if path == "/v" {
        if req.media_type() != Some("text/ping") {
            return refuse(415, "a ping is text/ping");
        }
        r["kind"] = "ping".into();
        r["from"] = req.header("Ping-From").into();
        r["to"] = req.header("Ping-To").into();
    } else {
        if req.media_type() != Some("application/reports+json") {
            return refuse(415, "reports are application/reports+json");
        }
        let reports = serde_json::from_slice::<Value>(&req.body).ok().filter(Value::is_array);
        let Some(reports) = reports else {
            return refuse(400, "reports are a JSON array");
        };
        r["kind"] = "report".into();
        r["reports"] = reports;
    }
    // counted only once it would be logged: a malformed request costs the viewer nothing, and logs nothing
    if !lim.take(viewer, at) {
        return (Response::text(429, "slow down").with("Retry-After", (60 - at.timestamp().rem_euclid(60)).to_string()), None);
    }
    (Response::new(204).with("Cache-Control", "no-store"), Some(r))
}

/// Take signals on `listener` for ever, each record one JSON line appended to `log`.
pub fn run(listener: TcpListener, per_minute: u32, log: PathBuf) -> std::io::Result<()> {
    let lim = Mutex::new(Limiter::new(per_minute));
    crate::http::run(
        listener,
        MAX_BODY,
        Some(log),
        Arc::new(move |req: &Request, at| accept(&mut lim.lock().unwrap_or_else(|e| e.into_inner()), req, at)),
    )
}
