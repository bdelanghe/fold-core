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

/** The kinds of source Basis knows (Bobby 11-02: "think like adding a podcast or rss feed"). Only a fold document is read today; a
 * feed (RSS, Atom, a podcast's) is named so the box can say it is not read yet, never fetched as if it were a fold. */
export const KINDS = { fold: "a fold document", feed: "a feed (RSS, Atom or a podcast)" };
/** The kind of source a typed address is: a feed by its scheme or its look (`feed:`, `podcast:`, `.rss`, `.atom`, `.xml`, a
 * `/feed` or `/rss` path), else a fold document. */
export const kindOf = (text) =>
  /^(feed|podcast|itpc|pcast):/i.test(text) || /(\.(rss|atom|xml)|\/(feed|rss|atom))\/?(\?.*)?$/i.test(text) ? "feed" : "fold";

/** The document this viewer reads: the fold-data meta's (`{path, source}`), or one named in the address (`?data=<path>`, as Basis's
 * add-a-source box submits it) by an absolute path on this origin. Anything else is refused, never fetched: `{path, error}`. */
export function dataOf(search, base, meta) {
  const named = new URLSearchParams(search).get("data")?.trim();
  if (named == null || named === "") return { path: meta.path, url: new URL(meta.path, base).href, source: meta.source };
  const kind = kindOf(named);
  if (kind !== "fold") return { path: named, kind, error: `${KINDS[kind]} is not read yet; only a fold document is` };
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
