# fold-core

One fold core: one JSON object (a status page, a deck, a page's Markdown) into fold-viewer nodes, in Rust, byte-identical to the
TypeScript fold viewer it was ported from. Pure: no clock, network or filesystem; the caller passes the time and the source files'
text.

    fold(root, opts) -> Vec<(id, Node)>      // insertion order, as the viewer's Map
    from_markdown(md), pages(lines), from_deck(deck), with_sources(value, read)
    walk::walk(nodes, opts) -> Walk               // walk.ts: the artificial viewer's findings, ranked
    walk::verdict(walk), walk::facto_body(...)    // its one line for the page, and its facto Observations
    ax::ax_of(node, path, href, mode, clock)      // fold-ax.js axOf: one fold's accessibility tree; ax::spoken(tree)

## WebAssembly

`nix build .#wasm` gives `lib/fold_core.wasm` (a plain C ABI, src/wasm.rs) and `lib/fold-core.ts`, its loader:
`const {fold, walk} = await core(bytes)`: `fold({root, files, opts})` returns the fold list exactly as a golden holds it (`files` maps
each `from` path to its text); `walk({nodes, opts, facto?})` returns `{walk, verdict, facto?}` as a walk golden holds it, where
`opts.budgets` is budgets.json's `budgets` (the walk reads `view.words`) and `opts.localTimeZone` the machine's zone; `ax({node, path, mode, href, localTimeZone?})` returns one fold's accessibility tree
(fold-ax.js `axOf`), `href` naming the links it is given: `"walk"` (`#/a/b`) or `"viewer"` (each id through `encodeURIComponent`). `nix flake check` runs js/parity.test.ts against every golden with the built module.

The module carries no zone data. It imports `fold_core.tz_offset(zone_ptr, zone_len, epoch_ms) -> seconds` (`i32::MIN` for an
unknown zone), and the loader answers it from `Intl`, the same zone rules fold.ts reads. A host that loads the module without
`core()` has to provide that import. The native library builds the rules in instead (the default `tzdb` feature, chrono-tz).
`checks.wasm-size` holds the module to 200 KB gzipped: it is about 262 KB raw and 115 KB gzipped (it was 2.1 MB raw and 539 KB
gzipped with chrono-tz and regex built in).

## Viewer

`nix build .#viewer` gives the fold viewer as a static app (`viewer/`; step 1 of ~/.lobby/ops/decisions/COGNITIVE-BROWSER-SDK-2026-10-30.md):
`index.html`, `view.js` (routing, the document fetch, the fold), `fold-dom.js` (the DOM from an accessibility tree, from home's
fold-ax.js), the module, its loader as browser JS (`fold-core.js`) and the one face. It holds no data. It fetches one JSON document,
the `fold-data` meta's URL (`/fold-data/fleet/page.json`, from a data path the app is not served from), and the files its `from`s
name relative to it; folds it in the browser with the core; and shows each fold through the core's tree (`ax`, the "viewer" links).
It reads the document again each minute. Served as is: at `/fold/` on phobos by `tailscale serve`.

`checks.viewer` runs `viewer/viewer.test.ts` on every golden: the document fetched from a data URL folds to the golden folds; every
location the viewer links to shows the golden tree; every fold derives its DOM with no refusal (the AX gate); the walk over what it
shows is the golden walk (the load gate). `checks.viewer-browser` (Linux) serves the package at `/fold/` and the `sample` page as its
data, and loads it in headless Chromium at 390×844 and 1024×1366: each location must show the fold the core folds there.

## Pages at real paths

`site({nodes, source, base?, localTimeZone?})` (src/page.rs, `site_json` in the module) writes every fold of a folded document as a
whole page of its own, for a host to serve as static files (BRIEF-fold-render-anywhere-2026-10-30, slice 2). The pages work with JS
off. A fold is at `/fold/<id>/<id>/` (each id one segment, id.rs `segment_of`) in the file `<id>/<id>/index.html`, and a fold that
presents as slides also has `…/@slides/`.

- **Title:** each page's `<title>` names the fold and the path it is in.
- **Links:** moves between folds are plain links in the same tab. Each ends in `#<id>`: going in, the fold's `<h1>`, which a browser
  focuses as it loads; going back up the path, the row you came from. No `autofocus`, which lone flags as focus theft.
- **Ids:** the h1 and each child row have the HTML id of their path under the document; a row's `data-haecceity` is its IRI (id.rs).
- **Stylesheet:** every page links `viewer/fold.css`, the viewer's own.
- **Refusals:** an id no file name can hold (empty, `.`, `..`, or holding `/`) is refused, as is a tree toDom refuses.

`tests/pages.rs` holds every walk case's pages to `goldens/pages/<case>.pages.json`, and `js/parity.test.ts` holds the WASM build to
them. `js/html.test.ts` has lone bless each page's `<main>`. `checks.pages-browser` (Linux, `viewer/pages-check.ts`) loads every page
in headless Chromium with JS off, in light and dark. Each page must have the right title, focus its h1 when entered by a link, and
have links that land. It must also pass lone's colour contrast over the colours Chromium computes from fold.css. Known findings are
in `goldens/pages/known.json`.

## Schema

`schema/node.schema.json` is the fold node types (`Node`, `Kid`, `Step`) as JSON Schema (draft 2020-12), generated from them by
`tests/schema.rs`. An absent field is left out, never null, so optional fields are optional, not nullable. The test fails when the types
change and the file was not regenerated (`FOLD_SCHEMA_WRITE=1 cargo test --test schema`), and every golden's folds must read as the
types and write back the same. A consumer takes it from here instead of defining its own: `cue import jsonschema: schema/node.schema.json`
gives the CUE definitions.

## Tests

    nix develop -c cargo test     # or: nix flake check

`tests/parity.rs` folds every `goldens/<case>.input.json` and requires `serde_json::to_string` of the result to equal that of
`goldens/<case>.folds.json`; `tests/ax.rs` builds every tree in `goldens/ax/<case>.ax.json` from that case's walk input; `tests/walk.rs` does the same for `goldens/walk/<case>.input.json` against `<case>.walk.json`.

## Goldens

Every input is synthetic (`goldens/fixtures/*.json`, `goldens/walk/fixtures/*.json`). The expected folds and walks come from the fold
viewer's own `fold.ts` and `walk.ts` at the rev in `goldens/REV`, by `TZ=UTC FOLD_SRC=<its source tree> FOLD_REV=<rev> bun goldens/gen.ts`.
Every fold case's folds are walked too (`walk/fold-<case>`), and every walk case's folds go through `axOf` as folds and (when they
present) as slides, with both links, plus a fold with no data (`ax/<case>.ax.json`, one `[id, mode, href, tree]` a line). Regenerate them only on purpose: a diff in them
is a change in behaviour.

| case | exercises |
|---|---|
| `sample` | a status page as written: needs items (action, why, link), goal and seat items (state, last, waits, alert), titled groups, a reference name (`Page thread #43`), quoted text, `empty`, times in `meta.timeZone` |
| `everything` | the same kind of page through the viewer's `everything()` top level: machines, open pull requests by repo (`widgets #123`), CI under Machines, the fleet's state from scoped health lines, an empty group, sections with only `empty` |
| `many` | `More of this page` at the top, `More in …` for a short list, `part-N` runs two levels deep (37 and 120 items), id dedupe, ids the fold makes itself taken (`root`, `more`, `part-2`), untitled groups as layout, labels from `nav`/`action`/`label`, `Untitled`, non-ASCII and emoji labels cut by `short()` |
| `markdown` | a `from` Markdown file: headings and nesting, a ~3,000-word section split into screenfuls (list items, paragraphs, then sentences), `p2…` folds, a heading ending in `\r`, a second `#`, `#######`, and a missing file's note |
| `deck` | a root presenting as folds holding a `from` fold page that presents as slides, which holds a third deck: each sequence starts and ends at its own folds; string `text` from a source; a `{meta, slides}` deck through `from_deck` (kickers, transcript pages, markup stripped) |
| `md-pages` | a section split at words (a 400-word list item with no full stop), a section long enough that its page size grows to the cap and still pages twice, subsections after the pages, a section that fits one screen |
| `act` | a fold's one action (`{cap, call, args, label}`): label trimmed, cap and call as written, args an object or array; missing label, null args or a blank cap give none |
| `fields` | alerts (why, waits and last, neither), labelled fields, times near midnight and with offsets, a non-time in `at`, reference names with and without text, a self row and look-alikes, hints over 90 code units with and without a sentence end and with emoji, duplicate ids, `empty` with and without kids, `stale` |

| walk `checks` | every check once: unexplained states (bare, with punctuation, labelled or with a reason: not), no-intent, choices, dangling, unreachable, cut hints (`…` and `...`), ruled words in hints and text; its facto Observations |
| walk `words` | the `view.words` budget at 220 (passes) and 221 (blocks), text as one string, a leaf with only an href, titles of three and four words, a blank title |
| walk `inline` | a kept page read inline (`artifacts~…`): nothing blocks but fold-bytes; nested runs on the page and inline; fold docs over 8 KB |
| walk `slides` | folds that `present`: slide words over budget, prev/next that exist and that dangle; every property row (stale, act with and without a label, href and hrefLabel, health, checked, "Data from" with and without a source, times with offsets, no offset, a date alone and a non-time) |
| walk `order` | no root, every fold unreachable, ranked by id as `localeCompare` orders it (punctuation, digits, case) |
| `zones`, `zones-page` | times in the machine's zone (`opts.localTimeZone`): a DST gap (moves forward) and overlap (the earlier), offsets, `Z`, a date alone, `24:00`, 1883; a page in its own zone with a +12:45 offset |
| walk `encoded` | ids the viewer's links encode: a space, past ASCII, emoji, URL delimiters, the marks `encodeURIComponent` keeps |
| walk `zones` | "Data from" times in a gap and an overlap, in the machine's zone |
| walk `repeats` | a repeated id (first place, last node) and a cycle back to a fold already walked |

## Known limits against JS

A cut through a surrogate pair (a lone surrogate in JS) becomes U+FFFD; `new Date()` is reproduced for ISO-like forms only
(JavaScriptCore's free-form fallback is not); a time with no offset, or a page with no `meta.timeZone`, is read in
`opts.localTimeZone` (UTC when absent) where JS uses the machine's zone; JSON parse error messages in a `from` note differ from
JavaScriptCore's wording. The walk's ranking orders fold ids as ICU does for ASCII; past ASCII it orders by code point (ICU
interleaves; ids are slugs). A fold's `updated` that is not a string reads as "Invalid Date" (JS would read a number as epoch ms).
