//! Reporter tokens for Clax (src/report.rs, basis-clax goal close condition 3): the token's round trip and every way it is refused,
//! the config that never holds a key, and `POST report-token` (src/serve.rs) minting only for the one login, same-origin, through
//! tailscale serve. The Clax queue and its grant are stubbed: a fixed key and clock stand in for the Fleet vault's grant.

use fold_core::act::Bad;
use fold_core::report::{check, mint, Config, Reporting, MIN_KEY, TTL};
use fold_core::serve::{answer, Request, Site, REPORT_TOKEN};
use serde_json::Value;

const NOW: i64 = 1_793_361_600;
const KEY: [u8; 32] = [5; 32];
const ME: &str = "bobby@example.com";
const HOST: &str = "phobos.example.ts.net";
const QUEUE: &str = "basis";
const OWNER: &str = "personal-allie-lead";

#[test]
fn a_token_holds_for_its_queue_owner_and_login_until_it_lapses() {
    let t = mint(&KEY, QUEUE, OWNER, ME, NOW);
    assert!(t.starts_with(&format!("r1.{}.", NOW + TTL)), "{t}");
    assert_eq!(check(&KEY, &t, QUEUE, OWNER, ME, NOW), Ok(NOW + TTL));
    assert_eq!(check(&KEY, &t, QUEUE, OWNER, ME, NOW + TTL - 1), Ok(NOW + TTL));
    assert_eq!(check(&KEY, &t, QUEUE, OWNER, ME, NOW + TTL), Err(Bad::Expired));
    // anything it was not minted for is a forgery, never a lapse
    assert_eq!(check(&[6; 32], &t, QUEUE, OWNER, ME, NOW), Err(Bad::Forged));
    assert_eq!(check(&KEY, &t, "other", OWNER, ME, NOW), Err(Bad::Forged));
    assert_eq!(check(&KEY, &t, QUEUE, "someone-else", ME, NOW), Err(Bad::Forged));
    assert_eq!(check(&KEY, &t, QUEUE, OWNER, "eve@example.com", NOW), Err(Bad::Forged));
    // a later expiry written in by hand does not carry the MAC with it
    let mac = t.rsplit('.').next().unwrap();
    assert_eq!(check(&KEY, &format!("r1.{}.{mac}", NOW + 10 * TTL), QUEUE, OWNER, ME, NOW), Err(Bad::Forged));
    let (cut, r2, letter) = (t[..t.len() - 1].to_string(), format!("r2{}", &t[2..]), t.replacen("r1.", "r1.x", 1));
    for bad in ["", "r1", "r1..", "v1.1.2.3", cut.as_str(), r2.as_str(), letter.as_str()] {
        assert_eq!(check(&KEY, bad, QUEUE, OWNER, ME, NOW), Err(Bad::Malformed), "{bad:?}");
    }
    // a token for acts (act.rs) is not one for reports
    assert_eq!(check(&KEY, &fold_core::act::mint(&KEY, "a", &"0".repeat(64), ME, NOW), QUEUE, OWNER, ME, NOW), Err(Bad::Malformed));
}

const CONFIG: &str = r#"{"login":"bobby@example.com","host":"phobos.example.ts.net","clax":"http://localhost:7480/","queue":"basis",
  "owner":"personal-allie-lead","grant_cmd":["op","read","op://Fleet/clax basis grant/credential"]}"#;

#[test]
fn the_config_names_a_grant_command_and_never_holds_a_key() {
    let c = Config::parse(CONFIG).unwrap();
    assert_eq!((c.clax.as_str(), c.queue.as_str(), c.owner.as_str()), ("http://localhost:7480", QUEUE, OWNER));
    assert_eq!(c.grant_cmd, ["op", "read", "op://Fleet/clax basis grant/credential"]);
    let seen = std::cell::RefCell::new(vec![]);
    let r = Config::parse(CONFIG)
        .unwrap()
        .with_grant(&|argv: &[String]| -> Result<Vec<u8>, String> {
            seen.borrow_mut().push(argv.to_vec());
            let mut k = vec![b'k'; 40];
            k.push(b'\n');
            Ok(k)
        })
        .unwrap();
    assert_eq!(r.key, vec![b'k'; 40], "the trailing newline is not the key's");
    assert_eq!(seen.borrow().len(), 1, "the grant is read once");
    // too short a grant, and a grant command that fails, are refused
    let short = Config::parse(CONFIG).unwrap().with_grant(&|_: &[String]| -> Result<Vec<u8>, String> { Ok(vec![1; MIN_KEY - 1]) });
    assert!(short.is_err_and(|e| e.contains("fewer than 32")));
    let failed =
        Config::parse(CONFIG).unwrap().with_grant(&|_: &[String]| -> Result<Vec<u8>, String> { Err("op: not signed in".to_string()) });
    assert!(failed.is_err_and(|e| e == "op: not signed in"));
    // a key kept in the file is refused, whatever its case
    for k in ["key", "Grant", "secret", "token", "PASSWORD"] {
        let with = CONFIG.replacen('{', &format!("{{\"{k}\":\"abc\","), 1);
        assert!(Config::parse(&with).is_err_and(|e| e.contains("never kept in a file")), "{k}");
    }
    // every field is needed; clax is an origin
    for k in ["login", "host", "clax", "queue", "owner", "grant_cmd"] {
        let mut v: Value = serde_json::from_str(CONFIG).unwrap();
        v.as_object_mut().unwrap().remove(k);
        assert!(Config::parse(&v.to_string()).is_err_and(|e| e.contains(k)), "{k}");
    }
    for clax in ["localhost:7480", "http://", "http://localhost:7480/q/basis", "ftp://x", "http://u@x"] {
        let mut v: Value = serde_json::from_str(CONFIG).unwrap();
        v["clax"] = clax.into();
        assert!(Config::parse(&v.to_string()).is_err_and(|e| e.contains("not an http(s) origin")), "{clax}");
    }
    let mut v: Value = serde_json::from_str(CONFIG).unwrap();
    v["grant_cmd"] = serde_json::json!("op read x");
    assert!(Config::parse(&v.to_string()).is_err(), "grant_cmd is an argv, never a shell line");
}

fn site(reporting: bool) -> Site {
    let root = std::env::temp_dir().join(format!("fold-report-{}", std::process::id()));
    let reporting = reporting.then(|| Reporting {
        login: ME.into(),
        host: HOST.into(),
        clax: "http://localhost:7480".into(),
        queue: QUEUE.into(),
        owner: OWNER.into(),
        key: KEY.to_vec(),
    });
    Site { root, mount: "/fold".into(), no_store: vec![], acting: None, reporting }
}

/// A request as tailscale serve hands one on from ME's own page, with `change` applied (None drops a header).
fn from_me(method: &str, change: &[(&str, Option<&str>)]) -> Request {
    let mut headers: Vec<(String, String)> = [
        ("Tailscale-User-Login", ME),
        ("X-Forwarded-For", "100.64.0.2"),
        ("X-Forwarded-Proto", "https"),
        ("X-Forwarded-Host", HOST),
        ("Sec-Fetch-Site", "same-origin"),
    ]
    .iter()
    .map(|(k, v)| (k.to_string(), v.to_string()))
    .collect();
    for (k, v) in change {
        headers.retain(|(n, _)| n != k);
        if let Some(v) = v {
            headers.push((k.to_string(), v.to_string()));
        }
    }
    Request { method: method.into(), target: REPORT_TOKEN.into(), headers, body: vec![], peer: Some("127.0.0.1".parse().unwrap()) }
}

#[test]
fn report_token_mints_only_for_the_one_login_same_origin() {
    let s = site(true);
    let (res, log) = answer(&s, &from_me("POST", &[]), NOW);
    assert_eq!(res.status, 200);
    assert_eq!(res.header("Cache-Control"), Some("no-store"));
    assert_eq!(res.header("Content-Type"), Some("application/json"));
    assert_eq!(res.header("ETag"), None, "a token is never revalidated or kept");
    let body: Value = serde_json::from_slice(&res.body).unwrap();
    assert_eq!((body["clax"].as_str(), body["queue"].as_str(), body["owner"].as_str()), (Some("http://localhost:7480"), Some(QUEUE), Some(OWNER)));
    assert_eq!(body["exp"], NOW + TTL);
    let token = body["token"].as_str().unwrap();
    assert_eq!(check(&KEY, token, QUEUE, OWNER, ME, NOW), Ok(NOW + TTL), "what the route hands out is what a Clax with the grant takes");
    let log = log.unwrap();
    assert_eq!(log["result"], "minted");
    assert!(!log.to_string().contains(token), "the log never holds a token");

    // An older browser: no Sec-Fetch-Site, the right Origin.
    let older = from_me("POST", &[("Sec-Fetch-Site", None), ("Origin", Some("https://phobos.example.ts.net"))]);
    assert_eq!(answer(&s, &older, NOW).0.status, 200);

    let mut far = from_me("POST", &[]);
    far.peer = Some("10.0.0.9".parse().unwrap());
    let refusals: Vec<(&str, Request, &str)> = vec![
        ("not from loopback", far, "not from the tailnet front"),
        ("no login", from_me("POST", &[("Tailscale-User-Login", None)]), "not who this server acts for"),
        ("another login", from_me("POST", &[("Tailscale-User-Login", Some("eve@example.com"))]), "not who this server acts for"),
        ("a Q-encoded login", from_me("POST", &[("Tailscale-User-Login", Some("=?utf-8?q?bobby?="))]), "unexpected login encoding"),
        ("not through serve", from_me("POST", &[("X-Forwarded-Proto", None)]), "not through tailscale serve"),
        ("another host", from_me("POST", &[("X-Forwarded-Host", Some("evil.example"))]), "not through tailscale serve"),
        ("cross-site", from_me("POST", &[("Sec-Fetch-Site", Some("cross-site"))]), "cross-site"),
        ("same-site is not same-origin", from_me("POST", &[("Sec-Fetch-Site", Some("same-site"))]), "cross-site"),
        ("typed into the bar", from_me("POST", &[("Sec-Fetch-Site", Some("none"))]), "cross-site"),
        (
            "same-origin with another Origin",
            from_me("POST", &[("Origin", Some("https://evil.example"))]),
            "cross-site",
        ),
        ("no Sec-Fetch-Site and no Origin", from_me("POST", &[("Sec-Fetch-Site", None)]), "cross-site"),
    ];
    for (what, r, why) in refusals {
        let (res, log) = answer(&s, &r, NOW);
        assert_eq!(res.status, 403, "{what}");
        assert_eq!(String::from_utf8_lossy(&res.body).trim(), why, "{what}");
        assert_eq!(log.unwrap()["result"], "refused:403", "{what}");
    }
    // only POST: a GET could be prefetched, cached or led to by a link
    for m in ["GET", "HEAD", "PUT"] {
        let (res, log) = answer(&s, &from_me(m, &[]), NOW);
        assert_eq!((res.status, res.header("Allow"), log.is_none()), (405, Some("POST"), true), "{m}");
    }
    // without --report-config the route says reporting is off, and mints nothing
    let (res, log) = answer(&site(false), &from_me("POST", &[]), NOW);
    assert_eq!((res.status, log.is_none()), (404, true));
    assert_eq!(String::from_utf8_lossy(&res.body).trim(), "reporting to Clax isn't on here");
}
