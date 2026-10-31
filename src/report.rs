//! Reporting to Clax from Basis (basis-clax goal, close condition 3): `fold serve` mints a reporter token, so the Clax overlay a
//! fold page embeds (`@empathic/clax-sdk`) can file a report about a fold into a Clax queue for the fold's owner. [`mint`] and
//! [`check`] are pure; [`Config`] is what `fold serve --report-config <file>` reads; serve.rs's `POST report-token` route hands a
//! token to the one login it reports for, same-origin only.
//!
//! - **Token:** `r1.<exp>.<mac>`, `mac` = base64url (no padding) of HMAC-SHA256(grant key, length-prefixed `"clax-report"`, queue,
//!   owner, exp, login). It names who may report, to which queue, for whom, until when; never what about: one token covers every
//!   row of the page, and each report's anchor carries the row's haecceity (`schema:about`, html.rs) itself.
//! - **Key:** the queue's service grant, read once at start from the stdout of the config's `grant_cmd` (the Fleet vault through
//!   the read-only op service account) and held in memory only. A config that holds a key itself is refused: the key is never in
//!   a file, least of all one in the repo.
//! - **Stub:** the Clax queue and its service grant do not exist yet (break-glass 3962fa0d). Until they do, a Clax that is not
//!   told the grant cannot check these tokens; the route, the token and the config are what this side owes, with tests.

use std::process::Command;

use serde_json::Value;

use crate::act::{b64url, hmac, Bad, B64URL};

/// A reporter token's life: an hour of an open page; the overlay asks again when it lapses.
pub const TTL: i64 = 3600;

/// The fewest key bytes a service grant may have.
pub const MIN_KEY: usize = 32;

/// What `fold serve` reports with: whom for, where to, and the grant that signs it.
pub struct Reporting {
    /// The one tailnet login that may report (tailscale serve's `Tailscale-User-Login`).
    pub login: String,
    /// The host the pages are served at: `https://<host>` is the one origin a token is handed to.
    pub host: String,
    /// The Clax origin the overlay files to (Bobby's local Clax, `http://localhost:7480`).
    pub clax: String,
    /// The Clax queue reports go to.
    pub queue: String,
    /// The fold's owner: the seat the queue's reports are for.
    pub owner: String,
    /// The queue's service grant key, in memory only.
    pub key: Vec<u8>,
}

fn mac(key: &[u8], queue: &str, owner: &str, exp: i64, login: &str) -> String {
    let mut m = vec![];
    for f in ["clax-report", queue, owner, &exp.to_string(), login] {
        m.extend((f.len() as u32).to_be_bytes());
        m.extend(f.as_bytes());
    }
    b64url(&hmac(key, &m))
}

/// A fresh reporter token for one queue, owner and login, good until `now` + [`TTL`].
pub fn mint(key: &[u8], queue: &str, owner: &str, login: &str, now: i64) -> String {
    let exp = now + TTL;
    format!("r1.{exp}.{}", mac(key, queue, owner, exp, login))
}

/// Whether a reporter token is this grant's, for this queue, owner and login, and current: its expiry when it is. Checked in act.rs's
/// order: shape, then the MAC (constant time), then the time. What a Clax that holds the grant runs on each report.
pub fn check(key: &[u8], token: &str, queue: &str, owner: &str, login: &str, now: i64) -> Result<i64, Bad> {
    let p: Vec<&str> = token.split('.').collect();
    let [v, exp, m] = p.as_slice() else { return Err(Bad::Malformed) };
    let num = !exp.is_empty() && exp.len() <= 12 && exp.bytes().all(|b| b.is_ascii_digit());
    if *v != "r1" || !num || m.len() != 43 || !m.bytes().all(|b| B64URL.contains(&b)) {
        return Err(Bad::Malformed);
    }
    let exp: i64 = exp.parse().map_err(|_| Bad::Malformed)?;
    let want = mac(key, queue, owner, exp, login);
    if want.bytes().zip(m.bytes()).fold(0u8, |d, (a, b)| d | (a ^ b)) != 0 {
        return Err(Bad::Forged);
    }
    if now >= exp {
        return Err(Bad::Expired);
    }
    Ok(exp)
}

/// `--report-config`'s file: `{login, host, clax, queue, owner, grant_cmd}`, every one required, `grant_cmd` an argv (no shell).
pub struct Config {
    pub login: String,
    pub host: String,
    pub clax: String,
    pub queue: String,
    pub owner: String,
    pub grant_cmd: Vec<String>,
}

/// Fields a config may never hold: a key in a file is the one thing this design rules out.
const SECRET: [&str; 5] = ["key", "grant", "secret", "token", "password"];

impl Config {
    /// The config's text, read: every field present and non-blank, `clax` an http(s) origin with no path, and no secret.
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
        let grant_cmd: Vec<String> = o
            .get("grant_cmd")
            .and_then(Value::as_array)
            .map(|a| a.iter().filter_map(Value::as_str).map(str::to_string).collect())
            .filter(|a: &Vec<String>| !a.is_empty() && a.iter().all(|x| !x.is_empty()))
            .ok_or("report config: \"grant_cmd\" is needed, an argv of strings")?;
        Ok(Config { login: s("login")?, host: s("host")?, clax, queue: s("queue")?, owner: s("owner")?, grant_cmd })
    }

    /// The config with its grant: `grant` runs `grant_cmd` and gives its stdout; one trailing newline is not part of the key.
    pub fn with_grant(self, grant: &dyn Fn(&[String]) -> Result<Vec<u8>, String>) -> Result<Reporting, String> {
        let mut key = grant(&self.grant_cmd)?;
        while key.last().is_some_and(|b| *b == b'\n' || *b == b'\r') {
            key.pop();
        }
        if key.len() < MIN_KEY {
            return Err(format!("report config: the grant is {} bytes, fewer than {MIN_KEY}", key.len()));
        }
        Ok(Reporting { login: self.login, host: self.host, clax: self.clax, queue: self.queue, owner: self.owner, key })
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
