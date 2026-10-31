//! fold's two processes (brief item 27), each on 127.0.0.1 for tailscale serve to put at its mount, and neither as root:
//!
//! - `fold serve --root <dir> --mount </fold> --port <n> [--no-store <path prefix>]...`: one directory, read-only (src/serve.rs).
//!   It replaces fold-serve's darkhttpd agents (brief item 24). With `--act-as <login> --act-host <host> --act-key <file> --chitin
//!   <bin> [--act-seat <seat>] [--act-log <file>]`, an act's form may ack, defer or decline a chitin message for that one login
//!   (S2: allie-s2-fold-act-write): the key is made if missing (0600), the seat is `fold-act` unless named, and each act's line goes
//!   to `--act-log`, by default `acts.jsonl` beside the key.
//! - `fold sink --port <n> --log <signals.jsonl> [--per-minute <n>]`: the pings and reports, append-only (src/sink.rs), put at
//!   `<mount>/_sink`.

use std::net::TcpListener;
use std::path::PathBuf;
use std::process::exit;

use fold_core::{act, serve, sink};

const USAGE: &str = "usage: fold serve --root <dir> --mount </fold> --port <n> [--no-store <prefix>]...\n                  [--act-as <login> --act-host <host> --act-key <file> --chitin <bin> [--act-seat <seat>] [--act-log <file>]]\n       fold sink --port <n> --log <file> [--per-minute <n>]";

extern "C" {
    fn geteuid() -> u32;
}

fn die(why: &str, code: i32) -> ! {
    eprintln!("{why}");
    exit(code)
}

fn num<T: std::str::FromStr>(cmd: &str, a: &str, v: &str) -> T {
    v.parse().unwrap_or_else(|_| die(&format!("fold {cmd}: {a} {v}: not a number"), 2))
}

fn main() {
    let args: Vec<String> = std::env::args().skip(1).collect();
    let cmd = args.first().map(String::as_str).unwrap_or("");
    if cmd != "serve" && cmd != "sink" {
        die(USAGE, 2);
    }
    // SAFETY: geteuid takes nothing and cannot fail (POSIX).
    if unsafe { geteuid() } == 0 {
        die(&format!("fold {cmd}: not as root"), 1);
    }
    let (mut root, mut mount, mut port, mut log, mut no_store, mut per_minute) = (None, String::new(), 0u16, None, vec![], 60u32);
    let (mut act_as, mut act_host, mut act_key, mut chitin, mut act_seat, mut act_log) =
        (None, None, None, None, "fold-act".to_string(), None);
    let mut it = args[1..].iter();
    while let Some(a) = it.next() {
        let v = it.next().unwrap_or_else(|| die(&format!("fold {cmd}: {a} needs a value"), 2));
        match (cmd, a.as_str()) {
            ("serve", "--root") => root = Some(PathBuf::from(v)),
            ("serve", "--mount") => mount = v.trim_end_matches('/').to_string(),
            ("serve", "--no-store") => no_store.push(v.clone()),
            ("serve", "--act-as") => act_as = Some(v.clone()),
            ("serve", "--act-host") => act_host = Some(v.clone()),
            ("serve", "--act-key") => act_key = Some(PathBuf::from(v)),
            ("serve", "--chitin") => chitin = Some(v.clone()),
            ("serve", "--act-seat") => act_seat = v.clone(),
            ("serve", "--act-log") => act_log = Some(PathBuf::from(v)),
            ("sink", "--log") => log = Some(PathBuf::from(v)),
            ("sink", "--per-minute") => per_minute = num(cmd, a, v),
            (_, "--port") => port = num(cmd, a, v),
            _ => die(&format!("fold {cmd}: no option {a}\n{USAGE}"), 2),
        }
    }
    let listener = TcpListener::bind(("127.0.0.1", port)).unwrap_or_else(|e| die(&format!("fold {cmd}: 127.0.0.1:{port}: {e}"), 1));
    let at = listener.local_addr().map(|a| a.to_string()).unwrap_or_default();
    let done = if cmd == "serve" {
        let root = root.unwrap_or_else(|| die("fold serve: --root is needed", 2));
        let (acting, log) = match (act_as, act_host, act_key, chitin) {
            (None, None, None, None) => (None, None),
            (Some(login), Some(host), Some(kf), Some(bin)) => {
                let key = act::key(&kf).unwrap_or_else(|e| die(&format!("fold serve: --act-key: {e}"), 1));
                let log = act_log.unwrap_or_else(|| kf.with_file_name("acts.jsonl"));
                eprintln!(
                    "fold serve: acting for {login} at https://{host}{mount}/act/ as chitin seat {act_seat}, logged to {}",
                    log.display()
                );
                let actor = Box::new(act::Chitin { bin, seat: act_seat });
                (Some(serve::Acting { login, host, key, actor }), Some(log))
            }
            _ => die("fold serve: --act-as, --act-host, --act-key and --chitin go together", 2),
        };
        let how = if acting.is_some() { "acting for one login" } else { "read-only" };
        eprintln!("fold serve: {} at {mount}/ on {at}, {how}", root.display());
        serve::run(listener, serve::Site { root, mount, no_store, acting }, log)
    } else {
        let log = log.unwrap_or_else(|| die("fold sink: --log is needed", 2));
        eprintln!("fold sink: appending to {} on {at}, {per_minute} a minute a viewer", log.display());
        sink::run(listener, per_minute, log)
    };
    if let Err(e) = done {
        die(&format!("fold {cmd}: {e}"), 1);
    }
}
