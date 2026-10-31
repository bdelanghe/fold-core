//! Acting from a page (S2: allie-s2-fold-act-write §2-4): what `fold serve`'s `POST act/<id>` route (serve.rs) and `site()`'s act
//! map (page.rs, `_acts.json`) share. The act's fixity, the token that carries it, the form body, and the [`Actor`] that moves a
//! chitin message's ack state for the one person this server acts for: ack, defer and decline only (Bobby's boundary 42184457).
//!
//! - **Fixity:** sha-256 hex of the canonical JSON (keys sorted, no space) of `{act, item}` as `_acts.json` holds it: per item,
//!   so a fold rewritten for an unrelated change keeps every other item's tokens good.
//! - **Token:** `v1.<exp>.<fixity>.<mac>`, `mac` = base64url (no padding) of HMAC-SHA256(key, length-prefixed `"fold-act"`, act id,
//!   fixity, exp, login). The brief's `v1.<exp>.<mac>` with the fixity carried in the clear beside it: the server must know which
//!   fixity was signed to tell a changed fold (409) from a forged token (403). Checked in the brief's order: shape and lengths,
//!   then the MAC (constant time), then `exp`, then the fixity against the current act map.
//! - **Key:** 32 random bytes in a file (0600, its directory 0700), made at start when missing; never logged.

#[cfg(not(target_arch = "wasm32"))]
use std::fs;
#[cfg(not(target_arch = "wasm32"))]
use std::io::{Read, Write};
#[cfg(not(target_arch = "wasm32"))]
use std::path::Path;
#[cfg(not(target_arch = "wasm32"))]
use std::process::Command;

use serde_json::Value;
use sha2::{Digest, Sha256};

/// A token's life: a day's use of an open page (S2 brief §3).
pub const TTL: i64 = 12 * 3600;

/// The acts this server may run, each a `(cap, call)` pair as Allie's cap host names them (`cap` the capability, `call` the
/// verb; personal-allie-lead 4f93c11f): only `chitin` × `ack`, `defer`, `decline`, each as the chitin ack state it sends and the
/// fixed reason chitin requires for it (S2 brief §4). Every other pair, `chitin`/`say` among them, is refused.
pub fn state_of(cap: &str, call: &str) -> Option<(&'static str, Option<&'static str>)> {
    if cap != "chitin" {
        return None;
    }
    match call {
        "ack" => Some(("acting", None)),
        "defer" => Some(("deferred", Some("Deferred from Basis"))),
        "decline" => Some(("declined", Some("Declined from Basis"))),
        _ => None,
    }
}

/// What the status line says after a call, done (`ok`) or done before (`already`): a fixed table, the query only selects a row.
pub fn said(call: &str, how: &str) -> Option<&'static str> {
    Some(match (call, how) {
        ("ack", "ok") => "Acknowledged.",
        ("defer", "ok") => "Deferred.",
        ("decline", "ok") => "Declined.",
        ("ack", "already") => "Already acknowledged.",
        ("defer", "already") => "Already deferred.",
        ("decline", "already") => "Already declined.",
        _ => return None,
    })
}

fn hex(b: &[u8]) -> String {
    b.iter().map(|x| format!("{x:02x}")).collect()
}

/// JSON with its object keys sorted at every depth and no whitespace: one text for one value, whatever order it was built in.
pub fn canonical(v: &Value) -> String {
    match v {
        Value::Object(m) => {
            let mut ks: Vec<&String> = m.keys().collect();
            ks.sort();
            let kv: Vec<String> = ks.iter().map(|k| format!("{}:{}", Value::String((*k).clone()), canonical(&m[*k]))).collect();
            format!("{{{}}}", kv.join(","))
        }
        Value::Array(a) => format!("[{}]", a.iter().map(canonical).collect::<Vec<_>>().join(",")),
        v => v.to_string(),
    }
}

/// An act's fixity: what its token is bound to, so a changed item is told from a forged token.
pub fn fixity(item: &str, act: &Value) -> String {
    hex(&Sha256::digest(canonical(&serde_json::json!({ "item": item, "act": act })).as_bytes()))
}

/// HMAC-SHA256 (RFC 2104), on sha2 alone.
pub fn hmac(key: &[u8], msg: &[u8]) -> [u8; 32] {
    let mut k = [0u8; 64];
    if key.len() > 64 {
        k[..32].copy_from_slice(&Sha256::digest(key));
    } else {
        k[..key.len()].copy_from_slice(key);
    }
    let pad = |b: u8| k.iter().map(|x| x ^ b).collect::<Vec<u8>>();
    let inner = Sha256::new().chain_update(pad(0x36)).chain_update(msg).finalize();
    Sha256::new().chain_update(pad(0x5c)).chain_update(inner).finalize().into()
}

const B64URL: &[u8; 64] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789-_";

fn b64url(b: &[u8]) -> String {
    let mut o = String::new();
    for c in b.chunks(3) {
        let n = (c[0] as u32) << 16 | (*c.get(1).unwrap_or(&0) as u32) << 8 | *c.get(2).unwrap_or(&0) as u32;
        for k in 0..=c.len() {
            o.push(B64URL[(n >> (18 - 6 * k) & 63) as usize] as char);
        }
    }
    o
}

fn mac(key: &[u8], act: &str, fixity: &str, exp: i64, login: &str) -> String {
    let mut m = vec![];
    for f in ["fold-act", act, fixity, &exp.to_string(), login] {
        m.extend((f.len() as u32).to_be_bytes());
        m.extend(f.as_bytes());
    }
    b64url(&hmac(key, &m))
}

/// A fresh token for one act, one fixity and one login, good until `now` + [`TTL`].
pub fn mint(key: &[u8], act: &str, fixity: &str, login: &str, now: i64) -> String {
    let exp = now + TTL;
    format!("v1.{exp}.{fixity}.{}", mac(key, act, fixity, exp, login))
}

/// Why a token is not taken, in the order it is checked.
#[derive(Debug, PartialEq)]
pub enum Bad {
    /// Not a token's shape (400).
    Malformed,
    /// Not minted by this key for this act and login (403).
    Forged,
    /// Minted here, but its time is up (the 403 stale re-render).
    Expired,
}

/// A token's signed fixity, once its shape, MAC and expiry hold: the caller compares it with the act map's (409 when it differs).
pub fn check(key: &[u8], token: &str, act: &str, login: &str, now: i64) -> Result<String, Bad> {
    let p: Vec<&str> = token.split('.').collect();
    let [v, exp, fx, m] = p.as_slice() else { return Err(Bad::Malformed) };
    let num = |s: &str| !s.is_empty() && s.len() <= 12 && s.bytes().all(|b| b.is_ascii_digit());
    let hexd = |s: &str| s.len() == 64 && s.bytes().all(|b| b.is_ascii_digit() || (b'a'..=b'f').contains(&b));
    if *v != "v1" || !num(exp) || !hexd(fx) || m.len() != 43 || !m.bytes().all(|b| B64URL.contains(&b)) {
        return Err(Bad::Malformed);
    }
    let exp: i64 = exp.parse().map_err(|_| Bad::Malformed)?;
    let want = mac(key, act, fx, exp, login);
    if want.bytes().zip(m.bytes()).fold(0u8, |d, (a, b)| d | (a ^ b)) != 0 {
        return Err(Bad::Forged);
    }
    if now >= exp {
        return Err(Bad::Expired);
    }
    Ok(fx.to_string())
}

#[cfg(not(target_arch = "wasm32"))]
/// The act key at `path`: read, or made (32 bytes from the system's random source, the file 0600 in a 0700 directory).
pub fn key(path: &Path) -> std::io::Result<Vec<u8>> {
    if let Ok(k) = fs::read(path) {
        if k.len() == 32 {
            return Ok(k);
        }
        return Err(std::io::Error::other(format!("{}: not a 32-byte key", path.display())));
    }
    let mut k = vec![0u8; 32];
    fs::File::open("/dev/urandom")?.read_exact(&mut k)?;
    if let Some(d) = path.parent().filter(|d| !d.as_os_str().is_empty()) {
        fs::create_dir_all(d)?;
        private(d, 0o700)?;
    }
    let mut o = fs::OpenOptions::new();
    o.write(true).create_new(true);
    #[cfg(unix)]
    std::os::unix::fs::OpenOptionsExt::mode(&mut o, 0o600);
    o.open(path)?.write_all(&k)?;
    Ok(k)
}

#[cfg(not(target_arch = "wasm32"))]
#[cfg(unix)]
fn private(p: &Path, mode: u32) -> std::io::Result<()> {
    use std::os::unix::fs::PermissionsExt;
    fs::set_permissions(p, fs::Permissions::from_mode(mode))
}

#[cfg(not(target_arch = "wasm32"))]
#[cfg(not(unix))]
fn private(_: &Path, _: u32) -> std::io::Result<()> {
    Ok(())
}

/// An `application/x-www-form-urlencoded` body as its fields, in order; None when a field is not UTF-8 or a `%` is cut.
pub fn form(body: &[u8]) -> Option<Vec<(String, String)>> {
    let dec = |s: &str| -> Option<String> {
        let b = s.as_bytes();
        let mut o = Vec::with_capacity(b.len());
        let mut i = 0;
        while i < b.len() {
            match b[i] {
                b'+' => o.push(b' '),
                b'%' => {
                    o.push(u8::from_str_radix(std::str::from_utf8(b.get(i + 1..i + 3)?).ok()?, 16).ok()?);
                    i += 2;
                }
                c => o.push(c),
            }
            i += 1;
        }
        String::from_utf8(o).ok()
    };
    let s = std::str::from_utf8(body).ok()?;
    s.split('&')
        .filter(|p| !p.is_empty())
        .map(|p| {
            let (k, v) = p.split_once('=').unwrap_or((p, ""));
            Some((dec(k)?, dec(v)?))
        })
        .collect()
}

/// What became of an ack.
#[derive(Debug, PartialEq, Clone)]
pub enum Done {
    /// Sent and taken.
    Ok,
    /// Taken before: chitin's ledger moved nothing (a double tap, a replay, a back-button resubmit).
    Already,
}

/// One ack state move on one chitin message, for the one person this server acts for. The route calls it only once every check
/// has held; tests inject a recording stub.
pub trait Actor: Send + Sync {
    /// Tell `to`, the seat that sent `msg_id`, that it is now `state` (with `why`, where chitin needs a reason), by the act `act`
    /// (its act id, the ack's artifact `fold:<act>`).
    fn ack(&self, msg_id: &str, state: &str, to: &str, why: Option<&str>, act: &str) -> Result<Done, String>;
}

#[cfg(not(target_arch = "wasm32"))]
/// The real actor: `chitin ack <msg_id> <state> --to <sender> --as <seat> --ref fold:<act id> [--why <why>]`, the reason fixed per
/// state (work-chitin-author's ack delegate, via personal-assistant d46b12c1); the key from the environment chitin reads
/// (`CHITIN_KEY_FILE`, set by fold-serve.nix). `written …` is done; a noop chitin reports (`already …`, `… is final`) is done
/// before; anything else is chitin's one-line reason.
pub struct Chitin {
    pub bin: String,
    pub seat: String,
}

#[cfg(not(target_arch = "wasm32"))]
impl Actor for Chitin {
    fn ack(&self, msg_id: &str, state: &str, to: &str, why: Option<&str>, act: &str) -> Result<Done, String> {
        let mut c = Command::new(&self.bin);
        c.args(["ack", msg_id, state, "--to", to, "--as", &self.seat, "--ref", &format!("fold:{act}")]);
        if let Some(w) = why {
            c.args(["--why", w]);
        }
        let out = c.output().map_err(|e| format!("could not run chitin: {e}"))?;
        let (o, e) = (String::from_utf8_lossy(&out.stdout), String::from_utf8_lossy(&out.stderr));
        let line = o.lines().chain(e.lines()).map(str::trim).find(|l| !l.is_empty()).unwrap_or("").to_string();
        let noop = line.contains("already ") || line.ends_with(" is final") || line.contains("does not follow");
        match (out.status.success(), noop) {
            (_, true) => Ok(Done::Already),
            (true, false) if line.starts_with("written") => Ok(Done::Ok),
            _ => Err(if line.is_empty() { format!("chitin exited {}", out.status) } else { line.chars().take(200).collect() }),
        }
    }
}
