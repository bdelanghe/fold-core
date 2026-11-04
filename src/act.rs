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

/// The ack moves this server may run, each a `(cap, call)` pair as Allie's cap host names them (`cap` the capability, `call` the
/// verb; personal-allie-lead 4f93c11f): `chitin` × `ack`, `defer`, `decline`, each as the chitin ack state it sends and the
/// fixed reason chitin requires for it (S2 brief §4). The one other act it runs, chitin's `say`, is [`run_of`]'s.
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
        ("say", "ok") => "Sent.",
        ("ack", "already") => "Already acknowledged.",
        ("defer", "already") => "Already deferred.",
        ("decline", "already") => "Already declined.",
        ("say", "already") => "Already sent.",
        _ => return None,
    })
}

/// The most a typed value may hold, in characters, whatever an act's `input` asks.
pub const INPUT_MAX: u64 = 1000;

/// An act's one typed field (Bobby 07c5f8dc, as designed in 5f9d0d87 and answered in 3afeab31): `input: {field, label,
/// maxlength, charset}` on the act, rendered as a named text box before its button and posted as `args.<field>`. The spec is
/// part of the act, so its fixity and token cover it; the typed value is payload, outside both, and is checked here against
/// the act map's spec, never against the form.
#[derive(Debug, PartialEq, Clone)]
pub struct Input {
    pub field: String,
    pub maxlength: usize,
}

/// An act's `input`, when it is one this server takes: a `field` of `[a-z][a-z0-9_]*` naming no fixed arg, a `maxlength` of 1 to
/// [`INPUT_MAX`], and the one charset there is, `ascii-printable` (0x20-0x7E). Anything else is no input this server takes.
pub fn input_of(act: &Value) -> Option<Input> {
    let i = act.get("input")?;
    let field = i.get("field")?.as_str()?;
    let mut b = field.bytes();
    if !b.next().is_some_and(|c| c.is_ascii_lowercase()) || !b.all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || c == b'_') {
        return None;
    }
    if act.get("args").and_then(|a| a.get(field)).is_some() {
        return None;
    }
    let max = i.get("maxlength")?.as_u64().filter(|m| (1..=INPUT_MAX).contains(m))?;
    if i.get("charset")?.as_str()? != "ascii-printable" {
        return None;
    }
    Some(Input { field: field.into(), maxlength: max as usize })
}

/// Why a typed value is not sent: a fixed sentence (the value is never echoed).
pub fn typed(input: &Input, v: &str) -> Result<(), &'static str> {
    if v.trim().is_empty() {
        return Err("nothing was typed");
    }
    if !v.bytes().all(|b| (0x20..=0x7e).contains(&b)) {
        return Err("only letters, digits, spaces and punctuation can be sent");
    }
    if v.len() > input.maxlength {
        return Err("that text is too long");
    }
    // chitin's own parser takes a lone `--word` argument as its flag wherever it stands: never hand it one
    if v.starts_with("--") && !v.contains(' ') {
        return Err("a lone word starting -- reads as a flag");
    }
    Ok(())
}

/// What an act runs, read from the act map's row alone: an ack state move, or a say with its one typed field. Free text never
/// reaches an ack: an ack, defer or decline that carries an `input` runs nothing.
#[derive(Debug, PartialEq)]
pub enum Run {
    Ack { state: &'static str, why: Option<&'static str> },
    Say(Input),
}

/// The act map row's run, if this server may run it: chitin's ack, defer and decline (Bobby's boundary 42184457) and, since Bobby
/// 07c5f8dc widened it, chitin's say with a typed line. Every other pair runs nothing.
pub fn run_of(entry: &Value) -> Option<Run> {
    let s = |k: &str| entry.get(k).and_then(Value::as_str).unwrap_or("");
    let (cap, call) = (s("cap"), s("call"));
    if let Some((state, why)) = state_of(cap, call) {
        return entry.get("input").is_none().then_some(Run::Ack { state, why });
    }
    (cap == "chitin" && call == "say").then(|| input_of(entry).map(Run::Say)).flatten()
}

pub fn hex(b: &[u8]) -> String {
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

pub(crate) const B64URL: &[u8; 64] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789-_";

/// base64url with no padding (RFC 4648 §5): a token's MAC, here and in report.rs.
pub(crate) fn b64url(b: &[u8]) -> String {
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
    /// Say `text` to the seat `to` (in reply to `in_reply_to`, when the act names one), by the act `act`. The route has checked
    /// `text` against the act's input ([`typed`]) and spent the form's token, so one form sends once.
    fn say(&self, to: &str, text: &str, in_reply_to: Option<&str>, act: &str) -> Result<Done, String>;
    /// What waits for `title`'s signature, as `chitin pending --as <title> --json` lists it (src/needs.rs filters it). Read for
    /// `needs.json`; an actor that can't read it says why. A verdict on one (approve, decline) is the passkey's, never this server's.
    fn pending(&self, title: &str) -> Result<serde_json::Value, String> {
        let _ = title;
        Err("this server reads no pending list".into())
    }
    /// One Needs-you record for chitin's notify-sink to push (`chitin notify --as <seat> --severity attention --dedupe <key> --url <url>
    /// <title>`, BRIEF addendum 2): src/needs.rs `notice` gives the three. Its Web Push sender (VAPID, quiet hours, one push per
    /// item ever) is chitin's, not this server's.
    fn notify(&self, seat: &str, title: &str, url: &str, dedupe: &str) -> Result<(), String> {
        let _ = (seat, title, url, dedupe);
        Err("this server sends no notifications".into())
    }
    /// One of chitin's push subscription verbs (`chitin push vapid-public | subscribe | unsubscribe <endpoint>`): its stdout's first line.
    fn push(&self, op: &Push) -> Result<String, String> {
        let _ = op;
        Err("this server keeps no push subscriptions".into())
    }
}

/// A push subscription verb, as chitin's `push` takes it. The subscription is the browser's PushSubscription JSON, cut to
/// `{endpoint, keys: {p256dh, auth}}`; chitin checks it (an https endpoint on a known push service, a P-256 point) and keeps it 0600.
/// Subscribe and Unsubscribe carry the device's `Tailscale-User-Login` (the second field), passed as `--login`: chitin maps it to the
/// stable tailnet user id it keys the subscription to, and refuses a call without one (chitin 7cb3da5, vapid-self-mint addendum 2.1).
#[derive(Debug, Clone, PartialEq)]
pub enum Push {
    Key,
    /// (the subscription JSON, the login)
    Subscribe(String, String),
    /// (the endpoint, the login)
    Unsubscribe(String, String),
}

/// The one chitin seat a typed say speaks as (Bobby 38e765d3, signed ebe199a4: S2 42184457 widened to typed say "as seat
/// fold-act"). It is never `--act-seat`, which names the ack's seat, so a say can't speak as Bobby or as any seat the flag names.
pub const SAY_SEAT: &str = "fold-act";

#[cfg(not(target_arch = "wasm32"))]
/// The real actor: `chitin ack <msg_id> <state> --to <sender> --as <seat> --ref fold:<act id> [--why <why>]`, the reason fixed per
/// state (work-chitin-author's ack delegate, via personal-assistant d46b12c1); the key from the environment chitin reads
/// (`CHITIN_KEY_FILE`, set by fold-serve.nix). `written …` is done; a noop chitin reports (`already …`, `… is final`) is done
/// before; anything else is chitin's one-line reason.
pub struct Chitin {
    pub bin: String,
    /// The seat an ack moves state as (`--act-seat`). A say ignores it and speaks as [`SAY_SEAT`].
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
        let (ok, line) = first_line(&mut c)?;
        let noop = line.contains("already ") || line.ends_with(" is final") || line.contains("does not follow");
        match (ok, noop) {
            (_, true) => Ok(Done::Already),
            (true, false) if line.starts_with("written") => Ok(Done::Ok),
            _ => Err(line),
        }
    }

    /// `chitin say --as fold-act [--in-reply-to <id>] --meta ref=fold:<act id> <to> <text>`: always as [`SAY_SEAT`], never `seat`;
    /// the text one argument, last, never a lone `--word` ([`typed`]). `written` or `held` (kept for the seat) is sent; anything
    /// else is chitin's one-line reason.
    fn say(&self, to: &str, text: &str, in_reply_to: Option<&str>, act: &str) -> Result<Done, String> {
        let mut c = Command::new(&self.bin);
        c.args(["say", "--as", SAY_SEAT]);
        if let Some(r) = in_reply_to {
            c.args(["--in-reply-to", r]);
        }
        c.args(["--meta", &format!("ref=fold:{act}"), to, text]);
        let (ok, line) = first_line(&mut c)?;
        if ok && (line.starts_with("written") || line.starts_with("held")) {
            Ok(Done::Ok)
        } else {
            Err(line)
        }
    }

    /// `chitin pending --as <title> --json`: its stdout, a JSON list.
    fn pending(&self, title: &str) -> Result<serde_json::Value, String> {
        let out = Command::new(&self.bin)
            .args(["pending", "--as", title, "--json"])
            .output()
            .map_err(|e| format!("could not run chitin: {e}"))?;
        if !out.status.success() {
            let e = String::from_utf8_lossy(&out.stderr);
            return Err(e.lines().map(str::trim).find(|l| !l.is_empty()).unwrap_or("chitin pending failed").chars().take(200).collect());
        }
        serde_json::from_slice(&out.stdout).map_err(|e| format!("chitin pending did not answer JSON: {e}"))
    }

    /// `chitin notify --as <seat> --severity attention --dedupe <key> --url <url> <title>`: `written`, or `suppressed: <key> was
    /// sent …` (sent already), is done; anything else, the hourly cap's `suppressed: … over …` among it, is chitin's reason, so
    /// the record is tried again at the next look.
    fn notify(&self, seat: &str, title: &str, url: &str, dedupe: &str) -> Result<(), String> {
        let mut c = Command::new(&self.bin);
        c.args(["notify", "--as", seat, "--severity", "attention", "--dedupe", dedupe, "--url", url, title]);
        let (ok, line) = first_line(&mut c)?;
        if (ok && line.starts_with("written")) || (line.starts_with("suppressed: ") && line.contains(" was sent less than ")) {
            Ok(())
        } else {
            Err(line)
        }
    }

    /// `chitin push vapid-public | subscribe --login <login> (the subscription on stdin) | unsubscribe --login <login> <endpoint>`.
    fn push(&self, op: &Push) -> Result<String, String> {
        use std::io::Write;
        let mut c = Command::new(&self.bin);
        match op {
            Push::Key => c.args(["push", "vapid-public"]),
            Push::Subscribe(_, login) => c.args(["push", "subscribe", "--login", login]),
            Push::Unsubscribe(e, login) => c.args(["push", "unsubscribe", "--login", login, e]),
        };
        c.stdin(std::process::Stdio::piped()).stdout(std::process::Stdio::piped()).stderr(std::process::Stdio::piped());
        let mut child = c.spawn().map_err(|e| format!("could not run chitin: {e}"))?;
        if let (Push::Subscribe(sub, _), Some(mut i)) = (op, child.stdin.take()) {
            i.write_all(sub.as_bytes()).map_err(|e| format!("could not hand chitin the subscription: {e}"))?;
        }
        let out = child.wait_with_output().map_err(|e| format!("chitin did not finish: {e}"))?;
        let (o, e) = (String::from_utf8_lossy(&out.stdout), String::from_utf8_lossy(&out.stderr));
        let line: String = o.lines().chain(e.lines()).map(str::trim).find(|l| !l.is_empty()).unwrap_or("").chars().take(200).collect();
        match (out.status.success(), op) {
            (true, Push::Subscribe(..)) if line != "subscribed" => Err(line),
            (true, _) => Ok(line),
            _ => Err(if line.is_empty() { format!("chitin exited {}", out.status) } else { line }),
        }
    }

}

#[cfg(not(target_arch = "wasm32"))]
/// Run chitin: whether it exited 0, and its first line (stdout, then stderr), cut to 200 characters; its exit status if silent.
fn first_line(c: &mut Command) -> Result<(bool, String), String> {
    let out = c.output().map_err(|e| format!("could not run chitin: {e}"))?;
    let (o, e) = (String::from_utf8_lossy(&out.stdout), String::from_utf8_lossy(&out.stderr));
    let line: String = o.lines().chain(e.lines()).map(str::trim).find(|l| !l.is_empty()).unwrap_or("").chars().take(200).collect();
    Ok((out.status.success(), if line.is_empty() { format!("chitin exited {}", out.status) } else { line }))
}
