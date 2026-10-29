//! The JavaScript string and date behaviour fold.ts leans on, reproduced: UTF-16 lengths and cuts, JS's `\s`/`trim()` whitespace
//! set, `.` stopping at line terminators, and `toLocaleString("en-US", {weekday: "short", hour: "2-digit", minute: "2-digit",
//! hour12: false})` as bun (JavaScriptCore) prints it ("Wed 00:05": midnight is 00, not 24).

use chrono::{Datelike, Duration, NaiveDate, NaiveDateTime, NaiveTime, Timelike};

use crate::tz::Zone;

pub fn is_ws_char(c: char) -> bool {
    matches!(
        c,
        '\t' | '\n' | '\x0B' | '\x0C' | '\r' | ' ' | '\u{A0}' | '\u{1680}' | '\u{2000}'
            ..='\u{200A}' | '\u{2028}' | '\u{2029}' | '\u{202F}' | '\u{205F}' | '\u{3000}' | '\u{FEFF}'
    )
}

/// JS `\s` on one UTF-16 code unit (every JS whitespace is in the BMP).
pub fn is_ws(u: u16) -> bool {
    char::from_u32(u as u32).is_some_and(is_ws_char)
}

/// What JS `.` does not match.
pub fn is_line_terminator(u: u16) -> bool {
    matches!(u, 0x0A | 0x0D | 0x2028 | 0x2029)
}

/// JS `String.prototype.trim`.
pub fn js_trim(s: &str) -> &str {
    s.trim_matches(is_ws_char)
}

pub fn split_ws(s: &str) -> impl Iterator<Item = &str> {
    s.split(is_ws_char)
}

/// JS `s.length`: UTF-16 code units.
pub fn len16(s: &str) -> usize {
    s.encode_utf16().count()
}

/// fold.ts `clip`: past `n` code units, cut at a word boundary and mark the cut with "…".
/// A cut through a surrogate pair leaves a lone surrogate in JS; Rust strings cannot hold one, so it becomes U+FFFD here.
pub fn clip(s: &str, n: usize) -> String {
    let u: Vec<u16> = s.encode_utf16().collect();
    if u.len() <= n {
        return s.to_string();
    }
    let cut = &u[..n.saturating_sub(1)];
    // .replace(/\s+\S*$/, ""): drop the last whitespace run and what follows it, when there is one.
    let mut j = cut.len();
    while j > 0 && !is_ws(cut[j - 1]) {
        j -= 1;
    }
    let end = if j == 0 {
        cut.len()
    } else {
        while j > 0 && is_ws(cut[j - 1]) {
            j -= 1;
        }
        j
    };
    String::from_utf16_lossy(&cut[..end]) + "…"
}

/// `.replace(/^"|"$/g, "")`.
pub fn strip_quotes(s: &str) -> String {
    let t = s.strip_prefix('"').unwrap_or(s);
    t.strip_suffix('"').unwrap_or(t).to_string()
}

/// The ASCII digits at the start of `s`.
fn digits(s: &str) -> &str {
    &s[..s.bytes().take_while(u8::is_ascii_digit).count()]
}

/// JS `\s+` at the start of `s`, greedy: the rest after it.
fn skip_ws(s: &str) -> &str {
    s.trim_start_matches(is_ws_char)
}

/// `/^(#{1,6})\s+(.*)$/` on one line: the level and the heading's words (`.` stops at line terminators, so a line with one
/// after its words is no heading).
pub fn heading(line: &str) -> Option<(usize, &str)> {
    let n = line.bytes().take_while(|&b| b == b'#').count();
    let rest = &line[n..];
    let words = skip_ws(rest);
    let ok = (1..=6).contains(&n) && words.len() < rest.len() && !words.contains(['\n', '\r', '\u{2028}', '\u{2029}']);
    ok.then_some((n, words))
}

/// `/^\s*([-*]|\d+\.)\s+/`: a list item's words after its marker.
pub fn list_item(line: &str) -> Option<&str> {
    let s = skip_ws(line);
    let after = match s.as_bytes().first()? {
        b'-' | b'*' => &s[1..],
        _ => s[digits(s).len()..].strip_prefix('.').filter(|_| !digits(s).is_empty())?,
    };
    let words = skip_ws(after);
    (words.len() < after.len()).then_some(words)
}

/// `.replace(/\s*\([^)]*\)$/, "")`: a trailing parenthetical off. The leftmost match: the first `(` after the last `)` but one,
/// with the whitespace run before it.
pub fn drop_paren(s: &str) -> String {
    let Some(body) = s.strip_suffix(')') else { return s.to_string() };
    let from = body.rfind(')').map_or(0, |i| i + 1);
    let Some(open) = body[from..].find('(').map(|i| from + i) else { return s.to_string() };
    s[..open].trim_end_matches(is_ws_char).to_string()
}

/// `new RegExp(`/a/${self}(?:[/?#]|$)`).test(href)`. fold.ts does not escape `self` either; here it is literal, which is the same
/// for an artifact id (letters and digits).
pub fn self_link(me: &str, href: &str) -> bool {
    let at = format!("/a/{me}");
    href.match_indices(&at).any(|(i, m)| href[i + m.len()..].chars().next().is_none_or(|c| matches!(c, '/' | '?' | '#')))
}

/// The zones `when` reads in: the page's `meta.timeZone`, else the machine's (the caller's `local_time_zone`), else UTC. A zone
/// the build does not know reads as UTC.
pub struct Clock {
    show: Zone,
    local: Zone,
}

impl Clock {
    pub fn new(page: Option<&str>, local: Option<&str>) -> Clock {
        let local = local.and_then(Zone::named).unwrap_or(Zone::Utc);
        Clock { show: page.and_then(Zone::named).unwrap_or_else(|| local.clone()), local }
    }
}

/// `new Date(iso)` for the ISO-like forms JavaScriptCore reads: `YYYY[-MM[-DD]]` (UTC) and a date with a time after `T`, `t` or
/// a space, with seconds and a fraction optional, then `Z`, `±HH:MM` or `±HHMM` (no offset: the local zone):
/// `/^(\d{4})(?:-(\d{2})(?:-(\d{2}))?)?(?:[Tt ](\d{2}):(\d{2})(?::(\d{2})(?:\.\d+)?)?(Z|z|[+-]\d{2}:?\d{2})?)?$/`.
fn parse(iso: &str) -> Option<NaiveDateTime> {
    // `n` ASCII digits, as a number, and what follows them
    fn num(s: &str, n: usize) -> Option<(u32, &str)> {
        (digits(s).len() >= n).then(|| (s[..n].parse().ok(), &s[n..])).and_then(|(v, r)| Some((v?, r)))
    }
    let (y, mut s) = num(iso, 4)?;
    let (mut mo, mut d) = (1, 1);
    if let Some(r) = s.strip_prefix('-') {
        (mo, s) = num(r, 2)?;
        if let Some(r) = s.strip_prefix('-') {
            (d, s) = num(r, 2)?;
        }
    }
    let date = NaiveDate::from_ymd_opt(y as i32, mo, d)?;
    let Some(r) = s.strip_prefix(['T', 't', ' ']) else {
        return s.is_empty().then(|| date.and_time(NaiveTime::MIN));
    };
    let (h, r) = num(r, 2)?;
    let (mi, mut s) = num(r.strip_prefix(':')?, 2)?;
    let mut se = 0;
    if let Some(r) = s.strip_prefix(':') {
        (se, s) = num(r, 2)?;
        if let Some(r) = s.strip_prefix('.') {
            let f = digits(r);
            if f.is_empty() {
                return None;
            }
            s = &r[f.len()..];
        }
    }
    let at = if h == 24 && mi == 0 && se == 0 {
        NaiveDateTime::new(date, NaiveTime::MIN) + Duration::days(1)
    } else {
        NaiveDateTime::new(date, NaiveTime::from_hms_opt(h, mi, se)?)
    };
    let off = match s {
        // No offset: read as UTC here; parse_in moves it into the local zone.
        "" | "Z" | "z" => 0,
        _ => {
            let sign = if s.starts_with('-') { -1 } else { 1 };
            let (oh, r) = num(s.strip_prefix(['+', '-'])?, 2)?;
            let (om, r) = num(r.strip_prefix(':').unwrap_or(r), 2)?;
            if !r.is_empty() || oh > 23 || om > 59 {
                return None;
            }
            sign * (oh as i64 * 60 + om as i64)
        }
    };
    Some(at - Duration::minutes(off))
}

/// `new Date(iso)` as a UTC time.
fn parse_in(iso: &str, clock: &Clock) -> Option<NaiveDateTime> {
    let utc = parse(iso)?;
    // Without an offset, a date with a time is local time.
    let zoned = iso.len() > 10 && !iso.ends_with(['Z', 'z']) && !has_offset(iso);
    Some(if zoned { clock.local.to_utc(&utc) } else { utc })
}

fn has_offset(iso: &str) -> bool {
    let t = &iso[10..];
    t.contains('+') || t.contains('-')
}

/// fold.ts `when`: an ISO time as "Tue 14:44" in the page's zone; anything else as it is.
pub fn when(iso: &str, clock: &Clock) -> String {
    match parse_in(iso, clock) {
        Some(d) => {
            let d = clock.show.local(&d);
            format!(
                "{} {:02}:{:02}",
                ["Mon", "Tue", "Wed", "Thu", "Fri", "Sat", "Sun"][d.weekday().num_days_from_monday() as usize],
                d.hour(),
                d.minute()
            )
        }
        None => iso.to_string(),
    }
}

/// `/^part-\d+$/`: the id of a named run.
pub fn is_run_id(id: &str) -> bool {
    id.strip_prefix("part-").is_some_and(|d| !d.is_empty() && d.bytes().all(|b| b.is_ascii_digit()))
}

/// `new Date(iso).toLocaleString([], {dateStyle: "medium", timeStyle: "short"})` in en-US as bun prints it ("Oct 27, 2026 at
/// 9:05 AM"), in the machine's zone (the clock's local zone); what `new Date` cannot read is "Invalid Date".
pub fn medium(iso: &str, clock: &Clock) -> String {
    const MONTHS: [&str; 12] = ["Jan", "Feb", "Mar", "Apr", "May", "Jun", "Jul", "Aug", "Sep", "Oct", "Nov", "Dec"];
    let Some(d) = parse_in(iso, clock) else { return "Invalid Date".into() };
    let d = clock.local.local(&d);
    let h = match d.hour() % 12 {
        0 => 12,
        h => h,
    };
    format!(
        "{} {}, {} at {h}:{:02} {}",
        MONTHS[d.month0() as usize],
        d.day(),
        d.year(),
        d.minute(),
        if d.hour() < 12 { "AM" } else { "PM" }
    )
}

/// JS `String(v)` for a JSON value (`undefined` when missing): what `"…" + v` writes.
pub fn string(v: Option<&serde_json::Value>) -> String {
    use serde_json::Value;
    match v {
        None => "undefined".into(),
        Some(Value::Null) => "null".into(),
        Some(Value::String(s)) => s.clone(),
        Some(Value::Bool(b)) => b.to_string(),
        Some(Value::Number(n)) => n.to_string(),
        Some(Value::Array(a)) => a.iter().map(|x| if x.is_null() { String::new() } else { string(Some(x)) }).collect::<Vec<_>>().join(","),
        Some(Value::Object(_)) => "[object Object]".into(),
    }
}

/// `s.slice(0, n)` in UTF-16 code units (a cut through a surrogate pair becomes U+FFFD, as in `clip`).
pub fn slice16(s: &str, n: usize) -> String {
    let u: Vec<u16> = s.encode_utf16().take(n).collect();
    String::from_utf16_lossy(&u)
}

/// `a.localeCompare(b)` as bun (ICU root collation, punctuation not ignored) orders ASCII: whitespace and punctuation, then
/// digits, then letters, case only breaking a tie (lower first). Characters past ASCII sort after every ASCII one, by code point
/// (ICU interleaves them; fold ids are slugs, so they meet none).
pub fn locale_compare(a: &str, b: &str) -> std::cmp::Ordering {
    // bun 1.3.13: [...printable ASCII].sort((a, b) => a.localeCompare(b)), case pairs equal at the first level
    const ORDER: &str = " _-,;:!?.'\"()[]{}@*/\\&#%`^+<=>|~$0123456789abcdefghijklmnopqrstuvwxyz";
    let primary = |c: char| -> u32 {
        let l = c.to_ascii_lowercase();
        match ORDER.find(l) {
            Some(i) if c.is_ascii() && !c.is_ascii_control() => i as u32,
            _ => 0x100 + c as u32,
        }
    };
    let p = |s: &str| s.chars().map(primary).collect::<Vec<_>>();
    let case = |s: &str| s.chars().map(|c| c.is_ascii_uppercase()).collect::<Vec<_>>();
    p(a).cmp(&p(b)).then_with(|| case(a).cmp(&case(b)))
}
