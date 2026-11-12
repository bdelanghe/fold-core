//! `basis`, the fold engine as a command for a seat (src/basis.rs; RECOMMEND-consultant-2026-11-01-basis-cli-plugin, slices 1
//! and 2; Bobby 089bd610). It is not `fold` because `/usr/bin/fold` (coreutils) is first on a Mac's PATH.
//!
//! Reads (they write nothing):
//! - `basis read [<path>]... [--doc <file>] [--json]`: each fold as text (src/basis.rs), or with `--json` its accessibility
//!   tree; no path is the root.
//! - `basis needs [--doc <file>] [--json]`: `basis read "Needs you"`.
//! - `--needs <file>` (either): Needs you read live, as the viewer reads it (src/needs.rs): the file is `fold serve`'s
//!   `needs.json` answer (`{as, waiting}`; chitin's own `pending --json` list is refused, as it names no title to read for), so `Needs you/Approvals/<item>` is the same path here as on the page. Without it, the
//!   document's own sign items are shown as its producer wrote them.
//!
//! The document is `--doc`, else `$BASIS_DOC`, else `~/.local/state/fold-data/fleet/page.json` (fleet-map's page). Times read
//! in `$TZ`, else the zone `/etc/localtime` names.
//!
//! Writes. basis keeps no state of its own: a message goes through chitin as the calling seat, and an ask is a row of the file
//! fleet-map reads. The seat is `--as`, else `$BASIS_SEAT`; chitin is `--chitin`, else `$BASIS_CHITIN`, else `chitin` on PATH.
//! - `basis say <to> <text> [--in-reply-to <id>] [--ref <path>]`: `chitin say --as <seat> --meta ref=basis[:<path>]`.
//!   An approve-shaped text (`Approve:`, `Sign:`, …) is refused, because signing stays on /approve; use `basis ask --intent`.
//! - `basis ack <msg id> acting|declined|deferred|done [--why <text>] [--ref <artifact>]`: `chitin ack --as <seat>`.
//! - `basis ask --topic <topic> --item <text> --why <text> (--intent <12 hex> | --link <href> [--label <text>]) [--pr <pr>]
//!   [--thread <n>] [--asks <file>]`: checks the row (src/basis.rs `ask_row`) and adds it to `--asks`, else `$FM_ASKS`, else
//!   `~/.lobby/ops/needs-you/asks.json`, under a lock and by rename, so a reader never sees half a file. It prints the row's id.

use std::collections::HashMap;
use std::fs::{self, File};
use std::io::Write;
use std::path::Path;
use std::process::{exit, Command};

use fold_core::basis::{add_ask, approve_shaped, ask_row, Ask, Doc};
use serde_json::Value;

const USAGE: &str = "usage: basis read [<path>]... [--doc <file>] [--needs <needs.json>] [--json]
       basis needs [--doc <file>] [--needs <needs.json>] [--json]
       basis say <to> <text> [--as <seat>] [--in-reply-to <id>] [--ref <path>]
       basis ack <msg id> acting|declined|deferred|done [--as <seat>] [--why <text>] [--ref <artifact>]
       basis ask --topic <topic> --item <text> --why <text> (--intent <12 hex> | --link <href> [--label <text>])
                 [--as <seat>] [--pr <pr>] [--thread <n>] [--asks <file>]";

/// Each verb's options that take a value; `--json` is the one flag.
fn takes(cmd: &str) -> &'static [&'static str] {
    match cmd {
        "read" | "needs" => &["--doc", "--needs"],
        "say" => &["--as", "--chitin", "--in-reply-to", "--ref"],
        "ack" => &["--as", "--chitin", "--why", "--ref"],
        "ask" => &["--as", "--topic", "--item", "--why", "--intent", "--link", "--label", "--pr", "--thread", "--asks"],
        _ => &[],
    }
}

fn die(why: &str, code: i32) -> ! {
    eprintln!("{why}");
    exit(code)
}

fn zone() -> Option<String> {
    std::env::var("TZ").ok().filter(|z| !z.is_empty()).or_else(|| {
        let l = fs::read_link("/etc/localtime").ok()?;
        let s = l.to_string_lossy();
        s.split_once("zoneinfo/").map(|(_, z)| z.to_string())
    })
}

fn home() -> String {
    std::env::var("HOME").unwrap_or_else(|_| die("basis: no $HOME", 2))
}

fn main() {
    let args: Vec<String> = std::env::args().skip(1).collect();
    let cmd = args.first().map(String::as_str).unwrap_or("");
    if matches!(cmd, "-h" | "--help") {
        die(USAGE, 0);
    }
    if takes(cmd).is_empty() {
        die(USAGE, 2);
    }
    let (mut opt, mut json, mut pos): (HashMap<&str, String>, bool, Vec<String>) = (HashMap::new(), false, vec![]);
    let mut it = args.iter().skip(1);
    while let Some(a) = it.next() {
        match a.as_str() {
            "--json" if matches!(cmd, "read" | "needs") => json = true,
            "-h" | "--help" => die(USAGE, 0),
            o if o.starts_with("--") => {
                let k = takes(cmd).iter().find(|k| **k == o).unwrap_or_else(|| die(&format!("basis {cmd}: no option {o}\n{USAGE}"), 2));
                let v = it.next().unwrap_or_else(|| die(&format!("basis {cmd}: {o} needs a value"), 2));
                opt.insert(k, v.clone());
            }
            p => pos.push(p.to_string()),
        }
    }
    match cmd {
        "read" | "needs" => read(cmd, &opt, json, pos),
        "say" | "ack" => chitin(cmd, &opt, &pos),
        _ => ask(&opt, &pos),
    }
}

fn read(cmd: &str, opt: &HashMap<&str, String>, json: bool, mut paths: Vec<String>) {
    if cmd == "needs" {
        if !paths.is_empty() {
            die("basis needs: takes no path (basis read <path>)", 2);
        }
        paths.push("Needs you".into());
    }
    if paths.is_empty() {
        paths.push(String::new());
    }
    let file = opt
        .get("--doc")
        .cloned()
        .or_else(|| std::env::var("BASIS_DOC").ok())
        .unwrap_or_else(|| format!("{}/.local/state/fold-data/fleet/page.json", home()));
    let text = fs::read_to_string(&file).unwrap_or_else(|e| die(&format!("basis: {file}: {e}"), 1));
    let root: Value = serde_json::from_str(&text).unwrap_or_else(|e| die(&format!("basis: {file}: not JSON: {e}"), 1));
    let live = opt.get("--needs").cloned();
    let root = match live {
        None => root,
        Some(f) => {
            let t = std::fs::read_to_string(&f).unwrap_or_else(|e| die(&format!("basis: {f}: {e}"), 1));
            let n: Value = serde_json::from_str(&t).unwrap_or_else(|e| die(&format!("basis: {f}: not JSON: {e}"), 1));
            if fold_core::needs::read_answer(&n).is_none() {
                die(&format!("basis: {f}: not a needs.json answer ({{\"as\": <title>, \"waiting\": [...]}}, as fold serve's needs.json gives it)"), 1);
            }
            let now = std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).map(|d| d.as_millis() as i64).unwrap_or(0);
            fold_core::needs::overlay_case(&serde_json::json!({ "root": root, "needs": n, "now": now }))
                .unwrap_or_else(|e| die(&format!("basis: {f}: {e}"), 1))
        }
    };
    let d = Doc::new(&root, zone()).unwrap_or_else(|e| die(&format!("basis: {file}: {e}"), 1));
    let mut out = vec![];
    for p in &paths {
        let path = d.resolve(p).unwrap_or_else(|e| die(&format!("basis {cmd}: {e}"), 1));
        out.push(if json { serde_json::to_string_pretty(&d.ax(&path)).unwrap_or_default() + "\n" } else { d.text(&path) });
    }
    print!("{}", out.join("\n"));
}

fn seat(cmd: &str, opt: &HashMap<&str, String>) -> String {
    opt.get("--as")
        .cloned()
        .or_else(|| std::env::var("BASIS_SEAT").ok())
        .filter(|s| !s.trim().is_empty())
        .unwrap_or_else(|| die(&format!("basis {cmd}: whose? --as <seat> or $BASIS_SEAT"), 2))
}

/// `basis say` and `basis ack`: the one chitin call, as the seat, its exit status basis's.
fn chitin(cmd: &str, opt: &HashMap<&str, String>, pos: &[String]) {
    let who = seat(cmd, opt);
    let bin = opt.get("--chitin").cloned().or_else(|| std::env::var("BASIS_CHITIN").ok()).unwrap_or_else(|| "chitin".into());
    let mut c = Command::new(&bin);
    match (cmd, pos) {
        ("say", [to, text]) => {
            if approve_shaped(text) {
                die("basis say: that asks for a signature, and signing stays on /approve: basis ask --intent <id> puts it under Needs you", 1);
            }
            let r = opt.get("--ref").map_or("ref=basis".to_string(), |p| format!("ref=basis:{p}"));
            c.args(["say", "--as", &who, "--meta", &r]);
            if let Some(id) = opt.get("--in-reply-to") {
                c.args(["--in-reply-to", id]);
            }
            c.args([to, text]);
        }
        ("ack", [id, state]) => {
            if !matches!(state.as_str(), "acting" | "declined" | "deferred" | "done") {
                die(&format!("basis ack: {state}: acting, declined, deferred or done"), 2);
            }
            c.args(["ack", id, state, "--as", &who]);
            for k in ["--why", "--ref"] {
                if let Some(v) = opt.get(k) {
                    c.args([k, v.as_str()]);
                }
            }
        }
        ("say", _) => die("basis say: <to> <text>, the text one argument (quote it)", 2),
        _ => die("basis ack: <msg id> acting|declined|deferred|done", 2),
    }
    let st = c.status().unwrap_or_else(|e| die(&format!("basis {cmd}: {bin}: {e}"), 1));
    exit(st.code().unwrap_or(1));
}

/// Now as ISO 8601 UTC to the second, from the clock alone (no zone lookup).
fn now() -> String {
    let s = std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).map(|d| d.as_secs() as i64).unwrap_or(0);
    chrono::DateTime::from_timestamp(s, 0).map(|t| t.format("%Y-%m-%dT%H:%M:%SZ").to_string()).unwrap_or_default()
}

fn ask(opt: &HashMap<&str, String>, pos: &[String]) {
    if !pos.is_empty() {
        die(&format!("basis ask: {}: every field is an option\n{USAGE}", pos[0]), 2);
    }
    let get = |k: &str| opt.get(k).cloned().unwrap_or_default();
    if opt.contains_key("--label") && !opt.contains_key("--link") {
        die("basis ask: --label names a --link", 2);
    }
    let a = Ask {
        seat: seat("ask", opt),
        topic: get("--topic"),
        item: get("--item"),
        why: get("--why"),
        intent: opt.get("--intent").cloned(),
        link: opt.get("--link").map(|h| (h.clone(), opt.get("--label").cloned().unwrap_or_else(|| "Open".into()))),
        pr: opt.get("--pr").cloned(),
        thread: opt.get("--thread").map(|t| t.parse().unwrap_or_else(|_| die(&format!("basis ask: --thread {t}: a number"), 2))),
    };
    let row = ask_row(&a, &now()).unwrap_or_else(|e| die(&format!("basis ask: {e}"), 2));
    let file = opt
        .get("--asks")
        .cloned()
        .or_else(|| std::env::var("FM_ASKS").ok())
        .unwrap_or_else(|| format!("{}/.lobby/ops/needs-you/asks.json", home()));
    let path = Path::new(&file);
    let dir = path.parent().filter(|d| !d.as_os_str().is_empty()).unwrap_or(Path::new("."));
    let fail = |e: std::io::Error| -> ! { die(&format!("basis ask: {file}: {e}"), 1) };
    // One writer at a time: the lock is a file beside the asks, so the rename below never replaces it.
    let lock = File::create(dir.join(".asks.lock")).unwrap_or_else(|e| fail(e));
    lock.lock().unwrap_or_else(|e| fail(e));
    let before = match fs::read_to_string(path) {
        Ok(t) => Some(t),
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => None,
        Err(e) => fail(e),
    };
    let id = row["id"].as_str().unwrap_or("").to_string();
    let after = add_ask(before.as_deref(), row).unwrap_or_else(|e| die(&format!("basis ask: {e}"), 1));
    let tmp = dir.join(format!(".asks.{}.tmp", std::process::id()));
    let mut f = File::create(&tmp).unwrap_or_else(|e| fail(e));
    f.write_all(after.as_bytes()).and_then(|_| f.sync_all()).unwrap_or_else(|e| fail(e));
    fs::rename(&tmp, path).unwrap_or_else(|e| fail(e));
    println!("{id}");
}
