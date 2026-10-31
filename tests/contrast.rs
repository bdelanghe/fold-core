//! Contrast proven from tokens, not measured in a browser (brief item 17): the viewer's theme (viewer/theme.tokens.json, a Baobab
//! pinning) clears every pair of its own contract in light and dark, through src/baobab.rs. Its tokens, pairs and results are
//! goldens/contrast.json, which js/contrast.test.ts runs Baobab's own TS over; fold.css carries the custom properties written from
//! the tokens and no colour of its own. `FOLD_BLESS=1 cargo test --test contrast` writes the golden and fold.css's block.

use std::fs;
use std::path::{Path, PathBuf};

use fold_core::baobab::{check_contrast, flatten_tokens, theme_css, theme_pairs, MODES};
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
fn the_golden_holds_the_tokens_pairs_and_results() {
    let theme = theme();
    let tokens = flatten_tokens(&theme).unwrap();
    let pairs = theme_pairs(&theme).unwrap();
    let results = check_contrast(&tokens, &pairs).unwrap();
    let got = json!({ "baobab": "@bounded-systems/baobab@0.2.2", "tokens": tokens, "pairs": pairs, "results": results });
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
