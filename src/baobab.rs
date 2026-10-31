//! Baobab in Rust: the token shape and the contrast check of `@bounded-systems/baobab` 0.2.2 (`src/tokens.ts`, `src/contrast.ts`),
//! ported line for line, so contrast is proven from tokens at build time with no stylesheet or browser (Bobby, brief item 17).
//! js/contrast.test.ts runs Baobab's own TS (vendored in js/baobab/) over the same tokens and pairs and must agree with
//! goldens/contrast.json, which tests/contrast.rs writes.
//!
//! The viewer's theme (viewer/theme.tokens.json) is a Baobab pinning: W3C design tokens with a `light` and a `dark` set of the
//! same roles, aliased to its `primitive`s, and its own contract under `$contrast` (role pairs and the ratio each must clear).
//! [`theme_pairs`] spreads that contract over both modes, [`check_contrast`] proves it, and [`theme_css`] writes the custom
//! properties fold.css carries, so no colour in the stylesheet is typed by hand.

use std::collections::HashSet;

use serde::{Deserialize, Serialize};
use serde_json::{Map, Value};

/// A flat, resolved slot map: `{ "light.paper": "#EDEAE1", … }`, in the tree's order.
pub type Tokens = Map<String, Value>;

/// One relationship a pinning wants held: a foreground/background token pair and the ratio they must clear.
#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
pub struct ContrastPair {
    pub fg: String,
    pub bg: String,
    pub min: f64,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub label: Option<String>,
}

/// A checked pair: the contract plus its measured ratio and verdict.
#[derive(Clone, Debug, Serialize, PartialEq)]
pub struct ContrastResult {
    #[serde(flatten)]
    pub pair: ContrastPair,
    pub ratio: f64,
    pub pass: bool,
}

/// The colour modes a theme pinning declares, in the order fold.css writes them, each with the contract minimum it is held to:
/// the light and dark sets clear a pair's `min`, the prefers-contrast:more sets its `more`.
pub const MODES: [(&str, Minimum); 4] =
    [("light", Minimum::Base), ("dark", Minimum::Base), ("light-more", Minimum::More), ("dark-more", Minimum::More)];

/// Which of a role pair's minimums a mode is held to.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Minimum {
    Base,
    More,
}

/// One pair of the theme's own contract (`$contrast`), before it is spread over the modes.
#[derive(Clone, Debug, Deserialize)]
struct RolePair {
    fg: String,
    bg: String,
    min: f64,
    more: f64,
    label: Option<String>,
}

/// The pairs of CSS Color 4 system colours the platform guarantees to contrast, unordered (a ratio has no direction). Under
/// forced-colors the user's palette fills them in, so a pair is proven by naming one of these, never by measuring.
pub const SYSTEM_PAIRS: [[&str; 2]; 6] = [
    ["CanvasText", "Canvas"],
    ["LinkText", "Canvas"],
    ["ButtonText", "ButtonFace"],
    ["ButtonBorder", "ButtonFace"],
    ["FieldText", "Field"],
    ["HighlightText", "Highlight"],
];

/// The CSS Color 4 system colour keywords a `forced` set may name.
pub const SYSTEM_COLORS: [&str; 18] = [
    "AccentColor",
    "AccentColorText",
    "ActiveText",
    "ButtonBorder",
    "ButtonFace",
    "ButtonText",
    "Canvas",
    "CanvasText",
    "Field",
    "FieldText",
    "GrayText",
    "Highlight",
    "HighlightText",
    "LinkText",
    "Mark",
    "MarkText",
    "SelectedItem",
    "SelectedItemText",
];

/// A contract pair under forced-colors: the system colours its roles map to, and whether the platform guarantees them.
#[derive(Clone, Debug, Serialize, PartialEq)]
pub struct ForcedResult {
    pub fg: String,
    pub bg: String,
    pub system: [String; 2],
    #[serde(skip_serializing_if = "Option::is_none")]
    pub label: Option<String>,
    pub pass: bool,
}

fn is_leaf(n: &Value) -> bool {
    n.as_object().is_some_and(|o| o.contains_key("$value"))
}

fn lookup<'a>(root: &'a Value, path: &str) -> Result<&'a Value, String> {
    let mut node = Some(root);
    for seg in path.split('.') {
        node = node.and_then(|n| n.get(seg));
    }
    match node {
        Some(n) if is_leaf(n) => Ok(n),
        _ => Err(format!("alias target is not a token: {{{path}}}")),
    }
}

fn alias(s: &str) -> Option<&str> {
    s.strip_prefix('{').and_then(|r| r.strip_suffix('}')).filter(|p| !p.is_empty())
}

fn resolve(root: &Value, value: &Value, seen: &mut HashSet<String>) -> Result<Value, String> {
    match value {
        Value::String(s) => match alias(s) {
            None => Ok(value.clone()),
            Some(path) => {
                if !seen.insert(path.to_string()) {
                    return Err(format!("alias cycle at {{{path}}}"));
                }
                resolve(root, &lookup(root, path)?["$value"], seen)
            }
        },
        Value::Array(a) => a.iter().map(|v| resolve(root, v, &mut seen.clone())).collect::<Result<_, _>>().map(Value::Array),
        Value::Object(o) => {
            o.iter().map(|(k, v)| Ok((k.clone(), resolve(root, v, &mut seen.clone())?))).collect::<Result<_, _>>().map(Value::Object)
        }
        _ => Ok(value.clone()),
    }
}

/// JS's `String(value)` for the scalars a token holds.
fn js_string(v: &Value) -> String {
    match v {
        Value::String(s) => s.clone(),
        Value::Number(n) => match n.as_f64() {
            Some(f) if f.fract() == 0.0 && f.abs() < 1e21 => format!("{}", f as i64),
            Some(f) => format!("{f}"),
            None => n.to_string(),
        },
        other => other.to_string(),
    }
}

fn css(value: &Value) -> String {
    match value {
        Value::Array(a) => a.iter().map(js_string).collect::<Vec<_>>().join(", "),
        Value::Object(_) => value.to_string(),
        v => js_string(v),
    }
}

/// Flatten a W3C design-token tree to a resolved slot map: aliases chased, groups joined by '.', `$`-keys skipped.
pub fn flatten_tokens(root: &Value) -> Result<Tokens, String> {
    fn walk(root: &Value, node: &Value, prefix: &str, out: &mut Tokens) -> Result<(), String> {
        if is_leaf(node) {
            out.insert(prefix.to_string(), Value::String(css(&resolve(root, &node["$value"], &mut HashSet::new())?)));
        } else if let Some(o) = node.as_object() {
            for (k, v) in o {
                if k.starts_with('$') {
                    continue;
                }
                walk(root, v, &if prefix.is_empty() { k.clone() } else { format!("{prefix}.{k}") }, out)?;
            }
        }
        Ok(())
    }
    let mut out = Tokens::new();
    walk(root, root, "", &mut out)?;
    Ok(out)
}

fn hex_to_linear(hex: &str) -> Result<[f64; 3], String> {
    let n = hex.replacen('#', "", 1);
    let full = if n.chars().count() == 3 { n.chars().flat_map(|c| [c, c]).collect() } else { n };
    if full.len() != 6 || !full.bytes().all(|b| b.is_ascii_hexdigit()) {
        return Err(format!("not a hex color: {hex}"));
    }
    let lin = |c: f64| if c <= 0.04045 { c / 12.92 } else { ((c + 0.055) / 1.055).powf(2.4) };
    let ch = |i: usize| lin(u8::from_str_radix(&full[i..i + 2], 16).unwrap() as f64 / 255.0);
    Ok([ch(0), ch(2), ch(4)])
}

/// WCAG 2.x relative luminance of an sRGB hex color (`#RGB` or `#RRGGBB`).
pub fn relative_luminance(hex: &str) -> Result<f64, String> {
    let [r, g, b] = hex_to_linear(hex)?;
    Ok(0.2126 * r + 0.7152 * g + 0.0722 * b)
}

/// WCAG contrast ratio between two sRGB hex colors. Order-independent, always ≥ 1.
pub fn contrast_ratio(a: &str, b: &str) -> Result<f64, String> {
    let (la, lb) = (relative_luminance(a)?, relative_luminance(b)?);
    let (lighter, darker) = if la >= lb { (la, lb) } else { (lb, la) };
    Ok((lighter + 0.05) / (darker + 0.05))
}

/// Check a pinning's resolved tokens against its own contrast contract; an unknown token path is an error.
pub fn check_contrast(tokens: &Tokens, pairs: &[ContrastPair]) -> Result<Vec<ContrastResult>, String> {
    pairs
        .iter()
        .map(|pair| {
            let get =
                |p: &str| tokens.get(p).and_then(Value::as_str).ok_or_else(|| format!("contrast pair references unknown token: \"{p}\""));
            let ratio = contrast_ratio(get(&pair.fg)?, get(&pair.bg)?)?;
            Ok(ContrastResult { pair: pair.clone(), ratio, pass: ratio >= pair.min })
        })
        .collect()
}

fn role_pairs(theme: &Value) -> Result<Vec<RolePair>, String> {
    serde_json::from_value(theme["$contrast"].clone()).map_err(|e| format!("theme $contrast: {e}"))
}

/// The theme's contract (`$contrast`: role pairs) spread over every mode as Baobab pairs on flat paths (`light.ink` on
/// `light.paper`), each at the minimum its mode is held to.
pub fn theme_pairs(theme: &Value) -> Result<Vec<ContrastPair>, String> {
    let roles = role_pairs(theme)?;
    Ok(MODES
        .iter()
        .flat_map(|&(m, at)| {
            roles.iter().map(move |r| ContrastPair {
                fg: format!("{m}.{}", r.fg),
                bg: format!("{m}.{}", r.bg),
                min: if at == Minimum::More { r.more } else { r.min },
                label: r.label.as_ref().map(|l| format!("{l} ({m})")),
            })
        })
        .collect())
}

/// The contract under forced-colors: every role of the `forced` set is a system colour keyword (no author colour), and every pair
/// lands on one of [`SYSTEM_PAIRS`]. A role that is not a system colour is an error, an unguaranteed pair a failing result.
pub fn forced_pairs(theme: &Value) -> Result<Vec<ForcedResult>, String> {
    let tokens = flatten_tokens(theme)?;
    let system = |role: &str| -> Result<String, String> {
        let v = tokens.get(&format!("forced.{role}")).and_then(Value::as_str).ok_or_else(|| format!("forced set has no role {role}"))?;
        SYSTEM_COLORS.contains(&v).then(|| v.to_string()).ok_or_else(|| format!("forced.{role} is not a system colour: {v}"))
    };
    role_pairs(theme)?
        .into_iter()
        .map(|r| {
            let (f, b) = (system(&r.fg)?, system(&r.bg)?);
            let pass = SYSTEM_PAIRS.iter().any(|[x, y]| (*x == f && *y == b) || (*x == b && *y == f));
            Ok(ForcedResult { fg: r.fg, bg: r.bg, system: [f, b], label: r.label, pass })
        })
        .collect()
}

/// The custom properties of each mode, as fold.css writes them: light on :root, dark under prefers-color-scheme unless the page
/// asks for light, and dark again when it asks for dark; the -more sets the same way under prefers-contrast:more, after the base
/// sets at the same specificity so they win; and the forced set under forced-colors, last and on every :root, so it wins over all.
pub fn theme_css(theme: &Value) -> Result<String, String> {
    let tokens = flatten_tokens(theme)?;
    let props = |mode: &str| -> String {
        let pre = format!("{mode}.");
        tokens.iter().filter_map(|(k, v)| Some(format!("--{}:{};", k.strip_prefix(&pre)?, v.as_str()?))).collect()
    };
    let (light, dark, light_more, dark_more, forced) =
        (props("light"), props("dark"), props("light-more"), props("dark-more"), props("forced"));
    let more = "@media (prefers-contrast:more)";
    Ok([
        format!(":root{{{light}}}"),
        format!("@media (prefers-color-scheme:dark){{:root:not([data-theme=\"light\"]){{{dark}}}}}"),
        format!(":root[data-theme=\"dark\"]{{{dark}}}"),
        format!("{more} and (prefers-color-scheme:light){{:root:not([data-theme=\"dark\"]){{{light_more}}}}}"),
        format!("{more}{{:root[data-theme=\"light\"]{{{light_more}}}}}"),
        format!("{more} and (prefers-color-scheme:dark){{:root:not([data-theme=\"light\"]){{{dark_more}}}}}"),
        format!("{more}{{:root[data-theme=\"dark\"]{{{dark_more}}}}}"),
        format!("@media (forced-colors:active){{:root,:root[data-theme]{{{forced}}}}}"),
    ]
    .map(|l| l + "\n")
    .concat())
}

/// The theme as an Xcode asset catalog's colorsets, one per role (brief item 18, native hosts): its light value for any
/// appearance, dark for dark, and light-more and dark-more for Increase Contrast, so the Rust proof covers Apple hosts too.
pub fn colorsets(theme: &Value) -> Result<Vec<(String, Value)>, String> {
    let tokens = flatten_tokens(theme)?;
    let roles: Vec<String> = tokens.keys().filter_map(|k| k.strip_prefix("light.")).map(str::to_string).collect();
    roles
        .into_iter()
        .map(|role| {
            let color = |mode: &str| -> Result<Value, String> {
                let hex = tokens.get(&format!("{mode}.{role}")).and_then(Value::as_str).ok_or_else(|| format!("{mode} has no role {role}"))?;
                let h = hex.trim_start_matches('#');
                if h.len() != 6 || !h.bytes().all(|b| b.is_ascii_hexdigit()) {
                    return Err(format!("{mode}.{role} is not #RRGGBB: {hex}"));
                }
                let c = |i: usize| Value::String(format!("0x{}", &h[i..i + 2]));
                Ok(serde_json::json!({ "color-space": "srgb", "components": { "alpha": "1.000", "blue": c(4), "green": c(2), "red": c(0) } }))
            };
            let look = |dark: bool, high: bool| {
                let mut a = vec![];
                if dark {
                    a.push(serde_json::json!({ "appearance": "luminosity", "value": "dark" }));
                }
                if high {
                    a.push(serde_json::json!({ "appearance": "contrast", "value": "high" }));
                }
                a
            };
            let entry = |mode: &str, dark: bool, high: bool| -> Result<Value, String> {
                let mut e = serde_json::json!({ "color": color(mode)?, "idiom": "universal" });
                if dark || high {
                    e["appearances"] = Value::Array(look(dark, high));
                }
                Ok(e)
            };
            let colors =
                vec![entry("light", false, false)?, entry("dark", true, false)?, entry("light-more", false, true)?, entry("dark-more", true, true)?];
            Ok((role, serde_json::json!({ "colors": colors, "info": { "author": "xcode", "version": 1 } })))
        })
        .collect()
}
