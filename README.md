# fold-core

One fold core: one JSON object (a status page, a deck, a page's Markdown) into fold-viewer nodes, in Rust, byte-identical to the
TypeScript fold viewer it was ported from. Pure: no clock, network or filesystem; the caller passes the time and the source files'
text.

    fold(root, opts) -> Vec<(id, Node)>      // insertion order, as the viewer's Map
    from_markdown(md), pages(lines), from_deck(deck), with_sources(value, read)

## Tests

    nix develop -c cargo test     # or: nix flake check

`tests/parity.rs` folds every `goldens/<case>.input.json` and requires `serde_json::to_string` of the result to equal that of
`goldens/<case>.folds.json`.

## Goldens

Every input is synthetic (`goldens/fixtures/*.json`). The expected folds come from the fold viewer's own `fold.ts` at the rev in
`goldens/REV`, by `FOLD_SRC=<its source tree> FOLD_REV=<rev> bun goldens/gen.ts`. Regenerate them only on purpose: a diff in them
is a change in behaviour.

| case | exercises |
|---|---|
| `sample` | a status page as written: needs items (action, why, link), goal and seat items (state, last, waits, alert), titled groups, a reference name (`Page thread #43`), quoted text, `empty`, times in `meta.timeZone` |
| `everything` | the same kind of page through the viewer's `everything()` top level: machines, open pull requests by repo (`widgets #123`), CI under Machines, the fleet's state from scoped health lines, an empty group, sections with only `empty` |
| `many` | `More of this page` at the top, `More in …` for a short list, `part-N` runs two levels deep (37 and 120 items), id dedupe, ids the fold makes itself taken (`root`, `more`, `part-2`), untitled groups as layout, labels from `nav`/`action`/`label`, `Untitled`, non-ASCII and emoji labels cut by `short()` |
| `markdown` | a `from` Markdown file: headings and nesting, a ~3,000-word section split into screenfuls (list items, paragraphs, then sentences), `p2…` folds, a heading ending in `\r`, a second `#`, `#######`, and a missing file's note |
| `deck` | a root presenting as folds holding a `from` fold page that presents as slides, which holds a third deck: each sequence starts and ends at its own folds; string `text` from a source; a `{meta, slides}` deck through `from_deck` (kickers, transcript pages, markup stripped) |
| `fields` | alerts (why, waits and last, neither), labelled fields, times near midnight and with offsets, a non-time in `at`, reference names with and without text, a self row and look-alikes, hints over 90 code units with and without a sentence end and with emoji, duplicate ids, `empty` with and without kids, `stale` |

## Known limits against JS

A cut through a surrogate pair (a lone surrogate in JS) becomes U+FFFD; `new Date()` is reproduced for ISO-like forms only
(JavaScriptCore's free-form fallback is not); a time with no offset, or a page with no `meta.timeZone`, is read in
`opts.localTimeZone` (UTC when absent) where JS uses the machine's zone; JSON parse error messages in a `from` note differ from
JavaScriptCore's wording.
