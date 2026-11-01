//! `basis`, the fold engine as a command for a seat (src/basis.rs; RECOMMEND-consultant-2026-11-01-basis-cli-plugin, slice 1).
//! It is not `fold` because `/usr/bin/fold` (coreutils) is first on a Mac's PATH.
//!
//! - `basis read [<path>]... [--doc <file>] [--json]`: each fold as text (src/basis.rs), or with `--json` its accessibility
//!   tree; no path is the root.
//! - `basis needs [--doc <file>] [--json]`: `basis read "Needs you"`.
//!
//! The document is `--doc`, else `$BASIS_DOC`, else `~/.local/state/fold-data/fleet/page.json` (fleet-map's page). Times read
//! in `$TZ`, else the zone `/etc/localtime` names. basis writes nothing.

use std::process::exit;

use fold_core::basis::Doc;
use serde_json::Value;

const USAGE: &str = "usage: basis read [<path>]... [--doc <file>] [--json]\n       basis needs [--doc <file>] [--json]";

fn die(why: &str, code: i32) -> ! {
    eprintln!("{why}");
    exit(code)
}

fn zone() -> Option<String> {
    std::env::var("TZ").ok().filter(|z| !z.is_empty()).or_else(|| {
        let l = std::fs::read_link("/etc/localtime").ok()?;
        let s = l.to_string_lossy();
        s.split_once("zoneinfo/").map(|(_, z)| z.to_string())
    })
}

fn main() {
    let args: Vec<String> = std::env::args().skip(1).collect();
    let cmd = args.first().map(String::as_str).unwrap_or("");
    let (mut doc, mut json, mut paths) = (None, false, vec![]);
    let mut it = args.iter().skip(1);
    while let Some(a) = it.next() {
        match a.as_str() {
            "--doc" => doc = Some(it.next().unwrap_or_else(|| die("basis: --doc needs a file", 2)).clone()),
            "--json" => json = true,
            "-h" | "--help" => die(USAGE, 0),
            o if o.starts_with("--") => die(&format!("basis {cmd}: no option {o}\n{USAGE}"), 2),
            p => paths.push(p.to_string()),
        }
    }
    match cmd {
        "read" => {}
        "needs" if paths.is_empty() => paths.push("Needs you".into()),
        "needs" => die("basis needs: takes no path (basis read <path>)", 2),
        _ => die(USAGE, 2),
    }
    if paths.is_empty() {
        paths.push(String::new());
    }
    let file = doc.or_else(|| std::env::var("BASIS_DOC").ok()).unwrap_or_else(|| {
        let home = std::env::var("HOME").unwrap_or_else(|_| die("basis: no $HOME, so no default document (--doc)", 2));
        format!("{home}/.local/state/fold-data/fleet/page.json")
    });
    let text = std::fs::read_to_string(&file).unwrap_or_else(|e| die(&format!("basis: {file}: {e}"), 1));
    let root: Value = serde_json::from_str(&text).unwrap_or_else(|e| die(&format!("basis: {file}: not JSON: {e}"), 1));
    let d = Doc::new(&root, zone()).unwrap_or_else(|e| die(&format!("basis: {file}: {e}"), 1));
    let mut out = vec![];
    for p in &paths {
        let path = d.resolve(p).unwrap_or_else(|e| die(&format!("basis {cmd}: {e}"), 1));
        out.push(if json { serde_json::to_string_pretty(&d.ax(&path)).unwrap_or_default() + "\n" } else { d.text(&path) });
    }
    print!("{}", out.join("\n"));
}
