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
  focuses as it loads; going back up the path, the row you came from. No `autofocus`, which lone flags as focus theft. No `ping`: with no
  `fold sink` deployed, every followed link would POST into a 405; it comes back only when a sink is deployed for a real write.
- **Acts:** a fold's act is a `<form>` named by the fold, posting its cap, call and args as hidden fields to `<base>act/<path>/<index>`
  with a native submit button, and no script (BRIEF-basis-forms-push-signing-2026-10-31 §1). The passkey (`webauthn`, `approve`)
  GETs the fold's sign page instead, which runs the ceremony.
- **A way back:** a missing fold, one whose data failed to load (`failed`), and a root with nothing in it each show the path (to the
  root at least), one line saying what was asked for and why there is nothing, and, off the root, a link back to it (§5).
- **Ids:** the h1 and each child row have the HTML id of their path under the document; a row is an RDFa `schema:WebPageElement` (`resource="#<id>"`) whose `schema:about` (an empty span's `resource`) is the IRI of the thing it shows (id.rs, brief item 20). The id is the element's; the IRI is the data's; the row is never the data.
- **Stylesheet:** every page links `viewer/fold.css`, the viewer's own.
- **Refusals:** an id no file name can hold (empty, `.`, `..`, or holding `/`) is refused, as is a tree toDom refuses.

`tests/pages.rs` holds every walk case's pages to `goldens/pages/<case>.pages.json`, and `js/parity.test.ts` holds the WASM build to
them. `js/html.test.ts` has lone bless each page's `<main>`. `checks.pages-browser` (Linux, `viewer/pages-check.ts`) loads every page
in headless Chromium with JS off, in five modes: light, dark, both again under `prefers-contrast: more`, and `forced-colors`. Each
page must have the right title, focus its h1 when entered by a link, and have links that land. It must also pass lone's colour
contrast over the colours Chromium computes from fold.css, clear 7:1 under `prefers-contrast: more`, and give every button and field a
visible border. Known findings are in `goldens/pages/known.json`.

The colours themselves are proven before any browser runs them. `tests/contrast.rs` checks `viewer/theme.tokens.json`, a Baobab
pinning, through `src/baobab.rs`:
- light and dark: text 4.5, UI 3;
- light-more and dark-more: text 7, UI 4.5;
- forced: every role is a CSS system colour, and every pair is one the platform guarantees.

The same test writes the fold.css block and `native/Colors.xcassets` (any, dark and Increase Contrast, for Apple hosts).
`js/contrast.test.ts` runs Baobab's own TS over the same pairs, and it must agree.

## fold serve and fold sink

Two processes (src/bin/fold.rs), each on 127.0.0.1 for `tailscale serve` to put at a mount, neither run as root (brief item 27: one
binary holding every read and write was the riskiest call). Approvals go through neither; they stay chitin's `/approve`.

`fold serve --root <dir> --mount /fold --port <n> [--no-store <prefix>]...` (src/serve.rs) is read-only: GET and HEAD of one
directory's files, and nothing else. It writes nothing, takes no body (413), and answers every other method 405. The one body it
takes is an act's form (`POST <mount>/act/<path>/<act key>`, at most 4 KiB). Without the act flags, that form is read only to be
dropped, and the answer is a page that says acting from here isn't on yet, with the way back (goldens/serve/act.html).

With `--act-as <login> --act-host <host> --act-key <file> --chitin <bin> [--act-seat fold-act] [--act-log <file>]`, the form can
ack, defer or decline a chitin message for that one login (S2: allie-s2-fold-act-write). That's all it can do: a passkey act is
422 and stays on chitin's `/approve`. Before the actor runs, every check must pass:
- the peer is loopback;
- `Tailscale-User-Login` is the login;
- the request came through serve (`X-Forwarded-Proto`, `X-Forwarded-Host`) and is same-origin (`Sec-Fetch-Site`, else `Origin`);
- the form's token (src/act.rs: HMAC over the act id, its fixity, expiry and login, 12 hours) is this server's, current, and for
  the act as it stands.

The act comes from the site's act map (`_acts.json`, written by `site()` and never served), not from the form. Success is a `303`
back to the fold, whose status line then says what was done. A stale token, or a fold that changed, gets the page back with
fresh tokens and nothing done. Each POST leaves one line in `acts.jsonl` (0600). goldens/serve/acts.json holds every refusal
and success, and tools/act-e2e.sh (`checks.act-e2e`) runs one ack end to end with curl and a stub chitin. It replaces
fold-serve's darkhttpd agents (brief items 23-24), which send no ETag, no per-file Cache-Control and no custom headers.

| Resource | Cache-Control | Validator |
|---|---|---|
| a path holding `@sha256:` | `public, max-age=31536000, immutable` | strong ETag |
| a page (`.html`, a directory's `index.html`) | `no-cache` | strong ETag `"sha-256:<hex>"`, the file digest (304 on `If-None-Match`), `Last-Modified` |
| a data document (`.json`) | `max-age=60, stale-while-revalidate=240, stale-if-error=86400` | the same |
| any other file (viewer JS, CSS, WASM, fonts, `sw.js`) | `no-cache` | the same |
| a `--no-store` prefix (the Sign fold, pending, presence), every error | `no-store` | none |

- Every file also gets `Content-Digest` (the same digest) and `Server-Timing: digest;desc="sha-256:…", asof;desc="…", serve;dur=…`.
- No page names a reporting endpoint or a report-only CSP: nothing is deployed to take them.
- `tests/serve.rs` holds every class's headers in `goldens/serve/headers.json`.

`fold sink --port <n> --log <signals.jsonl> [--per-minute 60]` (src/sink.rs) is append-only, put at `<mount>/_sink`:

- `POST /v` (`text/ping`, from `<a ping>`) logs a ping, with `Ping-From` and `Ping-To`: a view. A prefetch never pings, so it is
  never a view. `POST /report` (`application/reports+json`) logs the reports.
- A request with no `Tailscale-User-Login` is refused (403); each viewer gets `--per-minute` a clock minute, then 429.
- Each record is one JSON line appended to `--log`; nothing is read back or served. `tests/sink.rs` holds its answers and records in
  `goldens/sink/answers.json`.

`tests/fuzz.rs` throws request-shaped bytes at both (`FOLD_FUZZ=<n>` cases a side): neither panics or answers outside its statuses,
serve never serves a byte from outside its root nor writes into it, and the sink logs only what it accepted from an identity.

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
