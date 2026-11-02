//! The HTTP/1.1 both of fold's processes speak (brief item 27): `fold serve` (serve.rs, read-only) and `fold sink` (sink.rs,
//! append-only). One request per connection (`Connection: close`), on loopback, for tailscale serve to put at a mount. [`parse`]
//! reads from any `BufRead`, so tests/fuzz.rs throws bytes at it without a socket.

use std::fs;
use std::io::{BufRead, BufReader, Write};
use std::net::{IpAddr, TcpListener, TcpStream};
use std::path::PathBuf;
use std::sync::{Arc, Mutex};
use std::time::{Instant, SystemTime};

use chrono::{DateTime, Utc};
use serde_json::Value;

/// The most headers a request may carry.
const MAX_HEADERS: usize = 100;

pub struct Request {
    pub method: String,
    pub target: String,
    pub headers: Vec<(String, String)>,
    pub body: Vec<u8>,
    /// The TCP peer, as [`run`] accepted it; None for a request not read from a socket (tests, [`parse`]).
    pub peer: Option<IpAddr>,
}

impl Request {
    pub fn header(&self, name: &str) -> Option<&str> {
        self.headers.iter().find(|(k, _)| k.eq_ignore_ascii_case(name)).map(|(_, v)| v.as_str())
    }

    /// The `Content-Type` without its parameters.
    pub fn media_type(&self) -> Option<&str> {
        self.header("Content-Type").map(|c| c.split(';').next().unwrap_or("").trim())
    }
}

pub struct Response {
    pub status: u16,
    pub headers: Vec<(String, String)>,
    pub body: Vec<u8>,
}

impl Response {
    pub(crate) fn new(status: u16) -> Response {
        Response { status, headers: vec![], body: vec![] }
    }
    pub(crate) fn with(mut self, k: &str, v: impl Into<String>) -> Response {
        self.headers.push((k.into(), v.into()));
        self
    }
    pub(crate) fn text(status: u16, why: &str) -> Response {
        let mut r = Response::new(status).with("Content-Type", "text/plain; charset=utf-8").with("Cache-Control", "no-store");
        r.body = format!("{why}\n").into_bytes();
        r
    }
    pub fn header(&self, name: &str) -> Option<&str> {
        self.headers.iter().find(|(k, _)| k.eq_ignore_ascii_case(name)).map(|(_, v)| v.as_str())
    }
}

fn status_text(s: u16) -> &'static str {
    match s {
        200 => "OK",
        204 => "No Content",
        301 => "Moved Permanently",
        303 => "See Other",
        304 => "Not Modified",
        400 => "Bad Request",
        403 => "Forbidden",
        404 => "Not Found",
        405 => "Method Not Allowed",
        409 => "Conflict",
        413 => "Content Too Large",
        415 => "Unsupported Media Type",
        422 => "Unprocessable Content",
        429 => "Too Many Requests",
        502 => "Bad Gateway",
        _ => "Error",
    }
}

/// One request, its body at most `max_body` bytes, or the answer that refuses it.
pub fn parse(rd: &mut impl BufRead, max_body: usize) -> Result<Request, Response> {
    parse_with(rd, &|_: &str, _: &str| max_body)
}

/// How many body bytes a request may send, by its method and target: asked before any of the body is read.
pub type Limit = dyn Fn(&str, &str) -> usize + Send + Sync;

/// One request, its body at most `limit(method, target)` bytes, or the answer that refuses it.
pub fn parse_with(rd: &mut impl BufRead, limit: &dyn Fn(&str, &str) -> usize) -> Result<Request, Response> {
    let mut line = String::new();
    rd.read_line(&mut line).map_err(|_| Response::text(400, "no request"))?;
    let mut parts = line.split_whitespace();
    let (Some(method), Some(target)) = (parts.next(), parts.next()) else {
        return Err(Response::text(400, "no request line"));
    };
    let (method, target) = (method.to_string(), target.to_string());
    let mut headers = vec![];
    loop {
        let mut h = String::new();
        if rd.read_line(&mut h).map_err(|_| Response::text(400, "cut off"))? == 0 || h.trim_end().is_empty() {
            break;
        }
        if headers.len() >= MAX_HEADERS {
            return Err(Response::text(400, "too many headers"));
        }
        if let Some((k, v)) = h.split_once(':') {
            headers.push((k.trim().to_string(), v.trim().to_string()));
        }
    }
    let mut req = Request { method, target, headers, body: vec![], peer: None };
    let len = match req.header("Content-Length") {
        None => 0,
        Some(v) => v.parse::<usize>().map_err(|_| Response::text(400, "Content-Length is not a length"))?,
    };
    if req.header("Transfer-Encoding").is_some() {
        return Err(Response::text(400, "no Transfer-Encoding: send a Content-Length"));
    }
    let max_body = limit(&req.method, &req.target);
    if len > max_body {
        return Err(Response::text(413, &format!("at most {max_body} bytes")));
    }
    req.body = vec![0; len];
    rd.read_exact(&mut req.body).map_err(|_| Response::text(400, "body cut off"))?;
    Ok(req)
}

/// The most a loopback call's answer may be (the overlay is about 140 KB).
pub const MAX_ANSWER: usize = 4 << 20;

/// An `http://` origin on loopback (`127.0.0.1`, `localhost` or `[::1]`, with a port) as its host and port; None for anything else,
/// so a call ([`call`]) can only ever reach this machine, in the clear only where nothing leaves it.
pub fn loopback(origin: &str) -> Option<(String, u16)> {
    let rest = origin.trim_end_matches('/').strip_prefix("http://")?;
    let (host, port) = rest.rsplit_once(':')?;
    if !["127.0.0.1", "localhost", "[::1]"].contains(&host) {
        return None;
    }
    let port: u16 = port.parse().ok().filter(|p| *p > 0)?;
    Some((host.trim_matches(['[', ']']).to_string(), port))
}

/// The Host header [`call`] sends to a loopback origin (`127.0.0.1:7480`, `[::1]:7480`): what a signed request signs as its host.
pub fn authority(origin: &str) -> Option<String> {
    let (host, port) = loopback(origin)?;
    Some(if host.contains(':') { format!("[{host}]:{port}") } else { format!("{host}:{port}") })
}

/// One HTTP/1.1 request to a loopback origin ([`loopback`]), `Connection: close`: its status, headers and body (a Content-Length or
/// chunked one, or to the close), or why not. For fold serve's calls to the Clax instance beside it (report.rs).
pub fn call(origin: &str, method: &str, path: &str, headers: &[(&str, &str)], body: &[u8]) -> Result<Response, String> {
    use std::io::Read;
    let (host, port) = loopback(origin).ok_or_else(|| format!("{origin}: not an http origin on loopback"))?;
    let mut s = TcpStream::connect((host.as_str(), port)).map_err(|e| format!("{origin}: {e}"))?;
    let _ = s.set_read_timeout(Some(std::time::Duration::from_secs(10)));
    let authority = authority(origin).unwrap_or_default();
    let mut o = format!("{method} {path} HTTP/1.1\r\nHost: {authority}\r\nConnection: close\r\nContent-Length: {}\r\n", body.len());
    for (k, v) in headers {
        o += &format!("{k}: {v}\r\n");
    }
    o += "\r\n";
    s.write_all(o.as_bytes()).and_then(|_| s.write_all(body)).map_err(|e| format!("{origin}: {e}"))?;
    let mut raw = vec![];
    s.take(MAX_ANSWER as u64 + 64 * 1024).read_to_end(&mut raw).map_err(|e| format!("{origin}: {e}"))?;
    answer_of(&raw).ok_or_else(|| format!("{origin}: not an HTTP answer"))
}

/// An HTTP/1.1 answer's bytes as a [`Response`]: None when its head, its length or its chunks do not hold.
pub fn answer_of(raw: &[u8]) -> Option<Response> {
    let end = raw.windows(4).position(|w| w == b"\r\n\r\n")?;
    let head = std::str::from_utf8(&raw[..end]).ok()?;
    let mut lines = head.split("\r\n");
    let status: u16 = lines.next()?.split_whitespace().nth(1)?.parse().ok()?;
    let headers: Vec<(String, String)> =
        lines.filter_map(|l| l.split_once(':')).map(|(k, v)| (k.trim().to_string(), v.trim().to_string())).collect();
    let r = Response { status, headers, body: vec![] };
    let rest = &raw[end + 4..];
    let body = if r.header("Transfer-Encoding").is_some_and(|t| t.eq_ignore_ascii_case("chunked")) {
        let (mut out, mut at) = (vec![], 0);
        loop {
            let nl = rest[at..].windows(2).position(|w| w == b"\r\n")? + at;
            let size = std::str::from_utf8(&rest[at..nl]).ok()?.split(';').next()?.trim();
            let n = usize::from_str_radix(size, 16).ok()?;
            if n == 0 {
                break;
            }
            out.extend_from_slice(rest.get(nl + 2..nl + 2 + n)?);
            at = nl + 2 + n + 2;
            if out.len() > MAX_ANSWER {
                return None;
            }
        }
        out
    } else if let Some(n) = r.header("Content-Length") {
        rest.get(..n.parse::<usize>().ok()?)?.to_vec()
    } else {
        rest.to_vec()
    };
    (body.len() <= MAX_ANSWER).then_some(Response { body, ..r })
}

fn write(s: &mut TcpStream, r: &Response, head: bool) -> std::io::Result<()> {
    let mut o = format!("HTTP/1.1 {} {}\r\n", r.status, status_text(r.status));
    for (k, v) in &r.headers {
        o += &format!("{k}: {v}\r\n");
    }
    o += &format!("Content-Length: {}\r\nConnection: close\r\n\r\n", r.body.len());
    s.write_all(o.as_bytes())?;
    if !head {
        s.write_all(&r.body)?;
    }
    s.flush()
}

/// What answers one request at a time, and the record it leaves, if any.
pub type Handler = dyn Fn(&Request, DateTime<Utc>) -> (Response, Option<Value>) + Send + Sync;

/// One record appended to `log` as a JSON line. A log is the server's own record (acts.jsonl names who acted): readable by its owner
/// only. One line is one append, so a writer outside [`run`] (serve.rs's watcher) never splits another's line.
pub fn append(log: &std::path::Path, rec: &serde_json::Value) {
    let mut o = fs::OpenOptions::new();
    o.create(true).append(true);
    #[cfg(unix)]
    std::os::unix::fs::OpenOptionsExt::mode(&mut o, 0o600);
    if let Ok(mut f) = o.open(log) {
        let _ = f.write_all(format!("{rec}\n").as_bytes());
    }
}

/// Answer on `listener` for ever, one thread per connection. A `Server-Timing` the handler sends gets `serve;dur=<ms>`. Each
/// record is one JSON line appended to `log`; a handler with no log leaves none.
pub fn run(listener: TcpListener, limit: Arc<Limit>, log: Option<PathBuf>, handle: Arc<Handler>) -> std::io::Result<()> {
    let lock = Arc::new(Mutex::new(()));
    for s in listener.incoming() {
        let (handle, log, lock, limit) = (handle.clone(), log.clone(), lock.clone(), limit.clone());
        let Ok(mut s) = s else { continue };
        std::thread::spawn(move || {
            let t0 = Instant::now();
            let parsed = match s.try_clone() {
                Ok(c) => parse_with(&mut BufReader::new(c), &*limit),
                Err(_) => return,
            };
            let (mut r, head, rec) = match parsed {
                Ok(mut req) => {
                    req.peer = s.peer_addr().ok().map(|a| a.ip());
                    let (r, rec) = handle(&req, SystemTime::now().into());
                    (r, req.method == "HEAD", rec)
                }
                Err(r) => (r, false, None),
            };
            if let Some((_, st)) = r.headers.iter_mut().find(|(k, _)| k == "Server-Timing") {
                st.push_str(&format!(", serve;dur={:.1}", t0.elapsed().as_secs_f64() * 1000.0));
            }
            let _ = write(&mut s, &r, head);
            if let (Some(rec), Some(log)) = (rec, log) {
                let _g = lock.lock();
                append(&log, &rec);
            }
        });
    }
    Ok(())
}
