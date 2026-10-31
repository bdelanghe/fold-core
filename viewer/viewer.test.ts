// The viewer's own path on every golden, with the built module (FOLD_CORE_WASM): the document fetched from a data URL and folded here
// gives the golden folds; every location's tree is the golden one (the "viewer" links, read back by pathOf); every fold it can show
// derives a DOM without a refusal (fold-dom.js toDom: the AX gate); and the artificial viewer's walk over what it shows is the golden
// walk (the load gate: view.words and the rest, blocking where the golden blocks).
import { expect, test } from "bun:test";
import { readdirSync, readFileSync } from "node:fs";
import { join } from "node:path";
import { core } from "../js/fold-core.ts";
import { toDom } from "./fold-dom.js";
import { at, dataOf, haecceityOf, href, open, pathOf, segmentOf, since, viewOf } from "./view.js";
import { aboutOf, embed, flagUnnamed, pinOf, reporterToken, targetOf } from "./clax.js";

const goldens = join(import.meta.dir, "..", "goldens");
const wasm = process.env.FOLD_CORE_WASM ?? join(import.meta.dir, "..", "target", "wasm32-unknown-unknown", "release", "fold_core.wasm");
const { fold, walk, ax } = await core(readFileSync(wasm));
const read = (...p: string[]) => JSON.parse(readFileSync(join(...p), "utf8"));
const cases = readdirSync(goldens).filter((f) => f.endsWith(".input.json")).map((f) => f.replace(/\.input\.json$/, "")).sort();

/** The data origin: the case's root at /fold-data/<case>/page.json and its files beside it; anything else is a 404. */
function origin(name: string, input: { root: unknown; files?: Record<string, string> }) {
  const base = `https://data.test/fold-data/${name}/`;
  const served = new Map<string, string>([[base + "page.json", JSON.stringify(input.root)]]);
  for (const [p, t] of Object.entries(input.files ?? {})) served.set(new URL(p, base).href, t);
  const fetch = async (url: string) => {
    const t = served.get(url);
    return t == null ? { ok: false, status: 404, statusText: "Not Found" } : { ok: true, headers: new Headers(), json: async () => JSON.parse(t), text: async () => t };
  };
  return { url: base + "page.json", fetch: fetch as unknown as typeof globalThis.fetch };
}

/** Just enough of a document for toDom: elements that keep their tag, attributes, children and text. */
type El = { tag: string; attrs: Record<string, string>; kids: El[]; textContent: string; dataset: Record<string, string>; [k: string]: unknown };
const doc = {
  createElement: (tag: string): El => {
    const e: El = { tag, attrs: {}, kids: [], textContent: "", dataset: {} } as El;
    e.setAttribute = (k: string, v: string) => { e.attrs[k] = String(v); };
    e.append = (c: El) => { e.kids.push(c); };
    return e;
  },
};
const all = (e: El): El[] => [e, ...e.kids.flatMap(all)];

for (const name of cases) {
  const input = read(goldens, `${name}.input.json`);
  const want = read(goldens, `${name}.folds.json`);
  test(`${name}: fetched from its data URL and folded in the viewer, the golden folds`, async () => {
    const { url, fetch } = origin(name, input);
    const nodes = await open(fold, url, { fetch, opts: () => input.opts });
    expect(JSON.stringify([...nodes])).toBe(JSON.stringify(want));
  });
  test(`${name}: every fold it shows derives its DOM, as folds and as slides where it presents`, async () => {
    const { url, fetch } = origin(name, input);
    const nodes = await open(fold, url, { fetch, opts: () => input.opts });
    for (const [id, n] of nodes) for (const mode of n.present ? ["folds", "slides"] : ["folds"]) {
      const v = viewOf(ax, nodes, href(at(id), mode), input.opts.localTimeZone);
      expect([v.node, v.mode]).toEqual([n, mode]);
      const main = toDom(doc, v.tree) as El, els = all(main);
      expect(els.filter((e) => e.tag === "h1").map((e) => e.textContent)).toEqual([n.title]);
      expect(els.filter((e) => e.tag === "li" && e.className === "obj").length).toBe((n.kids ?? []).length);
    }
  });
  test(`${name}: the walk over what the viewer shows is the golden walk`, async () => {
    const { url, fetch } = origin(name, input);
    const nodes = await open(fold, url, { fetch, opts: () => input.opts });
    const w = read(goldens, "walk", `fold-${name}.input.json`);
    expect(JSON.stringify([...nodes])).toBe(JSON.stringify(w.nodes));
    expect(JSON.stringify(walk({ nodes: [...nodes], opts: w.opts }))).toBe(JSON.stringify(read(goldens, "walk", `fold-${name}.walk.json`)));
  });
}

for (const f of readdirSync(join(goldens, "ax")).sort()) {
  const name = f.replace(/\.ax\.json$/, "");
  test(`ax ${name}: each location the viewer links to shows the golden tree`, () => {
    const input = read(goldens, "walk", `${name}.input.json`), nodes = new Map(input.nodes as [string, unknown][]);
    const rows = read(goldens, "ax", f).filter(([, , style]: string[]) => style === "viewer");
    expect(rows.length).toBeGreaterThan(0);
    for (const [id, mode, , tree] of rows) {
      const hash = href(at(id), mode);
      expect(pathOf(hash)).toEqual(at(id));
      expect(JSON.stringify(viewOf(ax, nodes, hash, input.opts.localTimeZone).tree)).toBe(JSON.stringify(tree));
    }
  });
}

test("a location with no fold, and a document that cannot be read", async () => {
  const nodes = new Map([["root", { title: "T", trail: [], kids: [] }]]);
  // A missing fold still has its way back (BRIEF-basis-forms-push-signing-2026-10-31 §5): the path to the root, why, and the root.
  const gone = viewOf(ax, nodes, "#/gone", "UTC").tree.children;
  expect(gone.map((c: any) => c.role)).toEqual(["navigation", "heading", "list"]);
  expect(gone[0].children[0].children[0].children[0]).toEqual({ role: "link", name: "Everything", href: "#/" });
  expect(gone[1]).toEqual({ role: "heading", name: "Nothing here", focus: true });
  expect(gone[2].children.map((r: any) => r.children[0].name)).toEqual(["You asked for gone, and no fold has that path.", "Back to Everything"]);
  expect(viewOf(ax, nodes, "", "UTC").mode).toBe("folds");
  await expect(open(fold, "https://data.test/fold-data/none/page.json", { fetch: origin("x", { root: {} }).fetch })).rejects.toThrow("404");
});

test("the document read: the meta's, or one named in the address by an absolute path on this origin, nothing else", () => {
  const meta = { path: "/fold-data/fleet/page.json", source: "fleet-map" }, base = "https://phobos.test/fold/?x#/needs";
  expect(dataOf("", base, meta)).toEqual({ path: meta.path, url: "https://phobos.test/fold-data/fleet/page.json", source: "fleet-map" });
  // the Reading box submitted empty, or blank, reads the meta's
  for (const q of ["?data=", "?data=+"]) expect(dataOf(q, base, meta).url).toBe("https://phobos.test/fold-data/fleet/page.json");
  expect(dataOf("?data=/fold-data/demo/page.json", base, meta)).toEqual({ path: "/fold-data/demo/page.json", url: "https://phobos.test/fold-data/demo/page.json", source: null });
  for (const away of ["https://evil.test/page.json", "//evil.test/page.json", "/\\evil.test/page.json", "http://phobos.test/fold-data/fleet/page.json", "../fold-data/demo/page.json", "page.json"])
    expect(dataOf(`?data=${encodeURIComponent(away)}`, base, meta)).toEqual({ path: away, error: "only a path on this site, from /, is read" });
});

// The haecceity a row carries in the viewer is the one its pre-rendered page gives it (src/id.rs, src/page.rs): a Clax report made
// on either anchors to the same thing.
test("a source id is one segment, as src/id.rs segment_of writes it (tests/ids.rs's vectors)", () => {
  for (const [id, want] of [["needs", "needs"], ["2fac998588cf", "2fac998588cf"], ["pr/12", "pr%2F12"], ["Needs you", "Needs%20you"],
    ["Café — Ünïcode", "Caf%C3%A9%20%E2%80%94%20%C3%9Cn%C3%AFcode"], ["日本語", "%E6%97%A5%E6%9C%AC%E8%AA%9E"],
    ["a#3 (draft) [x]", "a%233%20%28draft%29%20%5Bx%5D"], ["~tilde", "%7Etilde"], ["50%", "50%25"], ["UPPER-lower_9.v2", "UPPER-lower_9.v2"]])
    expect(segmentOf(id)).toBe(want);
  expect(haecceityOf("fleet", [])).toBe("https://ipso.bounded.tools/fleet");
  expect(haecceityOf("Fleet Map", ["prs", "bdelanghe/fold-core#12"])).toBe("https://ipso.bounded.tools/Fleet%20Map/prs/bdelanghe%2Ffold-core%2312");
});

for (const f of readdirSync(join(goldens, "pages")).filter((f) => f.endsWith(".pages.json")).sort()) {
  const name = f.replace(/\.pages\.json$/, "");
  test(`pages ${name}: each row the viewer shows carries the haecceity its pre-rendered page gives it`, () => {
    const input = read(goldens, "walk", `${name}.input.json`), nodes = new Map(input.nodes as [string, any][]);
    const abouts = (t: any): string[] => [...(t?.haecceity ? [t.haecceity] : []), ...(t?.children ?? []).flatMap(abouts)];
    for (const p of read(goldens, "pages", f).filter((p: any) => p.mode === "folds" && p.html)) {
      const want = [...p.html.matchAll(/property="schema:about" resource="([^"]*)"/g)].map((m: string[]) => m[1]);
      const tree = viewOf(ax, nodes, href(at(p.id), "folds"), input.opts.localTimeZone, name).tree;
      expect(abouts(tree)).toEqual(want);
      // and without a source (a document named in the address) no row claims one
      expect(abouts(viewOf(ax, nodes, href(at(p.id), "folds"), input.opts.localTimeZone).tree)).toEqual([]);
    }
  });
}

/** A row as the DOM has it, enough for clax.js: closest() finds itself (as Element.closest does), its schema:about span if any. */
function fakeRow(i: number, about: string | null, id = "") {
  const span = { getAttribute: (k: string) => (k === "property" ? "schema:about" : k === "resource" ? about : null) };
  const r: any = { id, dataset: { i: String(i) }, children: about ? [span] : [] };
  r.closest = (sel: string) => (sel.includes("WebPageElement") ? (about ? r : null) : r);
  return r;
}

test("a report is about the row's haecceity; a row with none is flagged, never guessed", () => {
  const named = fakeRow(0, "https://ipso.bounded.tools/fleet/needs"), bare = fakeRow(1, null);
  expect(aboutOf(named)).toBe("https://ipso.bounded.tools/fleet/needs");
  expect(aboutOf(bare)).toBeNull();
  expect(aboutOf(null)).toBeNull();
  const locus = "https://phobos.test/fold/#/needs";
  expect(targetOf(named, { locus, quote: "Needs you" })).toEqual({
    about: "https://ipso.bounded.tools/fleet/needs", locus,
    situs: [{ type: "CssSelector", value: 'ol.rows > li.obj[data-i="0"]' }, { type: "TextQuoteSelector", exact: "Needs you" }],
  });
  expect(targetOf(fakeRow(2, "https://x.test/a", "a"), { locus, fixity: "sha-256:00" }).situs[0]).toEqual({ type: "FragmentSelector", value: "a" });
  expect(targetOf(bare, { locus })).toEqual({ flag: "no-haecceity", locus, situs: [{ type: "CssSelector", value: 'ol.rows > li.obj[data-i="1"]' }] });
  const flagged = flagUnnamed({ querySelectorAll: () => [named, bare] });
  expect(flagged).toEqual([bare]);
  expect([named.dataset.claxFlag, bare.dataset.claxFlag]).toEqual([undefined, "no-haecceity"]);
});

test("the overlay loads only pinned by version and SRI, and only where fold serve mints a token", async () => {
  const meta = (content: string, integrity: string) => ({ content, dataset: { integrity } });
  expect(pinOf(null).error).toBe("no clax-sdk overlay is pinned yet");
  expect(pinOf(meta("", "")).error).toBe("no clax-sdk overlay is pinned yet");
  expect(pinOf(meta("https://cdn.test/clax-sdk/overlay.js", "sha384-AAAA")).error).toContain("names no version");
  expect(pinOf(meta("vendor/clax-sdk@0.3.1/overlay.js", "")).error).toContain("no SRI digest");
  expect(pinOf(meta("vendor/clax-sdk@0.3.1/overlay.js", "md5-xyz")).error).toContain("no SRI digest");
  expect(pinOf(meta("vendor/clax-sdk@0.3.1/overlay.js", "sha384-oqVuAfXRKap7fdgcCY5uykM6+R9GqQ8K/uxy9rx7HNQlGYl1kPzQho1wx4JwY8wC")))
    .toEqual({ src: "vendor/clax-sdk@0.3.1/overlay.js", integrity: "sha384-oqVuAfXRKap7fdgcCY5uykM6+R9GqQ8K/uxy9rx7HNQlGYl1kPzQho1wx4JwY8wC" });

  const grant = { token: "r1.1.x", exp: 1, clax: "http://localhost:7480", queue: "basis", owner: "personal-allie-lead" };
  const calls: string[] = [];
  const fetchWith = (status: number) => (async (url: string, init?: RequestInit) => {
    calls.push(`${init?.method ?? "GET"} ${url}`);
    if (url.endsWith("/report-token")) return { ok: status === 200, status, json: async () => grant };
    return { ok: true, status: 200, json: async () => ({ light: {} }) };
  }) as unknown as typeof globalThis.fetch;
  const row = fakeRow(0, "https://ipso.bounded.tools/fleet/needs");
  const doc = (pinned: boolean) => ({
    baseURI: "https://phobos.test/fold/", location: { href: "https://phobos.test/fold/#/" },
    querySelector: () => (pinned ? meta("vendor/clax-sdk@0.3.1/overlay.js", "sha384-AAAA") : meta("", "")),
    querySelectorAll: () => [row],
  });
  const fail = async () => { throw new Error("not to be loaded"); };

  // unpinned: nothing is fetched, nothing loaded
  expect(await embed({ doc: doc(false) as any, fetch: fetchWith(200), load: fail })).toEqual({ mounted: false, why: "no clax-sdk overlay is pinned yet" });
  expect(calls).toEqual([]);
  // pinned, but this server does not report (or not for this reader): the overlay is not loaded
  expect(await embed({ doc: doc(true) as any, fetch: fetchWith(404), load: fail })).toEqual({ mounted: false, why: "fold serve does not report to Clax here" });
  expect(calls).toEqual(["POST https://phobos.test/fold/report-token"]);
  expect(await reporterToken("https://phobos.test/fold/", { fetch: (async () => { throw new Error("offline"); }) as any })).toBeNull();
  // pinned and minted: mounted with the token, the queue and the row's haecceity as its target
  let opts: any = null;
  const r = await embed({ doc: doc(true) as any, fetch: fetchWith(200), load: async () => ({ mountOverlay: (o: any) => { opts = o; } }) });
  expect(r).toEqual({ mounted: true, unnamed: 0 });
  expect([opts.endpoint, opts.queue, opts.owner, opts.token]).toEqual([grant.clax, grant.queue, grant.owner, grant.token]);
  expect(opts.target(row).about).toBe("https://ipso.bounded.tools/fleet/needs");
  expect(opts.tokens).toEqual({ light: {} });
  expect(await opts.refreshToken()).toBe(grant.token);
  // a pinned build without the entry this side expects says so, and mounts nothing
  expect(await embed({ doc: doc(true) as any, fetch: fetchWith(200), load: async () => ({}) })).toEqual({ mounted: false, why: "the pinned SDK has no mountOverlay" });
});

// A kept copy (sw.js's Fold-Stored): open says when it was kept, and since says it in the reader's zone with its age in words.
test("a kept copy is said as not updated since its time, and how long ago", async () => {
  const kept = "2026-10-31T14:05:00.000Z", body = JSON.stringify({ title: "Kept", kids: [] });
  let at: string | null | undefined;
  await open(fold, "https://data.test/page.json", { fetch: async () => new Response(body, { headers: { "fold-stored": kept } }), stored: (a: string | null) => { at = a; } });
  expect(at).toBe(kept);
  await open(fold, "https://data.test/page.json", { fetch: async () => new Response(body), stored: (a: string | null) => { at = a; } });
  expect(at).toBeNull();
  const now = new Date("2026-10-31T17:00:00Z");
  expect(since(kept, now, "UTC")).toBe("Not updated since 14:05, 2 hours ago.");
  expect(since("2026-10-31T16:59:30Z", now, "UTC")).toBe("Not updated since 16:59, less than a minute ago.");
  expect(since("2026-10-31T16:20:00Z", now, "UTC")).toBe("Not updated since 16:20, 40 minutes ago.");
  expect(since("2026-10-27T09:00:00Z", now, "UTC")).toBe("Not updated since Tue 27 Oct at 09:00, 4 days ago.");
});
