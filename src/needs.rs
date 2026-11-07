//! Needs you, live (BRIEF-stint-2026-11-02-approve-into-fold; RECOMMEND-consultant-2026-11-01-reversible-no-approval-and-sign-in-basis
//! S3, S5): what waits for a person's signature, read from chitin when it is asked for (`fold serve`'s `needs.json`, src/serve.rs),
//! never from the page the producer wrote minutes ago. One function puts it into a document ([`overlay`]), so the viewer, which
//! folds it in the browser, and `basis` (`--needs <file>`) read the same folds at the same paths: `Needs you/Approvals/<item>`.
//!
//! - **Waiting:** each intent `chitin pending --as <title> --json` lists that is queued as the title, neither declined nor expired,
//!   with a 12-hex id (chitin's pending.ts `ID`), a recipient and its text ([`waiting`]). The text shown here is for reading it;
//!   what is signed is shown again from chitin-approve's own challenge, the answer the passkey signs (viewer/sign.js).
//! - **An item** is sign.ts's (home programs/fleet-map), so its fold has the producer's id (`needs/sign-<id>`), with two acts, both
//!   the passkey (`webauthn`), each a verdict on the item (BRIEF addendum 1, Bobby: "we should add a decline with passkey in the
//!   approval flow"): Approve (`approve`, the item's first act: Enter and the rotor's first action run it) and Decline (`decline`,
//!   its second). chitin-approve binds each assertion to its verdict and the item's h, so one can't be replayed as the other. Both
//!   are named by the intent alone ([`intent_of`]); the endpoints are built from that id, never read from a document.
//! - **The overlay** takes the producer's sign items out of the Needs-you section and puts the live ones first, in a titled group,
//!   "Approvals" (a fold of its own: its heading is the rotor's group heading, `]]`/`[[` step to it). When the live read failed, the
//!   group says why and holds nothing to sign, so a stale page never offers a signature. The other items stay where the producer
//!   put them: no group is invented from an id's prefix.
//! - **Notifications** (BRIEF addendum 2, Bobby: "can this page have notifications"): when `fold serve` sends them (its answer's
//!   `push: true`), Needs you itself gets two acts, "Notify me on this device" and "Stop notifying this device" (`push`/`subscribe`,
//!   `push`/`unsubscribe`), which the viewer takes. What is pushed for a new item is [`notice`]: its one line and its fold's path.

use serde_json::{json, Map, Value};

/// The id of the section the overlay writes into (home fleet_map.ts's Needs you).
pub const SECTION: &str = "needs";

/// What Approvals says when the live read failed. Why it failed is a notice, the viewer's to say in its notices zone.
pub const UNREAD: &str = "Nothing can be signed from here right now.";

/// The live group's title.
pub const APPROVALS: &str = "Approvals";

/// The prefix of a sign item's id, as sign.ts writes it.
pub const ITEM: &str = "needs/sign-";

/// An intent id as chitin's pending.ts takes one: 12 lowercase hex digits. Nothing else reaches an argv or a URL.
pub fn intent_ok(id: &str) -> bool {
    id.len() == 12 && id.bytes().all(|b| b.is_ascii_digit() || (b'a'..=b'f').contains(&b))
}

/// chitin-approve's endpoints for one verdict on one intent: the challenge (GET) and where its assertion is posted. Approve is its
/// PROTOCOL.md's `/approve/v1/challenge/<id>` and `…/assertion/<id>`; decline is addendum 1's `/approve/v1/decline/<id>`, "the same
/// challenge/assertion flow", so GET for its challenge and POST for its assertion at the one path. viewer/sign.js `ENDPOINTS` is
/// the same table.
pub fn endpoints(id: &str, verdict: &str) -> Option<(String, String)> {
    match verdict {
        "approve" => Some((format!("/approve/v1/challenge/{id}"), format!("/approve/v1/assertion/{id}"))),
        "decline" => Some((format!("/approve/v1/decline/{id}"), format!("/approve/v1/decline/{id}"))),
        _ => None,
    }
}

/// The two verdicts a Needs-you item takes, in its acts' order: Enter runs the first.
pub const VERDICTS: [(&str, &str); 2] = [("approve", "Approve with passkey"), ("decline", "Decline with passkey")];

fn text<'a>(o: &'a Value, k: &str) -> Option<&'a str> {
    o.get(k).and_then(Value::as_str)
}

/// The intents that wait for `title`, each `{id, as, declined: false, expired: false, to, text, queued_by?, text_hash?, created_at?, age_s?}`
/// (chitin's own shape, cut to what is shown): `pending` is `chitin pending --json`'s list, or `needs.json`'s `waiting`, read again
/// here so nothing a reader is handed can widen it.
pub fn waiting(pending: &Value, title: &str) -> Vec<Value> {
    let Some(list) = pending.as_array() else { return vec![] };
    list.iter()
        .filter(|p| !title.is_empty() && text(p, "as") == Some(title))
        .filter(|p| p.get("declined") == Some(&Value::Bool(false)) && p.get("expired") == Some(&Value::Bool(false)))
        .filter_map(|p| {
            let id = text(p, "id").filter(|i| intent_ok(i))?;
            let to = text(p, "to").filter(|t| !t.trim().is_empty())?;
            let said = text(p, "text")?;
            let mut m = Map::new();
            m.insert("id".into(), id.into());
            m.insert("as".into(), title.into());
            m.insert("declined".into(), false.into());
            m.insert("expired".into(), false.into());
            m.insert("to".into(), to.into());
            m.insert("text".into(), said.into());
            for k in ["queued_by", "text_hash", "created_at"] {
                if let Some(v) = text(p, k) {
                    m.insert(k.into(), v.into());
                }
            }
            if let Some(n) = p.get("age_s").filter(|n| n.as_f64().is_some_and(|f| f.is_finite() && f >= 0.0)) {
                m.insert("age_s".into(), n.clone());
            }
            Some(Value::Object(m))
        })
        .collect()
}

/// `new Date(ms).toISOString()`.
fn iso_ms(ms: i64) -> Option<String> {
    chrono::DateTime::from_timestamp_millis(ms).map(|t| t.format("%Y-%m-%dT%H:%M:%S%.3fZ").to_string())
}

/// One waiting intent as a Needs-you item, sign.ts's shape, at `now_ms`.
pub fn item(w: &Value, now_ms: i64) -> Value {
    let id = text(w, "id").unwrap_or("");
    let by = text(w, "queued_by").unwrap_or("an unknown seat");
    let hash = text(w, "text_hash").map(|h| format!(", text hash {h}")).unwrap_or_default();
    let mut m = Map::new();
    m.insert("id".into(), format!("{ITEM}{id}").into());
    m.insert("name".into(), format!("To {}", text(w, "to").unwrap_or("")).into());
    m.insert("nameKind".into(), "agent".into());
    m.insert("text".into(), text(w, "text").unwrap_or("").into());
    m.insert("quoted".into(), true.into());
    m.insert("why".into(), format!("Waits for your signature. Queued by {by}; intent {id}{hash}.").into());
    m.insert("action".into(), "Approve or decline it with your passkey.".into());
    // when it was queued: chitin's own time, so the item reads the same on every read; else its age at `now_ms`, as sign.ts
    let queued =
        text(w, "created_at").and_then(|c| chrono::DateTime::parse_from_rfc3339(c).ok()).and_then(|t| iso_ms(t.timestamp_millis()));
    if let Some(at) = queued.or_else(|| w.get("age_s").and_then(Value::as_f64).and_then(|s| iso_ms(now_ms - (s * 1000.0).round() as i64))) {
        m.insert("at".into(), at.into());
    }
    let act = |(verdict, label): (&str, &str)| {
        let (c, a) = endpoints(id, verdict).unwrap_or_default();
        json!({ "cap": "webauthn", "call": verdict, "label": label, "args": { "challenge": c, "assertion": a } })
    };
    m.insert("act".into(), act(VERDICTS[0]));
    m.insert("acts".into(), json!([act(VERDICTS[1])]));
    Value::Object(m)
}

/// The intent an act is on, and its verdict, when it is one this module makes: the passkey (`webauthn`, or `approve` as sign.ts's
/// older cap) whose call is a verdict ([`VERDICTS`]; an `approve` cap's call may be `sign`) and whose args are exactly that
/// verdict's two endpoints for one intent. None for anything else, so no other act's args can point the ceremony anywhere.
pub fn intent_of(act: &Value) -> Option<(String, &'static str)> {
    let cap = text(act, "cap")?.trim();
    if cap != "webauthn" && cap != "approve" {
        return None;
    }
    let verdict = match text(act, "call").map(str::trim) {
        Some("decline") => "decline",
        Some("approve") | Some("sign") => "approve",
        _ => return None,
    };
    let args = act.get("args")?;
    let c = text(args, "challenge")?;
    let id = c.rsplit('/').next().filter(|i| intent_ok(i))?;
    (endpoints(id, verdict)? == (c.to_string(), text(args, "assertion")?.to_string())).then(|| (id.to_string(), verdict))
}

/// What the live read gave: the waiting intents, or why there are none to show.
pub enum Live<'a> {
    Read(&'a [Value]),
    Failed(&'a str),
}

fn is_sign_item(v: &Value) -> bool {
    text(v, "id").is_some_and(|i| i.starts_with(ITEM))
}

/// `root` with its Needs-you section's sign items replaced by `live`'s, first, in the titled group [`APPROVALS`]. A document with
/// no Needs-you section is returned as it is.
pub fn overlay(root: &Value, live: Live, now_ms: i64) -> Value {
    overlay_with(root, live, now_ms, false)
}

/// The two acts Needs you gets when this server sends notifications: subscribe this device, and stop.
pub const PUSH_ACTS: [(&str, &str); 2] = [("subscribe", "Notify me on this device"), ("unsubscribe", "Stop notifying this device")];

/// [`overlay`], and with `push` the section's two notification acts ([`PUSH_ACTS`]).
pub fn overlay_with(root: &Value, live: Live, now_ms: i64, push: bool) -> Value {
    let mut root = root.clone();
    let Some(sections) = root.get_mut("sections").and_then(Value::as_array_mut) else { return root };
    let Some(section) = sections.iter_mut().find(|s| text(s, "id") == Some(SECTION)) else { return root };
    let Some(sec) = section.as_object_mut() else { return root };
    if let Some(Value::Array(items)) = sec.get_mut("items") {
        items.retain(|i| !is_sign_item(i));
    }
    let mut groups = match sec.remove("groups") {
        Some(Value::Array(g)) => g,
        _ => vec![],
    };
    for g in groups.iter_mut() {
        if let Some(Value::Array(items)) = g.get_mut("items") {
            items.retain(|i| !is_sign_item(i));
        }
    }
    // an untitled group left with nothing is layout only, and an empty one says nothing: dropped
    groups.retain(|g| {
        text(g, "title").is_some_and(|t| !t.trim().is_empty()) || g.get("items").and_then(Value::as_array).is_none_or(|i| !i.is_empty())
    });
    let approvals = match live {
        Live::Read([]) => None,
        Live::Read(w) => Some(json!({ "title": APPROVALS, "items": w.iter().map(|w| item(w, now_ms)).collect::<Vec<_>>() })),
        // why is a notice, said in the viewer's notices zone (index.html), never in the fold: the fold only says what is so
        Live::Failed(_) => Some(json!({ "title": APPROVALS, "items": [], "empty": UNREAD })),
    };
    groups.splice(0..0, approvals);
    sec.insert("groups".into(), groups.into());
    if push {
        let acts: Vec<Value> =
            PUSH_ACTS.iter().map(|(call, label)| json!({ "cap": "push", "call": call, "label": label, "args": {} })).collect();
        sec.insert("acts".into(), acts.into());
    }
    root
}

/// `{root, needs?, error?, now}` (the viewer's live read, as wasm `needs_json` takes it: `needs` is `needs.json`'s answer, `error`
/// why there is none, `now` the time in ms) as the overlaid root. No answer and no error reads as failed: never the producer's list.
pub fn overlay_case(case: &Value) -> Result<Value, String> {
    let root = case.get("root").ok_or("no root")?;
    let now = case.get("now").and_then(Value::as_f64).ok_or("now: the time in ms is needed")? as i64;
    let w = case.get("needs").and_then(read_answer);
    let push = case.get("needs").and_then(|n| n.get("push")) == Some(&Value::Bool(true));
    Ok(match (text(case, "error"), w) {
        (Some(e), _) => overlay(root, Live::Failed(e), now),
        (None, Some(w)) => overlay_with(root, Live::Read(&w), now, push),
        (None, None) => overlay(root, Live::Failed("the live list was not given"), now),
    })
}

/// `needs.json`'s answer: `{as, waiting, push}`, `push` whether this server sends notifications (its subscribe routes are on).
pub fn answer(title: &str, waiting: &[Value], push: bool) -> Value {
    json!({ "as": title, "waiting": waiting, "push": push })
}

/// `needs.json` as `fold serve` answers it for `title`, from `chitin pending --as <title> --json`'s list: the one place that list becomes the
/// answer, shared by the route (src/serve.rs) and `basis needs --live`, so the CLI reads what the page reads.
pub fn live_answer(pending: &Value, title: &str, push: bool) -> Value {
    answer(title, &waiting(pending, title), push)
}

/// What is pushed for one waiting intent, as a `chitin notify` record chitin's notify-sink turns into a Web Push (its webpush.ts
/// `needsYou`): the item's one line (its fold's title, `To <recipient>`: never its text, which may say more than a lock screen
/// should), the item's fold as a path on `host` under the site's `mount`, with no query or fragment
/// (`https://<host><mount>/needs/approvals/needs-sign-<id>/`;
/// the service worker opens it as the viewer's `#/needs/approvals/needs-sign-<id>`), and `needs-you:sign-<id>`, the key chitin
/// pushes once per item, ever. None for an intent that isn't one ([`intent_ok`]) or a host that isn't a plain name.
pub fn notice(w: &Value, host: &str, mount: &str) -> Option<(String, String, String)> {
    let id = text(w, "id").filter(|i| intent_ok(i))?;
    let to: String = text(w, "to")?.chars().filter(|c| !c.is_control()).take(80).collect();
    if host.is_empty() || !host.bytes().all(|b| b.is_ascii_alphanumeric() || b == b'.' || b == b'-') {
        return None;
    }
    let mount = mount.trim_end_matches('/');
    if !mount.starts_with('/') || !mount[1..].split('/').all(|s| !s.is_empty() && s != "." && s != ".." && s.bytes().all(|b| b.is_ascii_alphanumeric() || b == b'-' || b == b'_')) {
        return None;
    }
    // the fold id fold() makes of the item's id (lib.rs `slug`: `/` becomes `-`)
    let fold = format!("{}{id}", ITEM.replace('/', "-"));
    Some((
        format!("To {to}"),
        format!("https://{host}{mount}/{SECTION}/{}/{fold}/", APPROVALS.to_lowercase()),
        format!("needs-you:sign-{id}"),
    ))
}

/// A `needs.json` answer read back (`basis --needs`, the viewer): the waiting list, filtered again for its own `as`.
pub fn read_answer(v: &Value) -> Option<Vec<Value>> {
    let title = text(v, "as")?;
    Some(waiting(v.get("waiting")?, title))
}
