//! Reporting to Clax from Basis (basis-clax goal, close condition 3; break-glass 420ff78a). The Clax overlay (`Clax.overlay`, clax main
//! d7812795 web/overlay) makes a non-extractable Ed25519 key in the page and asks its host's backend, `auth(publicKey)`, for a reporter
//! token for that key. Basis's backend is `fold serve`: [`Reporting`] is what it needs to answer, and serve.rs's routes use it:
//!
//! - `POST <mount>/report-token`, empty: `{instance, queue, owner}`, what the page mounts the overlay with.
//! - `POST <mount>/report-token`, `{"public_key": …}` (JSON): the token, minted by the Clax instance beside this server
//!   (`POST <upstream>/api/queues/<queue>/reporter-tokens`, authorised by the queue's service grant), as `{token, expires_at}`.
//! - `GET <mount>/clax/overlay.js`: the instance's `/_clax/overlay.js`, fetched over loopback and served from this origin, since
//!   the instance sends no CORS header on it and a pinned (SRI) script from another origin needs one. Nothing is copied into the
//!   repo: the bytes are the instance's, and the page's SRI digest pins them.
//!
//! Both report-token answers are only for the one login, same-origin, through tailscale serve (serve.rs `front`).
//!
//! - **Grant:** the queue's service grant is two secrets, its `cxq_` token and its Ed25519 private key (PKCS#8, unpadded
//!   base64url). Each is read once at start from the stdout of its command (`grant_cmd`, `grant_key_cmd`: the Fleet vault through
//!   the read-only op service account) and held in memory only. A config holding a key itself is refused, and neither is ever in an
//!   error, a log or an answer.
//! - **The mint** (clax main d7812795, crates/clax-server/src/routes/queue_reporter.rs, via work-clax-sdk-kit-goal b570ecc3):
//!   `POST /api/queues/<q>/reporter-tokens`, body `{reporter_id, origin, public_key}`, `Authorization: Clax-Queue <cxq_…>` and
//!   `Clax-Signature: t=…, n=…, s=…` (`clax-ed25519 v2`, Ed25519 over canonical lines). The answer is `{token, expires_at}` (a
//!   `cxr_` secret, 900 s), the overlay's `auth` result as is. A service call never carries Origin or a cookie.
//! - **Signed** with the grant's key (sig.rs: `clax-ed25519 v2`, held to Clax's own vector), the signer made at start so a bad key
//!   fails `fold serve`'s start. A [`ClaxMinter`] with no signer refuses before any connection, so nothing unsigned is ever sent.

use std::process::Command;

use serde_json::{json, Value};

use crate::http::{authority, call, loopback};
use crate::sig::{header, nonce, Ed25519, SignInput, Signer};

/// What `fold serve` reports with: whom for, which Clax, which queue, and the grant that lets it mint.
pub struct Reporting {
    /// The one tailnet login that may report (tailscale serve's `Tailscale-User-Login`).
    pub login: String,
    /// The host the pages are served at: `https://<host>` is the one origin the routes answer.
    pub host: String,
    /// The Clax instance as the page reaches it (the overlay's `instance`): its tailnet URL, or localhost on the Mac.
    pub clax: String,
    /// The same instance as this server reaches it, on loopback (`http://127.0.0.1:7480`).
    pub upstream: String,
    /// The Clax queue reports go to.
    pub queue: String,
    /// The fold's owner: the seat the queue's reports are for.
    pub owner: String,
    /// What mints a reporter token: the instance, through the queue's grant ([`ClaxMinter`]); tests give a stub.
    pub minter: Box<dyn Minter>,
}

/// What a reporter token is: the instance's own answer, `{token, expires_at}` (`expires_at` a time stamp, read by the page).
#[derive(Debug, PartialEq)]
pub struct Minted {
    pub token: String,
    pub expires_at: String,
}

/// One reporter token for a page's key, for `origin` (the page's, which must be one of the queue's origins) and `reporter_id`.
pub trait Minter: Send + Sync {
    fn mint(&self, public_key: &str, origin: &str, reporter_id: &str) -> Result<Minted, String>;
}

/// What reads a service grant: given `grant_cmd`'s argv, its stdout, or why not. [`run_grant`] runs it; tests stub it.
pub type Grant<'a> = dyn Fn(&[String]) -> Result<Vec<u8>, String> + 'a;

/// `--report-config`'s file: `{login, host, clax, upstream, queue, owner, grant_cmd, grant_key_cmd}`, every one required, each
/// `_cmd` an argv (no shell): `grant_cmd` prints the grant's `cxq_` token, `grant_key_cmd` its private key.
pub struct Config {
    pub login: String,
    pub host: String,
    pub clax: String,
    pub upstream: String,
    pub queue: String,
    pub owner: String,
    pub grant_cmd: Vec<String>,
    pub grant_key_cmd: Vec<String>,
}

/// Fields a config may never hold: a key in a file is the one thing this design rules out.
const SECRET: [&str; 5] = ["key", "grant", "secret", "token", "password"];

/// A queue id as one URL path segment can hold it, unescaped.
fn plain(s: &str) -> bool {
    !s.is_empty() && s.len() <= 128 && s.bytes().all(|b| b.is_ascii_alphanumeric() || b"-_.".contains(&b))
}

impl Config {
    /// The config's text, read: every field present and non-blank, `clax` an http(s) origin, `upstream` an http origin on loopback,
    /// the queue a plain id, and no secret.
    pub fn parse(text: &str) -> Result<Config, String> {
        let v: Value = serde_json::from_str(text).map_err(|e| format!("report config: not JSON: {e}"))?;
        let o = v.as_object().ok_or("report config: not an object")?;
        if let Some(k) = o.keys().find(|k| SECRET.contains(&k.to_ascii_lowercase().as_str())) {
            return Err(format!("report config: \"{k}\" is a secret's field; the grant is read by grant_cmd, never kept in a file"));
        }
        let s = |k: &str| -> Result<String, String> {
            o.get(k)
                .and_then(Value::as_str)
                .map(str::trim)
                .filter(|x| !x.is_empty())
                .map(str::to_string)
                .ok_or_else(|| format!("report config: \"{k}\" is needed"))
        };
        let clax = s("clax")?.trim_end_matches('/').to_string();
        let rest = clax.strip_prefix("https://").or_else(|| clax.strip_prefix("http://")).unwrap_or("");
        if rest.is_empty() || rest.contains(['/', '?', '#', '@']) {
            return Err(format!("report config: clax {clax:?} is not an http(s) origin"));
        }
        let upstream = s("upstream")?.trim_end_matches('/').to_string();
        if loopback(&upstream).is_none() {
            return Err(format!("report config: upstream {upstream:?} is not an http origin on loopback"));
        }
        let queue = s("queue")?;
        if !plain(&queue) {
            return Err(format!("report config: queue {queue:?} is not a plain id"));
        }
        let argv = |k: &str| -> Result<Vec<String>, String> {
            o.get(k)
                .and_then(Value::as_array)
                .map(|a| a.iter().filter_map(Value::as_str).map(str::to_string).collect())
                .filter(|a: &Vec<String>| !a.is_empty() && a.iter().all(|x| !x.is_empty()))
                .ok_or_else(|| format!("report config: \"{k}\" is needed, an argv of strings"))
        };
        Ok(Config {
            login: s("login")?,
            host: s("host")?,
            clax,
            upstream,
            queue,
            owner: s("owner")?,
            grant_cmd: argv("grant_cmd")?,
            grant_key_cmd: argv("grant_key_cmd")?,
        })
    }

    /// The config with its grant: `read` runs each command and gives its stdout. Trailing line ends are not part of a secret, and
    /// what is left must be one printable token: the `cxq_` token (it goes in an Authorization header) and the base64url key.
    pub fn with_grant(self, read: &Grant<'_>) -> Result<Reporting, String> {
        let one = |argv: &[String], what: &str| -> Result<String, String> {
            let mut g = read(argv)?;
            while g.last().is_some_and(|b| *b == b'\n' || *b == b'\r') {
                g.pop();
            }
            if g.is_empty() || !g.iter().all(|b| b.is_ascii_graphic()) {
                return Err(format!("report config: the grant's {what} is empty or not one printable token"));
            }
            String::from_utf8(g).map_err(|_| format!("report config: the grant's {what} is not text"))
        };
        let token = one(&self.grant_cmd, "token")?;
        if !token.starts_with("cxq_") {
            return Err("report config: the grant's token is not a cxq_ token".into());
        }
        let key = one(&self.grant_key_cmd, "key")?;
        // The signer is made now, so a key that is not one fails fold serve's start, not a reader's first report.
        let signer = Ed25519::from_pkcs8_b64url(&key)?;
        let minter = ClaxMinter::new(self.upstream.clone(), self.queue.clone(), token, key).with_signer(Box::new(signer));
        Ok(Reporting {
            login: self.login,
            host: self.host,
            clax: self.clax,
            upstream: self.upstream,
            queue: self.queue,
            owner: self.owner,
            minter: Box::new(minter),
        })
    }
}

/// The real minter: the Clax instance beside this server, called on loopback with the queue's grant (the `cxq_` token and its
/// Ed25519 key, both in memory only; no Debug, so neither can be printed by accident).
pub struct ClaxMinter {
    pub upstream: String,
    pub queue: String,
    token: String,
    key: String,
    /// The Ed25519 signer made from `key`: none until fold-core has an Ed25519 crate (sig.rs), so the mint refuses.
    signer: Option<Box<dyn Signer>>,
}

impl ClaxMinter {
    /// The minter for one queue on the instance at `upstream` (loopback), with its grant's `cxq_` token and key.
    pub fn new(upstream: String, queue: String, token: String, key: String) -> ClaxMinter {
        ClaxMinter { upstream, queue, token, key, signer: None }
    }

    /// The same, signing with `signer` (tests give one; the real one comes with an Ed25519 crate).
    pub fn with_signer(self, signer: Box<dyn Signer>) -> ClaxMinter {
        ClaxMinter { signer: Some(signer), ..self }
    }

    /// The request a mint makes, before its signature: path, headers (no Origin, no cookie: a service call never carries
    /// them) and body.
    pub fn request(&self, public_key: &str, origin: &str, reporter_id: &str) -> (String, Vec<(String, String)>, String) {
        let path = format!("/api/queues/{}/reporter-tokens", self.queue);
        let body = json!({ "reporter_id": reporter_id, "origin": origin, "public_key": public_key }).to_string();
        let headers = vec![
            ("Authorization".to_string(), format!("Clax-Queue {}", self.token)),
            ("Content-Type".to_string(), "application/json".to_string()),
            ("Accept".to_string(), "application/json".to_string()),
        ];
        (path, headers, body)
    }

    /// `Clax-Signature` for a request (sig.rs, `clax-ed25519 v2`) at `t`, with `nonce`: over the path, the Host [`call`] sends, the
    /// body and the grant's token. With no signer it refuses, so nothing unsigned is sent.
    pub fn signature(&self, path: &str, body: &str, t: u64, nonce: &str) -> Result<String, String> {
        let signer = self.signer.as_deref().ok_or_else(|| {
            let _ = &self.key; // what the real signer is made from
            "clax-ed25519 v2 signing is not built in fold serve yet (no Ed25519 crate), so no reporter token can be minted".to_string()
        })?;
        let host = authority(&self.upstream).ok_or("the upstream is not on loopback")?;
        let input = SignInput { method: "POST", target: path, host: &host, t, nonce, body: body.as_bytes(), token: &self.token, agent: &[] };
        header(signer, &input)
    }
}

impl Minter for ClaxMinter {
    fn mint(&self, public_key: &str, origin: &str, reporter_id: &str) -> Result<Minted, String> {
        let (path, mut headers, body) = self.request(public_key, origin, reporter_id);
        let t = std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).map_err(|e| e.to_string())?.as_secs();
        let sig = self.signature(&path, &body, t, &nonce()?)?;
        headers.push(("Clax-Signature".to_string(), sig));
        let hs: Vec<(&str, &str)> = headers.iter().map(|(k, v)| (k.as_str(), v.as_str())).collect();
        let r = call(&self.upstream, "POST", &path, &hs, body.as_bytes())?;
        if !(200..300).contains(&r.status) {
            return Err(format!("clax answered {}: {}", r.status, said(&r.body, &[self.token.as_str(), self.key.as_str()])));
        }
        let v: Value = serde_json::from_slice(&r.body).map_err(|_| "clax's answer is not JSON".to_string())?;
        match (v["token"].as_str(), v["expires_at"].as_str()) {
            (Some(t), Some(e)) if !t.is_empty() && !e.is_empty() => Ok(Minted { token: t.into(), expires_at: e.into() }),
            _ => Err("clax's answer has no token and expires_at".into()),
        }
    }
}

/// The real grant reader: runs the argv (no shell) and takes its stdout; its stderr is not echoed, so a key cannot leak through it.
pub fn run_grant(argv: &[String]) -> Result<Vec<u8>, String> {
    let (bin, args) = argv.split_first().ok_or("report config: grant_cmd is empty")?;
    let out = Command::new(bin).args(args).output().map_err(|e| format!("report config: could not run {bin}: {e}"))?;
    if !out.status.success() {
        return Err(format!("report config: {bin} exited {}", out.status));
    }
    Ok(out.stdout)
}

/// A page's public key as the overlay sends it: base64 or base64url (padded or not), or hex, of a sensible length. Nothing else
/// is passed on to Clax.
pub fn public_key_ok(k: &str) -> bool {
    (16..=512).contains(&k.len()) && k.bytes().all(|b| b.is_ascii_alphanumeric() || b"+/=-_".contains(&b))
}

/// One line of an upstream's answer, safe to show: printable, short, and never holding a secret.
fn said(body: &[u8], secrets: &[&str]) -> String {
    let t: String = String::from_utf8_lossy(body).chars().filter(|c| !c.is_control()).take(200).collect();
    if secrets.iter().any(|s| t.contains(s)) {
        "(its answer held a secret, so it is not shown)".into()
    } else {
        t
    }
}

impl Reporting {
    /// The overlay script as the instance serves it (`/_clax/overlay.js`), its bytes as they are.
    pub fn overlay(&self) -> Result<Vec<u8>, String> {
        let r = call(&self.upstream, "GET", "/_clax/overlay.js", &[("Accept-Encoding", "identity")], b"")?;
        if r.status != 200 {
            return Err(format!("clax answered {} for its overlay", r.status));
        }
        Ok(r.body)
    }
}
