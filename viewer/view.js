// The viewer's logic outside the DOM: fetch one JSON document from the data origin, fold it here with the core, and say which fold a
// location shows and as what tree. The app holds no data and its server serves none (COGNITIVE-BROWSER-SDK-2026-10-30): the document
// and the files it names come from their own URL. index.html wires this to the page; viewer.test.ts runs it on every golden.

/** The path a location's hash names (`#/a/b`, `#slides/a/b`), each id decoded on its own, as the core's "viewer" links encode them
 * (an empty id is a step too: `#/a//b`). */
export const pathOf = (hash) => {
  const s = hash.replace(/^#(slides)?\/?/, "");
  return s ? s.split("/").map(decodeURIComponent) : [];
};
export const docId = (p) => (p.length ? p.join("~") : "root");
/** As a deck (`#slides/…`, or a page whose root presents as slides opened with no path) a fold is a slide. */
export const modeOf = (hash, node) => (/^#slides/.test(hash) || (!hash && node?.present === "slides") ? "slides" : "folds");
/** The core's "viewer" link (ax.rs `link`): the same, so a location this makes is one pathOf reads back. */
export const href = (p, mode) => (mode === "slides" ? "#slides/" : "#/") + p.map(encodeURIComponent).join("/");
export const at = (id) => (id === "root" ? [] : id.split("~"));

/** Every `from` a page names, as fold.ts's withSources reads them (a loaded source's own `from` is not followed). */
export const froms = (o) =>
  Array.isArray(o) ? o.flatMap(froms) : o && typeof o === "object" ? [...(typeof o.from === "string" && o.from.trim() ? [o.from] : []), ...Object.values(o).flatMap(froms)] : [];

/**
 * The document at `url` (absolute), folded: its `from` files are fetched relative to it, and one that cannot be fetched is left out,
 * so the core notes it as fold.ts does. `opts(modified)` gives the fold's options from the document's Last-Modified (an ISO time, or
 * null when it has none). `stored(at)` is told when the copy read was kept (sw.js's Fold-Stored, an ISO time), or null for a fresh one.
 */
export async function open(fold, url, { fetch = globalThis.fetch, opts = () => ({}), stored = () => {} } = {}) {
  const r = await fetch(url, { cache: "no-cache" });
  if (!r.ok) throw new Error(`${url}: ${r.status} ${r.statusText ?? ""}`.trim());
  const kept = r.headers?.get?.("fold-stored");
  stored(kept && !Number.isNaN(Date.parse(kept)) ? new Date(kept).toISOString() : null);
  const root = await r.json();
  const files = {};
  await Promise.all(froms(root).map(async (p) => {
    try { const f = await fetch(new URL(p, url).href, { cache: "no-cache" }); if (f.ok) files[p] = await f.text(); } catch {}
  }));
  const modified = r.headers?.get?.("last-modified");
  return new Map(fold({ root, files, opts: opts(modified && !Number.isNaN(Date.parse(modified)) ? new Date(modified).toISOString() : null) }));
}

/** The document this viewer reads: the fold-data meta's (`{path, source}`), or one named in the address (`?data=<path>`, as the
 * Reading box submits it) by an absolute path on this origin. Anything else is refused, never fetched: `{path, error}`. */
export function dataOf(search, base, meta) {
  const named = new URLSearchParams(search).get("data")?.trim();
  if (named == null || named === "") return { path: meta.path, url: new URL(meta.path, base).href, source: meta.source };
  const u = new URL(named, base);
  if (!/^\/(?![/\\])/.test(named) || u.origin !== new URL(base).origin) return { path: named, error: "only a path on this site, from /, is read" };
  return { path: u.pathname + u.search, url: u.href, source: null };
}

/** A source id as one segment of a haecceity (src/id.rs `segment_of`): every UTF-8 byte but `[A-Za-z0-9._-]` as `%XX`, uppercase. */
export const segmentOf = (id) =>
  [...new TextEncoder().encode(id)].map((b) => (/[A-Za-z0-9._-]/.test(String.fromCharCode(b)) && b < 0x80 ? String.fromCharCode(b) : `%${b.toString(16).toUpperCase().padStart(2, "0")}`)).join("");
/** The haecceity of the fold at `path` in the document `source` (src/id.rs `haecceity`): which thing a row shows. */
export const haecceityOf = (source, path) => `https://ipso.bounded.tools/${segmentOf(source)}${path.length ? "/" + path.map(segmentOf).join("/") : ""}`;

/** Each child row of the tree carries its thing's haecceity, as src/page.rs `mark` gives a pre-rendered page's rows, so a row is the
 * RDFa subject whose schema:about a Clax report anchors to. No element id is added: the viewer's own ids (reading, stale) stay unique. */
function markRows(ax, kids, path, source) {
  if (ax?.role === "listitem" && ax.index != null && kids[ax.index]) ax.haecceity = haecceityOf(source, [...path, kids[ax.index].id]);
  for (const c of ax?.children ?? []) markRows(c, kids, path, source);
  return ax;
}

/** What a location shows: its path, its mode, the fold there (undefined when none) and that fold's accessibility tree from the core.
 * Given the document's `source`, each child row carries its haecceity; without one (a document named in the address), none does. */
export function viewOf(ax, nodes, hash, localTimeZone, source = null) {
  const path = pathOf(hash), node = nodes.get(docId(path)), mode = modeOf(hash, node);
  const tree = ax({ node: node ?? null, path, mode, href: "viewer", localTimeZone });
  return { path, mode, node, tree: source && node ? markRows(tree, node.kids ?? [], path, source) : tree };
}

/** What the viewer says over a fold read from the copy kept at `at` (an ISO time) when its host did not answer: the time it is
 * from, in the reader's zone (with the day once it is not today), and its age in words. "Not updated since 14:05, 3 hours ago." */
export function since(at, now, timeZone) {
  // built from parts, so it reads the same under every ICU (each words a date and time joined its own way)
  const parts = (d) => Object.fromEntries(new Intl.DateTimeFormat("en-GB", { timeZone, weekday: "short", year: "numeric", month: "short", day: "numeric",
    hour: "2-digit", minute: "2-digit", hourCycle: "h23" }).formatToParts(d).map((p) => [p.type, p.value]));
  const t = new Date(at), p = parts(t), n = parts(now), time = `${p.hour}:${p.minute}`;
  const when = p.year === n.year && p.month === n.month && p.day === n.day ? time : `${p.weekday} ${p.day} ${p.month} at ${time}`;
  const m = Math.max(0, Math.floor((now - t) / 60_000)), rel = new Intl.RelativeTimeFormat("en", { numeric: "always" });
  const age = m < 1 ? "less than a minute ago" : m < 60 ? rel.format(-m, "minute") : m < 48 * 60 ? rel.format(-Math.floor(m / 60), "hour") : rel.format(-Math.floor(m / 1440), "day");
  return `Not updated since ${when}, ${age}.`;
}
