//! The core as a WebAssembly module with a plain C ABI, so any JS runtime (bun, deno, a browser) loads it with
//! `WebAssembly.instantiate` and no generated glue (js/fold-core.ts is the loader).
//!
//!   alloc(len) -> ptr           the host writes its UTF-8 input there
//!   fold_json(ptr, len) -> u64  folds `{root, files, opts}` (the shape of goldens/<case>.input.json) and returns
//!                               (out_ptr << 32) | out_len: `[[id, node], ...]` in insertion order, or `{"error": "..."}`
//!   walk_json(ptr, len) -> u64  walks `{nodes, opts, facto?}` (goldens/walk/<case>.input.json): `{walk, verdict, facto?}`
//!   ax_json(ptr, len) -> u64    one fold's accessibility tree (fold-ax.js axOf) from `{node, path, mode, href, localTimeZone?}`
//!                               (`href` "walk" or "viewer", the links axOf is given in home; `node` null for a fold with no data)
//!   html_json(ptr, len) -> u64  one fold's HTML (fold-dom.js toDom, as text: src/html.rs) from its accessibility tree
//!                               (ax_json's output): `{"html": "<main>…"}`, or `{"error": "fold-ax: …"}` where toDom refuses
//!   dealloc(ptr, len)           frees either buffer
//!
//! `files` maps each `from` path to its text; a path not in it reads as missing, with fold.ts's ENOENT note.

use serde_json::{json, Value};

use crate::ax::{ax_of, link};
use crate::js::Clock;
use crate::walk::{facto_body, verdict, walk, WalkOpts};
use crate::{fold, with_sources, FoldOpts};

#[no_mangle]
pub extern "C" fn alloc(len: usize) -> *mut u8 {
    let mut b = Vec::<u8>::with_capacity(len.max(1));
    let p = b.as_mut_ptr();
    std::mem::forget(b);
    p
}

/// # Safety
/// `ptr` and `len` are a buffer this module returned (from `alloc`, or packed in a call's result), freed once.
#[no_mangle]
pub unsafe extern "C" fn dealloc(ptr: *mut u8, len: usize) {
    drop(Vec::from_raw_parts(ptr, 0, len.max(1)));
}

fn json_call(input: &str, f: impl FnOnce(Value) -> Result<String, String>) -> String {
    serde_json::from_str(input)
        .map_err(|e| format!("input is not JSON: {e}"))
        .and_then(f)
        .unwrap_or_else(|e| json!({ "error": e }).to_string())
}

/// Folds one `{root, files, opts}` JSON text into its fold list's JSON text.
pub fn fold_text(input: &str) -> String {
    json_call(input, |case| {
        let opts: FoldOpts = match case.get("opts") {
            Some(o @ Value::Object(_)) => serde_json::from_value(o.clone()).map_err(|e| format!("opts: {e}"))?,
            _ => FoldOpts::default(),
        };
        let files = case.get("files").and_then(Value::as_object).cloned().unwrap_or_default();
        let read = |p: &str| files.get(p).and_then(Value::as_str).map(str::to_string);
        let root = case.get("root").ok_or("no root")?;
        serde_json::to_string(&fold(&with_sources(root, &read), &opts)).map_err(|e| e.to_string())
    })
}

/// Walks one `{nodes, opts, facto?}` JSON text: `nodes` is the fold collection as `[[id, node], ...]` (or an `{id: node}`
/// object), `opts` the walk's (budgets.json's `budgets`, the machine's zone), `facto` `{artifact, at, rev}` for its Observations.
pub fn walk_text(input: &str) -> String {
    json_call(input, |case| {
        let nodes: Vec<(String, Value)> = match case.get("nodes") {
            Some(Value::Array(a)) => serde_json::from_value(Value::Array(a.clone())).map_err(|e| format!("nodes: {e}"))?,
            Some(Value::Object(m)) => m.iter().map(|(k, v)| (k.clone(), v.clone())).collect(),
            _ => return Err("no nodes".into()),
        };
        let opts: WalkOpts = serde_json::from_value(case.get("opts").cloned().unwrap_or(Value::Null)).map_err(|e| format!("opts: {e}"))?;
        let r = walk(&nodes, &opts)?;
        let mut out = json!({ "walk": r, "verdict": verdict(&r) });
        if let Some(f) = case.get("facto").filter(|f| f.is_object()) {
            let s = |k: &str| f.get(k).and_then(Value::as_str).unwrap_or("");
            out["facto"] = facto_body(s("artifact"), s("at"), s("rev"), &r);
        }
        serde_json::to_string(&out).map_err(|e| e.to_string())
    })
}

/// One fold's accessibility tree, from `{node, path, mode, href, localTimeZone?}`.
pub fn ax_text(input: &str) -> String {
    json_call(input, |case| {
        let path: Vec<String> = serde_json::from_value(case.get("path").cloned().unwrap_or(json!([]))).map_err(|e| format!("path: {e}"))?;
        let mode = case.get("mode").and_then(Value::as_str).unwrap_or("folds");
        let style = case.get("href").and_then(Value::as_str).unwrap_or("walk");
        let href = link(style).ok_or_else(|| format!("href: {style} is not walk or viewer"))?;
        let clock = Clock::new(None, case.get("localTimeZone").and_then(Value::as_str));
        let tree = ax_of(case.get("node"), &path, &href, mode, &clock);
        serde_json::to_string(&tree).map_err(|e| e.to_string())
    })
}

/// One fold's HTML, from its accessibility tree: `{"html": ...}`.
pub fn html_text(input: &str) -> String {
    json_call(input, |ax| Ok(json!({ "html": crate::html::html(&ax)? }).to_string()))
}

unsafe fn call(ptr: *const u8, len: usize, f: fn(&str) -> String) -> u64 {
    let input = std::str::from_utf8(std::slice::from_raw_parts(ptr, len)).unwrap_or("");
    let out = f(input).into_bytes().into_boxed_slice();
    let (n, p) = (out.len(), Box::into_raw(out) as *mut u8);
    ((p as u64) << 32) | n as u64
}

/// # Safety
/// `ptr` and `len` are UTF-8 the host wrote into a buffer from `alloc(len)`.
#[no_mangle]
pub unsafe extern "C" fn fold_json(ptr: *const u8, len: usize) -> u64 {
    call(ptr, len, fold_text)
}

/// # Safety
/// `ptr` and `len` are UTF-8 the host wrote into a buffer from `alloc(len)`.
#[no_mangle]
pub unsafe extern "C" fn walk_json(ptr: *const u8, len: usize) -> u64 {
    call(ptr, len, walk_text)
}

/// # Safety
/// `ptr` and `len` are UTF-8 the host wrote into a buffer from `alloc(len)`.
#[no_mangle]
pub unsafe extern "C" fn ax_json(ptr: *const u8, len: usize) -> u64 {
    call(ptr, len, ax_text)
}

/// # Safety
/// `ptr` and `len` are UTF-8 the host wrote into a buffer from `alloc(len)`.
#[no_mangle]
pub unsafe extern "C" fn html_json(ptr: *const u8, len: usize) -> u64 {
    call(ptr, len, html_text)
}
