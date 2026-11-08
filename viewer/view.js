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
    const ids = n.id.split("/"), last = ids[ids.length - 1];
    const put = (old) => (n.merge ? merged(old, n, last) : { ...n.data, id: last });
    const into = (o, i) => {
      if (!o || typeof o !== "object") return null;
      const c = { ...o };
      for (const k of KID_KEYS) {
        if (!Array.isArray(c[k])) continue;
        const j = c[k].findIndex((x) => x && typeof x === "object" && x.id === ids[i]);
        if (j < 0) continue;
        const next = i === ids.length - 1 ? put(c[k][j]) : into(c[k][j], i + 1);
        if (!next) return null;
        c[k] = c[k].map((x, m) => (m === j ? next : x));
        return c;
      }
      if (i !== ids.length - 1) return null;
      const k = KID_KEYS.find((k) => Array.isArray(c[k])) ?? "sections";
      c[k] = [...(c[k] ?? []), put(null)];
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
      if (id === docId(n.id.split("/")) && n.madeBy && typeof n.madeBy === "object") f.madeBy = n.madeBy; // "Made by <query>" (src/ax.rs)
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

// ── Folds by reference (BRIEF-stint-2026-11-03-basis-live-folds Addenda 1–3) ─────────────────────────────────────────────────────
// One JSON-LD file per fold (`/fold-data/<page>/<id>.jsonld`, written by rename, served with its own ETag). The format, as fold-core
// takes it (work-live-folds-home-stint's proposal, adopted):
//   Fold          {"@context", "@id": "fold:<page>/<id>", "@type": "Fold", id, title, source, updated, ok, stale_since, error?,
//                  madeBy: {query, version, href, frame?: {id, version}}, data: [the fold's rows]}
//   FoldManifest  {"@context", "@id", "@type": "FoldManifest", id, title, source, updated, ok, parts: [FoldRef]}
//   FoldRef       {"@id", "@type": "FoldRef", url (relative to the manifest), etag (last known, or null), updated (or null)}
// A manifest lists its child folds by reference, never embeds them. The viewer reads the top manifest, then each part on its own: a
// part whose ref names the ETag already held is not asked again; a slow part never holds its parent or its siblings (each lands and
// is said changed on its own); a missing, unreadable or cyclic part, or one with no derivation, is a notice in its own slot.

const isManifest = (d) => d?.["@type"] === "FoldManifest";

/** A fold document's id in its page: its `id`, else its `@id`'s last segment (`fold:fleet/needs`: `needs`). */
export const foldKey = (d) => (typeof d?.id === "string" && d.id ? d.id : typeof d?.["@id"] === "string" ? d["@id"].split("/").pop() : null);

/**
 * Why a fold may not be drawn, or null when it may (Addendum 3: a fold is only what a named query, under a frame, made, and it says
 * so): `madeBy` names the query, its version, and where its text is (`href`); a frame, when named, has an id and a version; and the
 * fold says when it was made (`updated`, its source's as-of). A manifest is drawn by its parts, each checked as it is read.
 * Provisional: the facto consultant's enforcement points (BRIEF-consultant-2026-11-03-facto-handlers Addendum 6) set the final shape;
 * the frame is not yet required because no writer has one yet.
 */
export function underived(d) {
  if (isManifest(d)) return null;
  const m = d?.madeBy;
  if (!m || typeof m !== "object") return "it names no query that made it";
  if (typeof m.query !== "string" || !m.query.trim()) return "its query has no name";
  if (!Number.isFinite(m.version) && !(typeof m.version === "string" && m.version.trim())) return `its query ${m.query} has no version`;
  if (typeof m.href !== "string" || !m.href.trim()) return `its query ${m.query} does not say where its text is`;
  if (m.frame != null && (typeof m.frame?.id !== "string" || m.frame.version == null)) return "its frame has no id and version";
  if (typeof d.updated !== "string" || Number.isNaN(Date.parse(d.updated))) return "it does not say when it was made";
  return null;
}

/** A manifest as a document to fold: its title, and one placeholder per part (in its order) until that part is read. */
export const manifestRoot = (m) => ({ title: m.title ?? foldKey(m) ?? "Folds", sections: (m.parts ?? []).map(placeholder) });
const placeholder = (p) => ({ id: foldKey(p) ?? "?", title: p?.title ?? foldKey(p) ?? "?", text: "Not read yet.", pending: true });

/** `old` (the fold there now, or null) with a read fold put in: its title and notice, and its rows (a Fold's) or its parts (a
 * manifest's, each the child already there or a placeholder) for children; anything else `old` says (purpose, empty) is kept. A
 * fold with no rows to give (unreadable, not drawn) keeps the children it had. */
function merged(old, n, id) {
  const out = { ...(old ?? {}), ...(n.data.title ? { title: n.data.title } : {}), id };
  if (out.pending) { delete out.pending; delete out.text; }
  if (n.data.text) out.text = n.data.text;
  if (Array.isArray(n.data.items) || Array.isArray(n.parts)) {
    const was = KID_KEYS.flatMap((k) => (Array.isArray(out[k]) ? out[k] : []));
    for (const k of KID_KEYS) delete out[k];
    if (Array.isArray(n.data.items)) out.items = n.data.items;
    else out.sections = n.parts.map((p) => was.find((x) => x?.id === foldKey(p)) ?? placeholder(p));
  }
  return out;
}

/** The path of the fold `key` in `root`: under `under` (the manifests' keys above it) when that path is there, else the first fold
 * of that id anywhere (depth first: page.json's Messages to you is under Agent fleet while its manifest is not), else under `under`. */
export function placeIn(root, key, under = []) {
  const kids = (o) => KID_KEYS.flatMap((k) => (Array.isArray(o?.[k]) ? o[k] : []));
  const walk = (o, path) => {
    for (const c of kids(o)) {
      if (!c || typeof c !== "object") continue;
      if (c.id === key) return [...path, key];
      const deeper = walk(c, [...path, c.id]);
      if (deeper) return deeper;
    }
    return null;
  };
  let o = root;
  for (const k of under) o = kids(o).find((c) => c?.id === k);
  if (o && kids(o).some((c) => c?.id === key)) return [...under, key];
  return walk(root, []) ?? [...under, key];
}

/**
 * The live nodes (as graft and stamp take them) for the folds read under a manifest, placed in `root`: each Fold's rows, said with
 * its source, time, freshness and the query that made it; one with no derivation, or a part that cannot be read or refers back to
 * a manifest above it, as a notice in its own slot. The top manifest is the document (or overlays it) and is not a node. Parents
 * come before children, so a manifest's placeholders are there for its parts to land in.
 */
export function foldNodes(root, folds) {
  return folds
    .filter((f) => f.chain.length)
    .map((f) => {
      const under = f.chain.slice(1).map((id) => foldKey({ "@id": id })), key = f.key, path = placeIn(root, key, under).join("/");
      const d = f.doc, title = d?.title;
      // refused or cyclic: no rows at all (never the document's own copy, which no query made); unreadable: what was there stays
      if (f.cycle) return { id: path, merge: true, data: { title, items: [], text: `Not followed: it refers back to itself (${f.cycle}).` } };
      if (!d) return { id: path, merge: true, ok: false, data: { title, text: `Not read: ${f.error ?? "no answer yet"}.` } };
      const why = underived(d);
      if (why) return { id: path, merge: true, data: { title, items: [], text: `Not shown: ${why}, so it cannot be checked.` } };
      const fresh = { source: d.source, updated: d.updated, ok: f.error ? false : d.ok !== false, stale_since: d.stale_since ?? null };
      if (isManifest(d)) return { id: path, merge: true, ...fresh, parts: d.parts ?? [], data: { title } };
      return { id: path, merge: true, ...fresh, madeBy: d.madeBy, data: { title, items: Array.isArray(d.data) ? d.data : [] } };
    })
    .sort((a, b) => a.id.split("/").length - b.id.split("/").length);
}

/**
 * The folds under the manifest at `top` (absolute), read by reference. `check({deep})` reads the top manifest again (a 304 when
 * nothing changed), then each part it lists whose ref ETag is not the one held (every part with `deep`, the page's focus), each on
 * its own: `changed(keys)` is called as each one lands with a change, never for a 304. A part that fails keeps its last good copy.
 * `watch()` holds a long poll on the top manifest (fold serve's `?wait=`, src/serve.rs `waited`) and checks as soon as it moves.
 * `folds()` is every fold held, `{key, chain, doc, error, cycle}`, for foldNodes.
 */
export function liveFolds(top, { fetch = globalThis.fetch, changed = () => {}, wait = 25 } = {}) {
  const held = new Map(); // url -> { key, chain: [@id of each manifest above], etag, doc, error, cycle }
  const read = async (url, chain, key, extra = "") => {
    const h = held.get(url) ?? { key, chain, etag: null, doc: null, error: null, cycle: null };
    held.set(url, { ...h, key, chain, cycle: null });
    const r = await revalidate(url + extra, h.etag, { fetch });
    if (r.status === 304) return false;
    if (r.status !== 200) {
      held.set(url, { ...held.get(url), error: r.error });
      return h.error !== r.error;
    }
    held.set(url, { ...held.get(url), key: foldKey(r.body) ?? key, etag: r.etag, doc: r.body, error: null });
    return true;
  };
  const visit = async (url, chain, key, deep, out, extra) => {
    if (await read(url, chain, key, extra)) { out.push(held.get(url).key); if (chain.length) changed([held.get(url).key]); }
    const h = held.get(url);
    if (!isManifest(h.doc)) return;
    const me = h.doc["@id"] ?? url, above = [...chain, me];
    await Promise.all((Array.isArray(h.doc.parts) ? h.doc.parts : []).map(async (p) => {
      if (typeof p?.url !== "string") return;
      const u = new URL(p.url, url).href, id = p["@id"] ?? u, k = foldKey(p) ?? u;
      if (above.includes(id)) {
        const slot = `${u} from ${me}`, was = held.get(slot); // its own slot: the fold it names is held (and drawn) where it is
        held.set(slot, { key: k, chain: above, etag: null, doc: null, error: null, cycle: [...above, id].join(" → ") });
        if (!was?.cycle) { out.push(k); changed([k]); }
        return;
      }
      const k0 = held.get(u);
      if (!deep && k0?.doc && p.etag && k0.etag === p.etag && !isManifest(k0.doc)) return; // the ref says it is the copy held
      await visit(u, above, k, deep, out, "");
    }));
  };
  const check = async ({ deep = false, extra = "" } = {}) => { const out = []; await visit(top, [], null, deep, out, extra); return out; };
  /** Long poll: one request held on the top manifest; when it moves, its parts are checked. Runs until `stop()`. */
  let stopped = false;
  const watch = async () => {
    while (!stopped) {
      const t0 = Date.now(), before = held.get(top)?.etag;
      await check({ extra: before ? `${top.includes("?") ? "&" : "?"}wait=${wait}` : "" });
      if (!stopped && Date.now() - t0 < 1000) await new Promise((r) => setTimeout(r, 5000)); // answered at once (no wait support, or down): pace
    }
  };
  return { check, watch, stop: () => { stopped = true; }, folds: () => [...held.values()] };
}

/** The kinds of source Basis knows (Bobby 11-02: "think like adding a podcast or rss feed"). Only a fold document is read today; a
 * feed (RSS, Atom, a podcast's) is named so the box can say it is not read yet, never fetched as if it were a fold. */
export const KINDS = { fold: "a fold document", feed: "a feed (RSS, Atom or a podcast)", query: "a stored query" };
/** The kind of source a typed address is: a feed by its scheme or its look (`feed:`, `podcast:`, `.rss`, `.atom`, `.xml`, a
 * `/feed` or `/rss` path); a stored query by its NAME (a bare word like `needs-you`: no slash, dot or colon, so it can never be a path or
 * a URL); else a fold document. */
export const kindOf = (text) =>
  /^(feed|podcast|itpc|pcast):/i.test(text) || /(\.(rss|atom|xml)|\/(feed|rss|atom))\/?(\?.*)?$/i.test(text) ? "feed"
    : QUERY_NAME.test(text) ? "query" : "fold";
/** A stored query's name (facto Query vertices, programs/fleet-map/nodes/*.cypher `// query: <name> v<n>`). */
export const QUERY_NAME = /^[a-z][a-z0-9-]*$/;
/** Where a stored query is read: the fold its runner writes (`// pick: true` queries, run-queries.ts), a manifest of one part. The name is
 * checked against QUERY_NAME, so no path is ever built from free text beyond a name. */
export const queryPath = (name) => `/fold-data/fleet/q.${name}.jsonld`;

/** The document this viewer reads: the fold-data meta's (`{path, source}`), or one named in the address (`?data=<path>`, as Basis's
 * add-a-source box submits it) by an absolute path on this origin. Anything else is refused, never fetched: `{path, error}`. */
export function dataOf(search, base, meta) {
  const named = new URLSearchParams(search).get("data")?.trim();
  if (named == null || named === "") return { path: meta.path, url: new URL(meta.path, base).href, source: meta.source };
  const kind = kindOf(named);
  // a stored query by name: its fold is the manifest the runner writes beside the fleet's folds; one not registered is a missing fold, said on Basis
  if (kind === "query") { const path = queryPath(named); return { path, url: new URL(path, base).href, source: null, kind, query: named }; }
  if (kind !== "fold") return { path: named, kind, error: `${KINDS[kind]} is not read yet; only a fold document is` };
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

// Basis, the level above every document (Bobby 11-02: "top level is basis and lists the folds"): its own fold, at #basis, listing
// the sources kept, with the add-a-source box as its first row. A document's root is named by the document, not "Everything".
export const BASIS = "#basis";
export const isBasis = (hash) => hash === BASIS;
/** The core's name for a root that names nothing of its own (src/ax.rs ROOT), which the fleet's producer writes as its root's title. */
export const GENERIC = "Everything";
const decoded = (s) => { try { return decodeURIComponent(s); } catch { return s; } };

/** A document's name: its root's own title, unless that is the generic one; else its folder ("/fold-data/fleet/page.json" is
 * "Fleet"), or, for a file not named page.json or index.json, the file's stem. */
export function nameOf(path, title) {
  const t = String(title ?? "").trim();
  if (t && t !== GENERIC) return t;
  const parts = new URL(path, "https://basis.invalid/").pathname.split("/").filter(Boolean).map(decoded);
  const file = parts.at(-1) ?? "", stem = /^(page|index)\.json$/i.test(file) ? parts.at(-2) : file.replace(/\.[^.]*$/, "");
  const w = (stem ?? "").replace(/[-_]+/g, " ").trim();
  return w ? w[0].toUpperCase() + w.slice(1) : t || GENERIC;
}

/** A document's folds with its root named `name`: the root's title, and the root wherever another fold says it (the first step of
 * its trail, a slide's previous or next). The folds given are not changed. */
export function named(nodes, name) {
  const root = nodes.get("root");
  if (!root || !name) return nodes;
  const was = root.title, step = (s) => (s?.id === "root" ? { ...s, title: name } : s);
  return new Map([...nodes].map(([id, n]) => {
    const m = { ...n, ...(id === "root" ? { title: name } : {}) };
    if (Array.isArray(n.trail) && n.trail.length && n.trail[0] === was) m.trail = [name, ...n.trail.slice(1)];
    for (const k of ["prev", "next"]) if (n[k]) m[k] = step(n[k]);
    return [id, m];
  }));
}

/** A fold's tree with Basis above it: its path starts at Basis (the root's path is just that), and a link to the root that the core
 * could only name generically (a missing fold's) says the document's name. On the root itself (`path` empty), the core's way back to
 * the root (a failed or empty root's) would go nowhere: Basis is the way out, so it goes. */
export function framed(tree, name, path = null) {
  const toRoot = (h) => h === "#/" || h === "#slides/";
  const rename = (n) => {
    let m = n;
    if (name && n.role === "link" && toRoot(n.href)) {
      if (n.name === GENERIC) m = { ...n, name };
      else if (n.name === `Back to ${GENERIC}`) m = { ...n, name: `Back to ${name}` };
    }
    return m.children ? { ...m, children: m.children.map(rename) } : m;
  };
  const t = rename(tree), kids = [...(t.children ?? [])];
  const crumb = { role: "listitem", children: [{ role: "link", name: "Basis", href: BASIS }] };
  if (kids[0]?.role === "navigation") {
    const list = kids[0].children[0], rest = list.children.filter((li) => !(path?.length === 0 && toRoot(li.children?.[0]?.href)));
    kids[0] = { ...kids[0], children: [{ ...list, children: [crumb, ...rest] }, ...kids[0].children.slice(1)] };
  } else kids.unshift({ role: "navigation", name: "Path", children: [{ role: "list", name: "Path", children: [crumb] }] });
  return { ...t, children: kids };
}

/** The sources kept, with `s` (`{kind, path, name}`, one just read) in it: once, by its path, where it was, with its latest name. */
export function keep(list, s) {
  const ok = (Array.isArray(list) ? list : []).filter((x) => x && typeof x.path === "string" && typeof x.name === "string");
  return ok.some((x) => x.path === s.path) ? ok.map((x) => (x.path === s.path ? s : x)) : [...ok, s];
}

/** Where a source opens, in this tab: the site's own document at its root, any other named in the address. */
export const sourceHref = (s, home) => (s.path === home ? "./#/" : `./?data=${encodeURIComponent(s.path)}#/`);

/** What Basis's box says under it: why the last address was not read, or what it takes. */
export const BOX_NOTE = "A fold document's path on this site, from /. Feeds (RSS, podcasts) are not read yet.";

/** Basis as a fold: `{node, tree}`. The site's own document (`home`, its path) first, then every other source kept, each entered
 * at its own address in this tab; the box first of all, holding `box.value` (an address not read) and saying `box.error`. */
export function basisOf(ax, kept, home, homeName, box = {}, localTimeZone) {
  const sources = [{ kind: "fold", path: home, name: homeName }, ...(kept ?? []).filter((s) => s.path !== home)];
  const node = { title: "Basis", trail: [], kids: sources.map((s, i) => ({ id: String(i), title: s.name, hint: s.path, href: sourceHref(s, home) })) };
  const local = (n) => ({ ...n, ...(n.role === "link" ? { local: true } : {}), ...(n.children ? { children: n.children.map(local) } : {}) });
  const tree = local(ax({ node, path: [], mode: "folds", href: "viewer", localTimeZone }));
  const row = { role: "listitem", cls: "prop source", children: [
    { role: "form", name: "Add a source", search: true, method: "get", action: "./", children: [
      { role: "searchbox", name: "Add a source", field: "data", ...(box.value ? { value: box.value } : {}), describedby: "source-note" },
      { role: "button", name: "Open" },
    ] },
    { role: "text", id: "source-note", cls: box.error ? "note alert" : "note", name: box.error ? `Not read: ${box.error}` : BOX_NOTE },
  ] };
  const list = tree.children.findIndex((c) => c.role === "list");
  tree.children[list] = { ...tree.children[list], children: [row, ...tree.children[list].children] };
  return { node, sources, tree };
}
