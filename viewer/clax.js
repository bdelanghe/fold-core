// Clax in Basis (basis-clax goal): a comment on any fold reaches the fold's owner through a Clax queue. Basis embeds Clax through
// @empathic/clax-sdk only: the overlay is the instance's own `/_clax/overlay.js` (the classic-script build of the SDK's ./overlay
// entry, global `Clax`), which fold serve passes through from the instance beside it (src/report.rs) so that it is same-origin and
// can be pinned by its SRI digest. It is never copied into this repo (break-glass fc88923c). This file is Basis's half: the pin, the
// mount, the reporter token from fold serve, and which thing a report is about.
//
// Clax main d7812795's overlay has no per-report target hook (its own picker chooses the element, range or area), so a report says
// which thing it is about through what the overlay does take (ANALYSIS-identity-in-fold-core-and-clax-loci-2026-10-31 §3-4):
// - `externalId`: the fold's haecceity. Reports on one fold fold into one Clax artifact, keyed on the thing, not on the URL.
// - `metadata`: `{about, owner, rows}`, the fold's haecceity and each row's (by its index, the `data-i` an element anchor's selector
//   lands on), with the rows that have none listed as `unnamed`, never guessed.
// The overlay is reopened on each move to another fold, so both always say the fold in view.

/** The thing a node shows: the haecceity of the row it is in (its schema:about), or null when it is in no row or its row has none. */
export function aboutOf(node) {
  const row = node?.closest?.('[typeof~="schema:WebPageElement"]');
  const about = row && [...row.children].find((c) => c.getAttribute("property") === "schema:about");
  return about?.getAttribute("resource") || null;
}

/** The rows on the page whose thing has no name: each marked `data-clax-flag="no-haecceity"` for the overlay (and the inspector) to
 * show, and returned. A mark only: no role, name or text changes, so the accessibility tree is the one the core gave. */
export function flagUnnamed(root) {
  const rows = [...(root?.querySelectorAll?.("ol.rows > li.obj") ?? [])].filter((r) => !aboutOf(r));
  for (const r of rows) r.dataset.claxFlag = "no-haecceity";
  return rows;
}

/** What the overlay is told about the fold in view: `{externalId, metadata}`. `fold` is the fold's haecceity (null when the
 * document has no source), `rows` the page's child rows. */
export function contextOf(fold, rows, owner) {
  const named = {}, unnamed = [];
  for (const r of rows) {
    const i = r?.dataset?.i, about = aboutOf(r);
    if (i == null) continue;
    if (about) named[i] = about; else unnamed.push(Number(i));
  }
  return { ...(fold ? { externalId: fold } : {}), metadata: { about: fold ?? null, owner, rows: named, ...(unnamed.length ? { unnamed } : {}) } };
}

/** fold serve's report-token route (src/serve.rs): with no body, `{instance, queue, owner}`; with a public key, `{token, expires_at}`.
 * Null when this server does not report (404), will not answer this reader (403), or did not mint (502). Same-origin only. */
export async function reportToken(base, publicKey = null, { fetch = globalThis.fetch } = {}) {
  try {
    const init = { method: "POST", credentials: "same-origin", cache: "no-store" };
    if (publicKey != null) Object.assign(init, { headers: { "Content-Type": "application/json" }, body: JSON.stringify({ public_key: publicKey }) });
    const r = await fetch(new URL("report-token", base).href, init);
    return r.ok ? await r.json() : null;
  } catch {
    return null;
  }
}

/** The overlay as the clax-sdk meta pins it: a path on this origin (fold serve's pass-through of the instance's overlay), the SDK
 * version and the SRI digest of the instance's bytes. None missing: nothing loads unpinned. `{src, integrity}`, or `{error}`. */
export function pinOf(meta, base) {
  const path = meta?.content?.trim(), version = meta?.dataset?.version?.trim(), integrity = meta?.dataset?.integrity?.trim();
  if (!path) return { error: "no clax-sdk overlay is pinned yet" };
  if (!/^\d+\.\d+\.\d+$/.test(version ?? "")) return { error: `the overlay ${path} names no version` };
  if (!/^sha(256|384|512)-[A-Za-z0-9+/]+=*$/.test(integrity ?? "")) return { error: `the overlay ${path} has no SRI digest` };
  const src = new URL(path, base);
  if (src.origin !== new URL(base).origin || /^[a-z][a-z0-9+.-]*:|^\/\//i.test(path)) return { error: `the overlay ${path} is not a path on this origin` };
  src.searchParams.set("v", version);
  return { src: src.href, integrity };
}

/** The overlay script, loaded with its SRI digest: a classic script that defines the global `Clax`. */
function loadPinned(doc, { src, integrity }) {
  return new Promise((resolve, reject) => {
    const s = doc.createElement("script");
    s.src = src; s.integrity = integrity; s.crossOrigin = "anonymous";
    s.addEventListener("load", () => (globalThis.Clax ? resolve(globalThis.Clax) : reject(new Error("the overlay loaded but defined no global Clax"))));
    s.addEventListener("error", () => reject(new Error(`the overlay ${src} did not load (or its digest did not match)`)));
    doc.head.append(s);
  });
}

/** Load the pinned overlay and mount it for the fold in view (`at()` gives `{fold, rows}`): `Clax.overlay({instance, queue, auth,
 * externalId, metadata})`, `auth` asking fold serve for each reporter token. Resolves to `{mounted, why?, reopen()}`: call reopen()
 * after each move to another fold. Nothing is fetched or loaded until the pin holds. */
export async function embed({ doc = document, base = doc.baseURI, fetch = globalThis.fetch, load = loadPinned, at = () => ({ fold: null, rows: [] }) } = {}) {
  const off = (why) => ({ mounted: false, why, reopen() {} });
  const pin = pinOf(doc.querySelector('meta[name="clax-sdk"]'), base);
  if (pin.error) return off(pin.error);
  const mount = await reportToken(base, null, { fetch });
  if (!mount?.instance || !mount?.queue) return off("fold serve does not report to Clax here");
  const Clax = await load(doc, pin);
  if (typeof Clax?.overlay !== "function") return off("the pinned overlay has no Clax.overlay");
  const auth = async (publicKey) => {
    const t = await reportToken(base, publicKey, { fetch });
    if (!t?.token || !t?.expires_at) throw new Error("fold serve did not mint a reporter token");
    return { token: t.token, expires_at: t.expires_at };
  };
  let handle = null;
  const reopen = () => {
    handle?.close();
    const { fold, rows } = at();
    flagUnnamed(doc);
    handle = Clax.overlay({ instance: mount.instance, queue: mount.queue, auth, ...contextOf(fold, rows ?? [], mount.owner) }, doc);
  };
  reopen();
  return { mounted: true, reopen };
}
