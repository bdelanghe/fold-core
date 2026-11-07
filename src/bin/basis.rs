//! `basis`, the fold engine as a command for a seat (src/basis.rs; RECOMMEND-consultant-2026-11-01-basis-cli-plugin, slice 1).
//! It is not `fold` because `/usr/bin/fold` (coreutils) is first on a Mac's PATH.
//!
//! - `basis read [<path>]... [--doc <file>] [--json]`: each fold as text (src/basis.rs), or with `--json` its accessibility
//!   tree; no path is the root.
//! - `basis needs [--doc <file>] [--json]`: `basis read "Needs you"`.
//! - `--needs <file>` (either): Needs you read live, as the viewer reads it (src/needs.rs): the file is `fold serve`'s
//!   `needs.json` answer (`{as, waiting}`; chitin's own `pending --json` list is refused, as it names no title to read for), so `Needs you/Approvals/<item>` is the same path here as on the page. Without it, the
//!   document's own sign items are shown as its producer wrote them.
//! - `--live [--as <title>] [--push] [--chitin <bin>]` (instead of `--needs`): the same answer, made here from `chitin pending --as
//!   <title> --json` (`bobby` by default; the binary is `--chitin`, else `$BASIS_CHITIN`, else `chitin`) by the code fold serve's
//!   needs.json runs (needs::live_answer), so the tree is the page's. `--push` is fold serve's notify flag (the page's notification acts).
//!
//! The document is `--doc`, else `$BASIS_DOC`, else `~/.local/state/fold-data/fleet/page.json` (fleet-map's page). Times read
//! in `$TZ`, else the zone `/etc/localtime` names. basis writes nothing.

use std::process::exit;

use fold_core::basis::Doc;
use serde_json::Value;

const USAGE: &str = "usage: basis read [<path>]... [--doc <file>] [--needs <needs.json> | --live [--as <title>] [--push] [--chitin <bin>]] [--json]\n       basis needs [--doc <file>] [--needs <needs.json> | --live [--as <title>] [--push] [--chitin <bin>]] [--json]";

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
    let (mut doc, mut json, mut paths, mut live) = (None, false, vec![], None);
    let (mut lively, mut title, mut push, mut chitin) = (false, "bobby".to_string(), false, None);
    let mut it = args.iter().skip(1);
    while let Some(a) = it.next() {
        match a.as_str() {
            "--live" => lively = true,
            "--push" => push = true,
            "--as" => title = it.next().unwrap_or_else(|| die("basis: --as needs a title", 2)).clone(),
            "--chitin" => chitin = Some(it.next().unwrap_or_else(|| die("basis: --chitin needs a binary", 2)).clone()),
            "--doc" => doc = Some(it.next().unwrap_or_else(|| die("basis: --doc needs a file", 2)).clone()),
            "--json" => json = true,
            "--needs" => live = Some(it.next().unwrap_or_else(|| die("basis: --needs needs a file", 2)).clone()),
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
    if lively && live.is_some() {
        die("basis: --live and --needs <file> are two ways to the same answer: give one", 2);
    }
    // --live: the answer fold serve's needs.json gives, made here from `chitin pending --as <title> --json` by the same code
    // (needs::live_answer), so what this reads is what the page reads. --push is the server's notify flag (the page's notification acts).
    let answer: Option<(String, Value)> = if lively {
        use fold_core::act::Actor;
        let bin = chitin.or_else(|| std::env::var("BASIS_CHITIN").ok()).unwrap_or_else(|| "chitin".to_string());
        let pending = fold_core::act::Chitin { bin, seat: String::new() }.pending(&title).unwrap_or_else(|e| die(&format!("basis: chitin did not list what waits for {title}: {e}"), 1));
        Some((format!("chitin pending --as {title}"), fold_core::needs::live_answer(&pending, &title, push)))
    } else {
        live.map(|f| {
            let t = std::fs::read_to_string(&f).unwrap_or_else(|e| die(&format!("basis: {f}: {e}"), 1));
            let n: Value = serde_json::from_str(&t).unwrap_or_else(|e| die(&format!("basis: {f}: not JSON: {e}"), 1));
            if fold_core::needs::read_answer(&n).is_none() {
                die(&format!("basis: {f}: not a needs.json answer ({{\"as\": <title>, \"waiting\": [...]}}, as fold serve's needs.json gives it)"), 1);
            }
            (f, n)
        })
    };
    let root = match answer {
        None => root,
        Some((from, n)) => {
            let now = std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).map(|d| d.as_millis() as i64).unwrap_or(0);
            fold_core::needs::overlay_case(&serde_json::json!({ "root": root, "needs": n, "now": now }))
                .unwrap_or_else(|e| die(&format!("basis: {from}: {e}"), 1))
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
