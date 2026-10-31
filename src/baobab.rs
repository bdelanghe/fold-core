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

/// The modes a theme pinning declares, in the order fold.css writes them.
pub const MODES: [&str; 2] = ["light", "dark"];

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

/// The theme's contract (`$contrast`: role pairs) spread over every mode as Baobab pairs on flat paths (`light.ink` on `light.paper`).
pub fn theme_pairs(theme: &Value) -> Result<Vec<ContrastPair>, String> {
    let roles: Vec<ContrastPair> = serde_json::from_value(theme["$contrast"].clone()).map_err(|e| format!("theme $contrast: {e}"))?;
    Ok(MODES
        .iter()
        .flat_map(|m| {
            roles.iter().map(move |r| ContrastPair {
                fg: format!("{m}.{}", r.fg),
                bg: format!("{m}.{}", r.bg),
                min: r.min,
                label: r.label.as_ref().map(|l| format!("{l} ({m})")),
            })
        })
        .collect())
}

/// The custom properties of each mode, as fold.css writes them: light on :root, dark under prefers-color-scheme unless the page
/// asks for light, and dark again when it asks for dark.
pub fn theme_css(theme: &Value) -> Result<String, String> {
    let tokens = flatten_tokens(theme)?;
    let props = |mode: &str| -> String {
        let pre = format!("{mode}.");
        tokens.iter().filter_map(|(k, v)| Some(format!("--{}:{};", k.strip_prefix(&pre)?, v.as_str()?))).collect()
    };
    let (light, dark) = (props("light"), props("dark"));
    Ok(format!(
        ":root{{{light}}}\n@media (prefers-color-scheme:dark){{:root:not([data-theme=\"light\"]){{{dark}}}}}\n:root[data-theme=\"dark\"]{{{dark}}}\n"
    ))
}
