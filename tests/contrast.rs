//! Contrast proven from tokens, not measured in a browser (brief item 17): the viewer's theme (viewer/theme.tokens.json, a Baobab
//! pinning) clears every pair of its own contract in light and dark (text 4.5, UI 3) and in light-more and dark-more for
//! prefers-contrast:more (text 7, UI 4.5), through src/baobab.rs; under forced-colors every role is a system colour and every pair
//! one the platform guarantees. Its tokens, pairs and results are goldens/contrast.json, which js/contrast.test.ts runs Baobab's
//! own TS over; fold.css carries the custom properties written from the tokens and no colour of its own, and keeps a border
//! wherever it draws a fill, so the edge survives forced-colors; native/Colors.xcassets carries the same roles for Apple hosts
//! (any, dark, Increase Contrast). `FOLD_BLESS=1 cargo test --test contrast` writes the golden, fold.css's block and the assets.

use std::fs;
use std::path::{Path, PathBuf};

use fold_core::baobab::{check_contrast, colorsets, flatten_tokens, forced_pairs, theme_css, theme_pairs, MODES};
use serde_json::{json, Value};

fn root() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).to_path_buf()
}

fn theme() -> Value {
    serde_json::from_str(&fs::read_to_string(root().join("viewer/theme.tokens.json")).unwrap()).unwrap()
}

const BEGIN: &str = "/* baobab:begin */\n";
const END: &str = "/* baobab:end */";

#[test]
fn every_role_pair_clears_its_minimum_in_every_mode() {
    let theme = theme();
    let results = check_contrast(&flatten_tokens(&theme).unwrap(), &theme_pairs(&theme).unwrap()).unwrap();
    assert_eq!(results.len(), MODES.len() * theme["$contrast"].as_array().unwrap().len());
    let failed: Vec<_> = results
        .iter()
        .filter(|r| !r.pass)
        .map(|r| format!("{} on {}: {:.2} < {} ({})", r.pair.fg, r.pair.bg, r.ratio, r.pair.min, r.pair.label.as_deref().unwrap_or("")))
        .collect();
    assert!(failed.is_empty(), "pairs under their minimum:\n{}", failed.join("\n"));
}

#[test]
fn every_role_pair_is_a_guaranteed_system_pair_under_forced_colors() {
    let forced = forced_pairs(&theme()).unwrap();
    let failed: Vec<_> = forced
        .iter()
        .filter(|r| !r.pass)
        .map(|r| format!("{} on {} = {} on {} ({})", r.fg, r.bg, r.system[0], r.system[1], r.label.as_deref().unwrap_or("")))
        .collect();
    assert!(failed.is_empty(), "forced-colors pairs the platform does not guarantee:\n{}", failed.join("\n"));
}

#[test]
fn the_golden_holds_the_tokens_pairs_and_results() {
    let theme = theme();
    let tokens = flatten_tokens(&theme).unwrap();
    let pairs = theme_pairs(&theme).unwrap();
    let results = check_contrast(&tokens, &pairs).unwrap();
    let forced = forced_pairs(&theme).unwrap();
    let got = json!({ "baobab": "@bounded-systems/baobab@0.2.2", "tokens": tokens, "pairs": pairs, "results": results, "forced": forced });
    // compared as text: serde_json reads floats a ulp off without float_roundtrip, and the ratios are held to the last bit
    let got = serde_json::to_string_pretty(&got).unwrap() + "\n";
    let path = root().join("goldens/contrast.json");
    if std::env::var("FOLD_BLESS").is_ok() {
        fs::write(&path, &got).unwrap();
    }
    assert!(got == fs::read_to_string(&path).unwrap(), "goldens/contrast.json is stale (FOLD_BLESS=1 cargo test --test contrast)");
}

#[test]
fn fold_css_carries_the_tokens_and_no_colour_of_its_own() {
    let path = root().join("viewer/fold.css");
    let css = fs::read_to_string(&path).unwrap();
    let (head, rest) = css.split_once(BEGIN).expect("fold.css has no baobab:begin mark");
    let (block, tail) = rest.split_once(END).expect("fold.css has no baobab:end mark");
    let want = theme_css(&theme()).unwrap();
    if std::env::var("FOLD_BLESS").is_ok() && block != want {
        fs::write(&path, format!("{head}{BEGIN}{want}{END}{tail}")).unwrap();
    } else {
        assert_eq!(block, want, "fold.css's colours are not the tokens' (FOLD_BLESS=1 cargo test --test contrast)");
    }

    // Outside the block: no colour literal, and every custom property a colour is drawn with is a role the contract pairs.
    let theme = theme();
    let paired: Vec<&str> =
        theme["$contrast"].as_array().unwrap().iter().flat_map(|p| [p["fg"].as_str().unwrap(), p["bg"].as_str().unwrap()]).collect();
    let own = format!("{head}{tail}");
    for lit in ["#", "rgb(", "hsl(", "oklch(", "color-mix("] {
        let hits: Vec<_> = own.lines().filter(|l| !l.trim_start().starts_with("/*") && l.contains(lit)).collect();
        assert!(hits.is_empty(), "fold.css has a colour of its own ({lit}): {hits:?}");
    }
    for decl in own.split([';', '{', '}']) {
        let Some((prop, value)) = decl.split_once(':') else { continue };
        let prop = prop.trim();
        if !(prop == "color" || prop.starts_with("background") || prop.starts_with("border") || prop.starts_with("outline")) {
            continue;
        }
        for role in value.split("var(--").skip(1).map(|v| v.split(')').next().unwrap()) {
            assert!(paired.contains(&role), "fold.css draws {prop} with --{role}, a role no contrast pair covers");
        }
    }
}

#[test]
fn fold_css_keeps_an_edge_wherever_it_fills_and_never_opts_out_of_forced_colors() {
    // Under forced-colors a background is the user's Canvas, so a fill alone marks no edge; a border or outline does (the system
    // repaints it, transparent or not). So: no rule fills with a role but drops its border, and nothing turns forced colours off.
    let css = fs::read_to_string(root().join("viewer/fold.css")).unwrap();
    assert!(!css.contains("forced-color-adjust"), "fold.css opts out of forced-colors");
    for rule in css.split('}') {
        let Some((sel, body)) = rule.rsplit_once('{') else { continue };
        let fills = body.split(';').any(|d| {
            d.split_once(':').is_some_and(|(p, v)| p.trim() == "background" && v.contains("var(--") && !v.contains("var(--paper)"))
        });
        let edged = body.split(';').any(|d| {
            d.split_once(':').is_some_and(|(p, v)| {
                (p.trim().starts_with("border") && !p.contains("radius") || p.trim().starts_with("outline"))
                    && v.trim() != "0"
                    && v.trim() != "none"
            })
        });
        assert!(!fills || edged, "{} fills with a role but has no border or outline", sel.trim());
    }
}

#[test]
fn native_colorsets_are_the_tokens() {
    // One colorset per role for Apple hosts (brief item 18): any, dark, and Increase Contrast (light-more, dark-more).
    let dir = root().join("native/Colors.xcassets");
    let want = colorsets(&theme()).unwrap();
    let bless = std::env::var("FOLD_BLESS").is_ok();
    if bless {
        let _ = fs::remove_dir_all(&dir);
        fs::create_dir_all(&dir).unwrap();
        fs::write(dir.join("Contents.json"), "{\n  \"info\" : {\n    \"author\" : \"xcode\",\n    \"version\" : 1\n  }\n}\n").unwrap();
    }
    let mut names: Vec<String> =
        fs::read_dir(&dir).map(|d| d.flatten().map(|e| e.file_name().to_string_lossy().into_owned()).collect()).unwrap_or_default();
    names.retain(|n| n.ends_with(".colorset"));
    names.sort();
    let mut expect: Vec<String> = want.iter().map(|(role, _)| format!("{role}.colorset")).collect();
    for (role, contents) in &want {
        let path = dir.join(format!("{role}.colorset"));
        let text = serde_json::to_string_pretty(contents).unwrap() + "\n";
        if bless {
            fs::create_dir_all(&path).unwrap();
            fs::write(path.join("Contents.json"), &text).unwrap();
        } else {
            assert!(
                fs::read_to_string(path.join("Contents.json")).ok().as_deref() == Some(&text),
                "{role}.colorset is stale (FOLD_BLESS=1 cargo test --test contrast)"
            );
        }
    }
    if !bless {
        expect.sort();
        assert_eq!(names, expect, "native/Colors.xcassets holds colorsets for other roles");
    }
}
