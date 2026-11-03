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
 * `root(r)` is given the document before it is folded, and what it returns (or resolves to) is folded (Needs you read live: the core's
 * `needs`, which may wait for its own read, begun beside this one).
 */
export async function open(fold, url, { fetch = globalThis.fetch, opts = () => ({}), stored = () => {}, root: live = (r) => r } = {}) {
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
  const whole = await live(root);
  return new Map(fold({ root: whole, files, opts: opts(modified && !Number.isNaN(Date.parse(modified)) ? new Date(modified).toISOString() : null) }));
}

// ── Live folds (BRIEF-stint-2026-11-03-basis-live-folds Part A) ───────────────────────────────────────────────────────────────────
// Each fold may have its own live node, `nodes/<id>.json` beside the document ({id, source, updated, ok, stale_since, cadence_s,
// data}), written whole by its own handler and served by fold serve with a strong ETag and no-cache (src/serve.rs NODES). The viewer
// reads each on open and revalidates it with If-None-Match: a 304 costs a round trip and nothing else (no fold, no render); a 200
// replaces that one fold's subtree and re-renders only when the fold in view is under it.

/** One JSON object read, or revalidated against `etag` (the last one held, or null): `{status: 304}` when unchanged, `{status: 200,
 * etag, body}` when it changed, `{status, error}` when it could not be read. The browser's own cache is bypassed (`no-store`), so a
 * 304 is seen as one and never turned into a 200 from cache. */
export async function revalidate(url, etag, { fetch = globalThis.fetch } = {}) {
  try {
    const r = await fetch(url, { cache: "no-store", headers: etag ? { "If-None-Match": etag } : {} });
    if (r.status === 304) return { status: 304 };
    if (!r.ok) return { status: r.status, error: `${r.status} ${r.statusText ?? ""}`.trim() };
    const body = await r.json();
    if (!body || typeof body !== "object" || Array.isArray(body)) return { status: r.status, error: "not a JSON object" };
    return { status: 200, etag: r.headers?.get?.("etag") ?? null, body };
  } catch (e) {
    return { status: 0, error: String(e?.message ?? e) };
  }
}

const KID_KEYS = ["sections", "groups", "items", "kids", "children"];
/** The document with each live node's `data` put where its id says (`fleet/inbox`: the child `inbox` of the root's child `fleet`),
 * replacing the fold of that id or, when there is none, added to the parent's first list of children. The document is not changed:
 * the path to each node is copied. A node with no data, or whose parent is not there, is left out. */
export function graft(root, live) {
  let out = root;
  for (const n of live) {
    if (!n?.data || typeof n.data !== "object" || typeof n.id !== "string") continue;
    const ids = n.id.split("/"), put = { ...n.data, id: ids[ids.length - 1] };
    const into = (o, i) => {
      if (!o || typeof o !== "object") return null;
      const c = { ...o };
      for (const k of KID_KEYS) {
        if (!Array.isArray(c[k])) continue;
        const j = c[k].findIndex((x) => x && typeof x === "object" && x.id === ids[i]);
        if (j < 0) continue;
        const next = i === ids.length - 1 ? put : into(c[k][j], i + 1);
        if (!next) return null;
        c[k] = c[k].map((x, m) => (m === j ? next : x));
        return c;
      }
      if (i !== ids.length - 1) return null;
      const k = KID_KEYS.find((k) => Array.isArray(c[k])) ?? "sections";
      c[k] = [...(c[k] ?? []), put];
      return c;
    };
    out = into(out, 0) ?? out;
  }
  return out;
}

/** The folded map's ids at or under a live node's fold (its path as the viewer's ids, docId's `~`). */
export const underNode = (nodeId, foldId) => {
  const p = docId(nodeId.split("/"));
  return foldId === p || foldId.startsWith(p + "~");
};

/** Each fold from a live node says that node's source and time ("Data from <source>, <time>", src/ax.rs), not the document's; and a
 * node that is not ok, or stale since a time, says so on its own fold. `folds` is changed in place and returned. */
export function stamp(folds, live) {
  for (const n of live) {
    if (typeof n?.id !== "string") continue;
    for (const [id, f] of folds) {
      if (!underNode(n.id, id)) continue;
      if (typeof n.source === "string" && n.source.trim()) f.source = n.source;
      if (typeof n.updated === "string" && !Number.isNaN(Date.parse(n.updated))) f.updated = new Date(n.updated).toISOString();
      if (id === docId(n.id.split("/")) && (n.ok === false || n.stale_since)) f.stale = n.stale_since ? `Stale since ${n.stale_since}` : "Last read failed";
    }
  }
  return folds;
}

/** Whether a changed node shows on the fold at `path`: that fold is under the node, or is the node's parent (whose row for it may
 * have changed its name or hint). Any other fold is left as it is drawn. */
export function shows(nodeId, path) {
  const n = nodeId.split("/"), here = docId(path);
  return underNode(nodeId, here) || here === docId(n.slice(0, -1));
}

/** How long until a node is asked again: its own `cadence_s` (at least 5 s), else 60 s. */
export const cadenceOf = (n) => (Number.isFinite(n?.cadence_s) && n.cadence_s > 0 ? Math.max(5, n.cadence_s) : 60) * 1000;

/**
 * The live nodes under `index` (fold serve's `nodes/`, absolute): read once from the index, then each revalidated on its own cadence
 * (`later(fn, ms)`, setTimeout by default) and whenever `check()` is called (the page's focus and visibility). `changed(ids)` is
 * called with the ids whose node a 200 replaced, never for a 304 or a failed read (the last good node is kept, and said stale by
 * stamp when its handler says so). `nodes()` is every node held now, for graft and stamp.
 */
export function liveNodes(index, { fetch = globalThis.fetch, later = (f, ms) => setTimeout(f, ms), changed = () => {} } = {}) {
  const held = new Map(); // id -> { url, etag, node }
  let indexEtag = null;
  const one = async (id) => {
    const h = held.get(id);
    const r = await revalidate(h.url, h.etag, { fetch });
    if (r.status !== 200 || r.body.id !== id) return false;
    held.set(id, { ...h, etag: r.etag, node: r.body });
    return true;
  };
  const tick = (id) => later(async () => { if (held.has(id)) { if (await one(id)) changed([id]); tick(id); } }, cadenceOf(held.get(id)?.node));
  /** The index again (one request, a 304 when no node changed), then each node it lists whose ETag is not the one held. */
  const check = async () => {
    const r = await revalidate(index, indexEtag, { fetch });
    if (r.status !== 200) return [];
    indexEtag = r.etag;
    const fresh = [];
    for (const row of Array.isArray(r.body.nodes) ? r.body.nodes : []) {
      if (typeof row?.id !== "string" || typeof row.href !== "string") continue;
      const h = held.get(row.id);
      if (h && h.etag === row.etag) continue;
      const first = !h;
      held.set(row.id, { url: new URL(row.href, index).href, etag: h?.etag ?? null, node: h?.node ?? null });
      if (await one(row.id)) fresh.push(row.id);
      if (first) tick(row.id);
    }
    if (fresh.length) changed(fresh);
    return fresh;
  };
  return { check, nodes: () => [...held.values()].map((h) => h.node).filter(Boolean) };
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

/** The fold beside the one at `path` (`dir` 1 the next, -1 the previous), among its parent's child folds: `]]` and `[[`, vim's section
 * motions (RECOMMEND-consultant-2026-11-01-reversible-no-approval-and-sign-in-basis S5). From a group (Needs you › Approvals) that is
 * the next group; from an item, the next item. Path arithmetic only: no cursor is kept. Null at either end, at the root, or where
 * the parent's next row is not a fold of its own (a "back to the top" link). */
export function beside(nodes, path, dir) {
  if (!path.length) return null;
  const up = path.slice(0, -1), kids = nodes.get(docId(up))?.kids ?? [];
  const i = kids.findIndex((k) => k.id === path[path.length - 1]);
  const k = i < 0 ? null : kids[i + dir];
  return k && !k.href ? [...up, k.id] : null;
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
