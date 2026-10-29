//! The JavaScript string and date behaviour fold.ts leans on, reproduced: UTF-16 lengths and cuts, JS's `\s`/`trim()` whitespace
//! set, `.` stopping at line terminators, and `toLocaleString("en-US", {weekday: "short", hour: "2-digit", minute: "2-digit",
//! hour12: false})` as bun (JavaScriptCore) prints it ("Wed 00:05": midnight is 00, not 24).

use std::sync::OnceLock;

use chrono::{DateTime, Duration, NaiveDate, NaiveDateTime, NaiveTime, TimeZone, Utc};
use chrono_tz::Tz;
use regex::Regex;

/// JS `\s` (WhiteSpace and LineTerminator), as a regex class.
const WS: &str = r"[\t\n\x0B\x0C\r \x{A0}\x{1680}\x{2000}-\x{200A}\x{2028}\x{2029}\x{202F}\x{205F}\x{3000}\x{FEFF}]";

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

fn re(cell: &'static OnceLock<Regex>, src: impl FnOnce() -> String) -> &'static Regex {
    cell.get_or_init(|| Regex::new(&src()).expect("valid regex"))
}

/// `/^(#{1,6})\s+(.*)$/` on one line: the level and the heading's words.
pub fn heading(line: &str) -> Option<(usize, &str)> {
    static R: OnceLock<Regex> = OnceLock::new();
    let c = re(&R, || format!(r"^(#{{1,6}}){WS}+([^\n\r\x{{2028}}\x{{2029}}]*)$")).captures(line)?;
    Some((c.get(1)?.as_str().len(), c.get(2)?.as_str()))
}

/// `/^\s*([-*]|\d+\.)\s+/`: a list item's words after its marker.
pub fn list_item(line: &str) -> Option<&str> {
    static R: OnceLock<Regex> = OnceLock::new();
    let m = re(&R, || format!(r"^{WS}*(?:[-*]|[0-9]+\.){WS}+")).find(line)?;
    Some(&line[m.end()..])
}

/// `.replace(/\s*\([^)]*\)$/, "")`: a trailing parenthetical off.
pub fn drop_paren(s: &str) -> String {
    static R: OnceLock<Regex> = OnceLock::new();
    re(&R, || format!(r"{WS}*\([^)]*\)$")).replace(s, "").into_owned()
}

/// `new RegExp(`/a/${self}(?:[/?#]|$)`).test(href)` (self is not escaped in fold.ts either).
pub fn self_link(me: &str, href: &str) -> bool {
    Regex::new(&format!(r"/a/{me}(?:[/?#]|$)")).is_ok_and(|r| r.is_match(href))
}

/// The zones `when` reads in: the page's `meta.timeZone`, else the machine's (the caller's `local_time_zone`), else UTC.
pub struct Clock {
    show: Tz,
    local: Tz,
}

impl Clock {
    pub fn new(page: Option<&str>, local: Option<&str>) -> Clock {
        let local = local.and_then(|z| z.parse().ok()).unwrap_or(Tz::UTC);
        Clock { show: page.and_then(|z| z.parse().ok()).unwrap_or(local), local }
    }
}

/// `new Date(iso)` for the ISO-like forms JavaScriptCore reads: `YYYY[-MM[-DD]]` (UTC) and a date with a time after `T`, `t` or
/// a space, with seconds and a fraction optional, then `Z`, `±HH:MM` or `±HHMM` (no offset: the local zone).
fn parse(iso: &str) -> Option<DateTime<Utc>> {
    static R: OnceLock<Regex> = OnceLock::new();
    let r = re(&R, || {
        r"^(\d{4})(?:-(\d{2})(?:-(\d{2}))?)?(?:[Tt ](\d{2}):(\d{2})(?::(\d{2})(?:\.\d+)?)?(Z|z|[+-]\d{2}:?\d{2})?)?$".to_string()
    });
    let c = r.captures(iso)?;
    let n = |i: usize, d: u32| c.get(i).map_or(Some(d), |m| m.as_str().parse().ok());
    let (y, mo, d) = (c.get(1)?.as_str().parse::<i32>().ok()?, n(2, 1)?, n(3, 1)?);
    let date = NaiveDate::from_ymd_opt(y, mo, d)?;
    let Some(h) = c.get(4) else {
        return Some(Utc.from_utc_datetime(&date.and_time(NaiveTime::MIN)));
    };
    let (h, mi, se) = (h.as_str().parse::<u32>().ok()?, n(5, 0)?, n(6, 0)?);
    let at = if h == 24 && mi == 0 && se == 0 {
        NaiveDateTime::new(date, NaiveTime::MIN) + Duration::days(1)
    } else {
        NaiveDateTime::new(date, NaiveTime::from_hms_opt(h, mi, se)?)
    };
    match c.get(7).map(|m| m.as_str()) {
        Some("Z" | "z") => Some(Utc.from_utc_datetime(&at)),
        Some(off) => {
            let sign = if off.starts_with('-') { -1 } else { 1 };
            let digits: String = off[1..].chars().filter(char::is_ascii_digit).collect();
            let (oh, om): (i64, i64) = (digits[..2].parse().ok()?, digits[2..].parse().ok()?);
            if oh > 23 || om > 59 {
                return None;
            }
            Some(Utc.from_utc_datetime(&(at - Duration::minutes(sign * (oh * 60 + om)))))
        }
        // No offset: read as UTC here; parse_in moves it into the local zone.
        None => Some(Utc.from_utc_datetime(&at)),
    }
}

fn parse_in(iso: &str, clock: &Clock) -> Option<DateTime<Utc>> {
    let utc = parse(iso)?;
    // Without an offset, a date with a time is local time.
    let zoned = iso.len() > 10 && !iso.ends_with(['Z', 'z']) && !has_offset(iso);
    if zoned {
        let naive = utc.naive_utc();
        return clock.local.from_local_datetime(&naive).earliest().map(|d| d.with_timezone(&Utc));
    }
    Some(utc)
}

fn has_offset(iso: &str) -> bool {
    let t = &iso[10..];
    t.contains('+') || t.contains('-')
}

/// fold.ts `when`: an ISO time as "Tue 14:44" in the page's zone; anything else as it is.
pub fn when(iso: &str, clock: &Clock) -> String {
    match parse_in(iso, clock) {
        Some(d) => d.with_timezone(&clock.show).format("%a %H:%M").to_string(),
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
    use chrono::{Datelike, Timelike};
    let Some(d) = parse_in(iso, clock) else { return "Invalid Date".into() };
    let d = d.with_timezone(&clock.local);
    let h = match d.hour() % 12 {
        0 => 12,
        h => h,
    };
    format!("{} {}, {} at {h}:{:02} {}", d.format("%b"), d.day(), d.year(), d.minute(), if d.hour() < 12 { "AM" } else { "PM" })
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
