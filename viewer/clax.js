// Clax in Basis (basis-clax goal): a comment on any fold reaches the fold's owner through a Clax queue. Basis embeds Clax through
// `@empathic/clax-sdk` only, loaded by the clax-sdk meta in index.html (a pinned URL with its SRI digest), never from Clax's own
// shell or bridge. This file is Basis's half: which thing a report is about, the reporter token fold serve mints, the theme, and
// the rows no report can name. The overlay is the one script the page needs for this. (With JS off, a page's "Comment in Clax"
// link is close condition 4, not yet written: it waits on the queue's URL in Clax.)
//
// A report's target follows the one target rule (ANALYSIS-identity-in-fold-core-and-clax-loci-2026-10-31 §4):
// `{about, locus, situs, fixity}`. `about` is the row's haecceity (its RDFa schema:about, src/html.rs and fold-dom.js), the key a
// thread is filed under; `locus` is where it was seen (this page, at this fold); `situs` is the position there; `fixity` is what was
// seen, when the overlay gives it. A row with no haecceity still gets a report, flagged, with no `about` (§3: "the inspector should
// flag those rows").

/** The thing a node shows: the haecceity of the row it is in (its schema:about), or null when it is in no row or its row has none. */
export function aboutOf(node) {
  const row = node?.closest?.('[typeof~="schema:WebPageElement"]');
  const about = row && [...row.children].find((c) => c.getAttribute("property") === "schema:about");
  return about?.getAttribute("resource") || null;
}

/** Where a node sits in the fold: the row's index among the fold's rows (data-i) and its element id where it has one. */
function situsOf(node) {
  const row = node?.closest?.("li.obj, li.prop");
  const s = [];
  if (row?.id) s.push({ type: "FragmentSelector", value: row.id });
  if (row?.dataset?.i != null) s.push({ type: "CssSelector", value: `ol.rows > li.obj[data-i="${row.dataset.i}"]` });
  return s;
}

/** A report's target, by the one target rule: `about` when the node's row names its thing, else `flag: "no-haecceity"`. `locus` is
 * the page as seen (its URL, hash and all); `quote` is the text the reader picked, as a TextQuoteSelector. */
export function targetOf(node, { locus, quote = null, fixity = null } = {}) {
  const about = aboutOf(node);
  const situs = situsOf(node);
  if (quote) situs.push({ type: "TextQuoteSelector", exact: quote });
  return { ...(about ? { about } : { flag: "no-haecceity" }), locus, situs, ...(fixity ? { fixity } : {}) };
}

/** The rows on the page whose thing has no name: each marked `data-clax-flag="no-haecceity"` for the overlay (and the inspector) to
 * show, and returned. A mark only: no role, name or text changes, so the accessibility tree is the one the core gave. */
export function flagUnnamed(root) {
  const rows = [...(root?.querySelectorAll?.("ol.rows > li.obj") ?? [])].filter((r) => !aboutOf(r));
  for (const r of rows) r.dataset.claxFlag = "no-haecceity";
  return rows;
}

/** A reporter token from fold serve (POST report-token, src/report.rs): `{token, exp, clax, queue, owner}`, or null when this server
 * does not report (404) or will not mint for this reader (403). Same-origin only: the request carries no credentials elsewhere. */
export async function reporterToken(base, { fetch = globalThis.fetch } = {}) {
  try {
    const r = await fetch(new URL("report-token", base).href, { method: "POST", credentials: "same-origin", cache: "no-store" });
    return r.ok ? await r.json() : null;
  } catch {
    return null;
  }
}

/** The SDK's overlay script as the clax-sdk meta pins it: its path on Bobby's self-hosted Clax (the origin fold serve names with the
 * token), its version and its SRI digest. The SDK is never copied into this repo (break-glass fc88923c: self-hosting is allowed,
 * redistribution is not); it is loaded at run time from that Clax, and only these bytes run. None missing: nothing loads unpinned.
 * `{src, integrity}`, or `{error}` saying why nothing is loaded. */
export function pinOf(meta, origin) {
  const path = meta?.content?.trim(), version = meta?.dataset?.version?.trim(), integrity = meta?.dataset?.integrity?.trim();
  if (!path) return { error: "no clax-sdk overlay is pinned yet" };
  if (!/^\d+\.\d+\.\d+$/.test(version ?? "")) return { error: `the overlay ${path} names no version` };
  if (!/^sha(256|384|512)-[A-Za-z0-9+/]+=*$/.test(integrity ?? "")) return { error: `the overlay ${path} has no SRI digest` };
  if (!/^\//.test(path) || /^\/\//.test(path)) return { error: `the overlay ${path} is not a path on the Clax origin` };
  const src = new URL(path, origin);
  src.searchParams.set("v", version);
  return { src: src.href, integrity };
}

/** Load the pinned overlay (one module script, with its SRI digest) and mount it with Basis's hooks: its target from the row's
 * haecceity, its token from fold serve, its look from theme.tokens.json. Resolves to what was done, for the caller to say. */
export async function embed({ doc = document, base = doc.baseURI, fetch = globalThis.fetch, load = importPinned } = {}) {
  const meta = doc.querySelector('meta[name="clax-sdk"]');
  if (pinOf(meta, base).error) return { mounted: false, why: pinOf(meta, base).error };
  const grant = await reporterToken(base, { fetch });
  if (!grant) return { mounted: false, why: "fold serve does not report to Clax here" };
  // The overlay comes from the Clax the reports go to (fold serve's config names it: localhost on the Mac, its tailnet URL elsewhere).
  const pin = pinOf(meta, grant.clax);
  const sdk = await load(doc, pin);
  // The SDK's overlay entry (@empathic/clax-sdk/overlay). ASSUMED, not read: `mountOverlay` and its options are this side's
  // guess, since the SDK's source was out of reach when this was written; the first pinned build settles them (viewer/.qual).
  // Its host token set is clax-sdk-kit-goal's close condition 4: until the overlay takes `tokens`, it keeps its own look.
  const tokens = await fetch(new URL("theme.tokens.json", base).href).then((r) => (r.ok ? r.json() : null)).catch(() => null);
  const overlay = sdk.mountOverlay ?? sdk.default?.mountOverlay;
  if (typeof overlay !== "function") return { mounted: false, why: "the pinned SDK has no mountOverlay" };
  overlay({
    endpoint: grant.clax,
    queue: grant.queue,
    owner: grant.owner,
    token: grant.token,
    refreshToken: async () => (await reporterToken(base, { fetch }))?.token ?? null,
    target: (node, seen = {}) => targetOf(node, { locus: doc.location?.href ?? base, ...seen }),
    tokens,
  });
  return { mounted: true, unnamed: flagUnnamed(doc).length };
}

/** The overlay module, loaded with its SRI digest: a module script element (import() takes no integrity), whose module puts its
 * exports where this can read them. */
function importPinned(doc, { src, integrity }) {
  return new Promise((resolve, reject) => {
    const s = doc.createElement("script");
    s.type = "module"; s.src = src; s.integrity = integrity; s.crossOrigin = "anonymous";
    s.addEventListener("load", () => (globalThis.ClaxSDK ? resolve(globalThis.ClaxSDK) : reject(new Error("the overlay loaded but exposed no ClaxSDK"))));
    s.addEventListener("error", () => reject(new Error(`the overlay ${src} did not load (or its digest did not match)`)));
    doc.head.append(s);
  });
}
