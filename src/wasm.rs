//! The core as a WebAssembly module with a plain C ABI, so any JS runtime (bun, deno, a browser) loads it with
//! `WebAssembly.instantiate` and no generated glue (js/fold-core.ts is the loader).
//!
//!   alloc(len) -> ptr           the host writes its UTF-8 input there
//!   fold_json(ptr, len) -> u64  folds `{root, files, opts}` (the shape of goldens/<case>.input.json) and returns
//!                               (out_ptr << 32) | out_len: `[[id, node], ...]` in insertion order, or `{"error": "..."}`
//!   dealloc(ptr, len)           frees either buffer
//!
//! `files` maps each `from` path to its text; a path not in it reads as missing, with fold.ts's ENOENT note.

use serde_json::Value;

use crate::{fold, with_sources, FoldOpts};

#[no_mangle]
pub extern "C" fn alloc(len: usize) -> *mut u8 {
    let mut b = Vec::<u8>::with_capacity(len.max(1));
    let p = b.as_mut_ptr();
    std::mem::forget(b);
    p
}

/// # Safety
/// `ptr` and `len` are a buffer this module returned (from `alloc`, or packed in `fold_json`'s result), freed once.
#[no_mangle]
pub unsafe extern "C" fn dealloc(ptr: *mut u8, len: usize) {
    drop(Vec::from_raw_parts(ptr, 0, len.max(1)));
}

/// Folds one `{root, files, opts}` JSON text into its fold list's JSON text.
pub fn fold_text(input: &str) -> String {
    let run = || -> Result<String, String> {
        let case: Value = serde_json::from_str(input).map_err(|e| format!("input is not JSON: {e}"))?;
        let opts: FoldOpts = match case.get("opts") {
            Some(o @ Value::Object(_)) => serde_json::from_value(o.clone()).map_err(|e| format!("opts: {e}"))?,
            _ => FoldOpts::default(),
        };
        let files = case.get("files").and_then(Value::as_object).cloned().unwrap_or_default();
        let read = |p: &str| files.get(p).and_then(Value::as_str).map(str::to_string);
        let root = case.get("root").ok_or("no root")?;
        serde_json::to_string(&fold(&with_sources(root, &read), &opts)).map_err(|e| e.to_string())
    };
    run().unwrap_or_else(|e| serde_json::json!({ "error": e }).to_string())
}

/// # Safety
/// `ptr` and `len` are UTF-8 the host wrote into a buffer from `alloc(len)`.
#[no_mangle]
pub unsafe extern "C" fn fold_json(ptr: *const u8, len: usize) -> u64 {
    let input = std::str::from_utf8(std::slice::from_raw_parts(ptr, len)).unwrap_or("");
    let out = fold_text(input).into_bytes().into_boxed_slice();
    let (n, p) = (out.len(), Box::into_raw(out) as *mut u8);
    ((p as u64) << 32) | n as u64
}
