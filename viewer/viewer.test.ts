// The viewer's own path on every golden, with the built module (FOLD_CORE_WASM): the document fetched from a data URL and folded here
// gives the golden folds; every location's tree is the golden one (the "viewer" links, read back by pathOf); every fold it can show
// derives a DOM without a refusal (fold-dom.js toDom: the AX gate); and the artificial viewer's walk over what it shows is the golden
// walk (the load gate: view.words and the rest, blocking where the golden blocks).
import { expect, test } from "bun:test";
import { readdirSync, readFileSync } from "node:fs";
import { join } from "node:path";
import { core } from "../js/fold-core.ts";
import { toDom } from "./fold-dom.js";
import { at, basisOf, BOX_NOTE, dataOf, framed, haecceityOf, href, isBasis, keep, KINDS, kindOf, named, nameOf, open, pathOf, segmentOf, since, viewOf } from "./view.js";
import { aboutOf, contextOf, embed, flagUnnamed, pinOf, reportToken } from "./clax.js";

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
    // A repeated id (goldens/walk/repeats) is one fold to the viewer, the last (a Map's), as it is one file to a host, written last.
    const last = new Map(read(goldens, "pages", f).filter((p: any) => p.mode === "folds" && p.html).map((p: any) => [p.id, p]));
    for (const p of last.values()) {
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
  // what the overlay is told: the fold's haecceity as its artifact, each row's by its data-i, the unnamed ones listed
  expect(contextOf("https://ipso.bounded.tools/fleet", [named, bare], "personal-allie-lead")).toEqual({
    externalId: "https://ipso.bounded.tools/fleet",
    metadata: { about: "https://ipso.bounded.tools/fleet", owner: "personal-allie-lead", rows: { 0: "https://ipso.bounded.tools/fleet/needs" }, unnamed: [1] },
  });
  // a document with no source: no artifact claimed, no about
  expect(contextOf(null, [named], "o")).toEqual({ metadata: { about: null, owner: "o", rows: { 0: "https://ipso.bounded.tools/fleet/needs" } } });
  const flagged = flagUnnamed({ querySelectorAll: () => [named, bare] });
  expect(flagged).toEqual([bare]);
  expect([named.dataset.claxFlag, bare.dataset.claxFlag]).toEqual([undefined, "no-haecceity"]);
});

test("the overlay loads only pinned, same-origin, and mounts as Clax.overlay with fold serve's tokens", async () => {
  const meta = (content: string, version: string, integrity: string) => ({ content, dataset: { version, integrity } });
  const base = "https://phobos.test/fold/", sri = "sha384-oqVuAfXRKap7fdgcCY5uykM6+R9GqQ8K/uxy9rx7HNQlGYl1kPzQho1wx4JwY8wC";
  expect(pinOf(null, base).error).toBe("no clax-sdk overlay is pinned yet");
  expect(pinOf(meta("", "", ""), base).error).toBe("no clax-sdk overlay is pinned yet");
  expect(pinOf(meta("clax/overlay.js", "", sri), base).error).toContain("names no version");
  expect(pinOf(meta("clax/overlay.js", "latest", sri), base).error).toContain("names no version");
  expect(pinOf(meta("clax/overlay.js", "0.6.0", ""), base).error).toContain("no SRI digest");
  expect(pinOf(meta("clax/overlay.js", "0.6.0", "md5-xyz"), base).error).toContain("no SRI digest");
  // fold serve's pass-through only: never another host (the instance sends no CORS header, so a pinned load from it fails)
  for (const away of ["https://cdn.test/overlay.js", "//cdn.test/overlay.js", "http://localhost:7480/_clax/overlay.js"])
    expect(pinOf(meta(away, "0.6.0", sri), base).error).toContain("not a path on this origin");
  expect(pinOf(meta("clax/overlay.js", "0.6.0", sri), base)).toEqual({ src: "https://phobos.test/fold/clax/overlay.js?v=0.6.0", integrity: sri });
  // the pin index.html ships is one pinOf takes
  const shipped = readFileSync(join(import.meta.dir, "index.html"), "utf8").match(/<meta name="clax-sdk" content="([^"]*)" data-version="([^"]*)" data-integrity="([^"]*)">/);
  expect(shipped).not.toBeNull();
  expect(pinOf(meta(shipped![1], shipped![2], shipped![3]), base).src).toBe("https://phobos.test/fold/clax/overlay.js?v=0.6.0");

  const mount = { instance: "https://clax.tail.test", queue: "basis", owner: "personal-allie-lead" };
  const minted = { token: "cxr_x", expires_at: "2026-10-31T18:15:00Z" };
  const calls: string[] = [];
  const fetchWith = (status: number) => (async (url: string, init?: RequestInit) => {
    calls.push(`${init?.method ?? "GET"} ${url} ${init?.body ?? ""}`.trim());
    const keyed = typeof init?.body === "string";
    return { ok: status === 200, status, json: async () => (keyed ? minted : mount) };
  }) as unknown as typeof globalThis.fetch;
  const named = fakeRow(0, "https://ipso.bounded.tools/fleet/needs"), bare = fakeRow(1, null);
  const doc = (pinned: boolean) => ({
    baseURI: base,
    querySelector: () => (pinned ? meta("clax/overlay.js", "0.6.0", "sha384-AAAA") : meta("", "", "")),
    querySelectorAll: () => [named, bare],
  });
  const fail = async () => { throw new Error("not to be loaded"); };
  const at = () => ({ fold: "https://ipso.bounded.tools/fleet", rows: [named, bare] });

  // unpinned: nothing is fetched, nothing loaded
  expect((await embed({ doc: doc(false) as any, fetch: fetchWith(200), load: fail, at })).why).toBe("no clax-sdk overlay is pinned yet");
  expect(calls).toEqual([]);
  // pinned, but this server does not report (or not for this reader): the overlay is not loaded
  expect((await embed({ doc: doc(true) as any, fetch: fetchWith(404), load: fail, at })).why).toBe("fold serve does not report to Clax here");
  expect(calls).toEqual(["POST https://phobos.test/fold/report-token"]);
  expect(await reportToken(base, null, { fetch: (async () => { throw new Error("offline"); }) as any })).toBeNull();
  // pinned and reporting: Clax.overlay with the instance, the queue, an auth that asks fold serve, and the fold in view
  const opened: any[] = [], closed: number[] = [];
  let loaded: any = null;
  const Clax = { overlay: (o: any, d: unknown) => { opened.push({ ...o, d }); const n = opened.length; return { close: () => closed.push(n) }; } };
  const r = await embed({ doc: doc(true) as any, fetch: fetchWith(200), load: async (_d: unknown, pin: any) => { loaded = pin; return Clax; }, at });
  expect(r.mounted).toBe(true);
  expect(loaded).toEqual({ src: "https://phobos.test/fold/clax/overlay.js?v=0.6.0", integrity: "sha384-AAAA" });
  const o = opened[0];
  expect([o.instance, o.queue, o.externalId]).toEqual([mount.instance, mount.queue, "https://ipso.bounded.tools/fleet"]);
  expect(o.metadata).toEqual({ about: "https://ipso.bounded.tools/fleet", owner: mount.owner, rows: { 0: "https://ipso.bounded.tools/fleet/needs" }, unnamed: [1] });
  expect(bare.dataset.claxFlag).toBe("no-haecceity");
  calls.length = 0;
  expect(await o.auth("PUBKEY")).toEqual(minted);
  expect(calls).toEqual(['POST https://phobos.test/fold/report-token {"public_key":"PUBKEY"}']);
  // a move to another fold: the old overlay closed, a new one for the fold now in view
  r.reopen();
  expect([opened.length, closed]).toEqual([2, [1]]);
  // fold serve not minting: auth throws, so the overlay says it, never a made-up token
  const r2 = await embed({ doc: doc(true) as any, fetch: (async (u: string, i?: RequestInit) => (typeof i?.body === "string" ? { ok: false, status: 502 } : { ok: true, status: 200, json: async () => mount })) as any, load: async () => Clax, at });
  await expect(opened[opened.length - 1].auth("PUBKEY")).rejects.toThrow("did not mint");
  expect(r2.mounted).toBe(true);
  // a pinned build without Clax.overlay says so, and mounts nothing
  expect((await embed({ doc: doc(true) as any, fetch: fetchWith(200), load: async () => ({}), at })).why).toBe("the pinned overlay has no Clax.overlay");
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

// Basis (Bobby 11-02): a document's root is named by the document, Basis is the level above it, and the Reading box is Basis's
// add-a-source box, search-like, with a named seam for feeds.
test("a document's root is named by the document: its own title, or its path, never the generic Everything", () => {
  expect(nameOf("/fold-data/fleet/page.json", "Everything")).toBe("Fleet");
  expect(nameOf("/fold-data/demo/page.json", "Demo")).toBe("Demo");
  expect(nameOf("/fold-data/work-notes/page.json", "Everything")).toBe("Work notes");
  expect(nameOf("/fold-data/x/weekly_plan.json", "")).toBe("Weekly plan");
  expect(nameOf("/fold-data/caf%C3%A9/index.json", undefined)).toBe("Café");
  expect(nameOf("/", "Everything")).toBe("Everything");
});

test("named: the root's title and every place a fold says the root, without changing the folds given", async () => {
  const input = read(goldens, "everything.input.json"), { url, fetch } = origin("fleet", input);
  const nodes = await open(fold, url, { fetch, opts: () => input.opts }), before = JSON.stringify([...nodes]);
  const n = named(nodes, "Fleet");
  expect(JSON.stringify([...nodes])).toBe(before);
  expect(n.get("root").title).toBe("Fleet");
  for (const [id, x] of n) if (id !== "root" && x.trail?.length) expect(x.trail[0]).toBe("Fleet");
  expect(JSON.stringify([...n])).not.toContain('"trail":["Everything"');
  // a deck's step back to the root says its name too
  const deck = named(new Map([["root", { title: "Everything", trail: [] }], ["a", { title: "A", trail: ["Everything"], prev: { id: "root", title: "Everything" } }]]), "Talk");
  expect(deck.get("a")).toEqual({ title: "A", trail: ["Talk"], prev: { id: "root", title: "Talk" } });
});

test("framed: every fold's path starts at Basis, and a missing fold's way back says the document's name", () => {
  const nodes = named(new Map([["root", { title: "Everything", trail: [], kids: [{ id: "a", title: "A" }] }], ["a", { title: "A", trail: ["Everything"], kids: [] }]]), "Fleet");
  const crumbs = (t: any) => t.children[0].children[0].children.map((li: any) => li.children[0]);
  const root = framed(viewOf(ax, nodes, "#/", "UTC").tree, "Fleet");
  expect(root.children.map((c: any) => c.role)).toEqual(["navigation", "heading", "list"]);
  expect(crumbs(root)).toEqual([{ role: "link", name: "Basis", href: "#basis" }]);
  expect(crumbs(framed(viewOf(ax, nodes, "#/a", "UTC").tree, "Fleet"))).toEqual([{ role: "link", name: "Basis", href: "#basis" }, { role: "link", name: "Fleet", href: "#/" }]);
  const gone = framed(viewOf(ax, nodes, "#/gone", "UTC").tree, "Fleet");
  expect(crumbs(gone).map((l: any) => l.name)).toEqual(["Basis", "Fleet"]);
  expect(gone.children[2].children.map((r: any) => r.children[0].name)).toEqual(["You asked for gone, and no fold has that path.", "Back to Fleet"]);
  // a root that could not be read: its way out is Basis, not a link to itself
  const failed = new Map([["root", { title: "Could not read the data", trail: [], failed: "Could not read x: 404" }]]);
  const lost = framed(viewOf(ax, failed, "", "UTC").tree, null, []);
  expect(crumbs(viewOf(ax, failed, "", "UTC").tree).map((l: any) => l.name)).toEqual(["Everything"]);
  expect(crumbs(lost)).toEqual([{ role: "link", name: "Basis", href: "#basis" }]);
  for (const t of [root, gone, lost]) toDom(doc, t);
  // Basis is no path: the hash names it alone
  expect([isBasis("#basis"), isBasis("#/basis"), isBasis("")]).toEqual([true, false, false]);
});

test("Basis: the box first, then the site's document and each other kept, entered in this tab", () => {
  let kept = keep([], { kind: "fold", path: "/fold-data/fleet/page.json", name: "Fleet" });
  kept = keep(kept, { kind: "fold", path: "/fold-data/demo/page.json", name: "Demo" });
  kept = keep(kept, { kind: "fold", path: "/fold-data/fleet/page.json", name: "Fleet 2" });
  expect(kept.map((s) => s.name)).toEqual(["Fleet 2", "Demo"]);
  expect(keep([null, { path: 1 }, "x"] as any, { kind: "fold", path: "/p.json", name: "P" })).toEqual([{ kind: "fold", path: "/p.json", name: "P" }]);
  const b = basisOf(ax, kept, "/fold-data/fleet/page.json", "Fleet", {}, "UTC");
  expect(b.node.kids.map((k: any) => [k.title, k.href])).toEqual([["Fleet", "./#/"], ["Demo", "./?data=%2Ffold-data%2Fdemo%2Fpage.json#/"]]);
  const main = toDom(doc, b.tree) as El, els = all(main);
  expect(els.filter((e) => e.tag === "h1").map((e) => e.textContent)).toEqual(["Basis"]);
  expect(els.some((e) => e.tag === "nav")).toBe(false); // the top: no path above it
  const form = els.find((e) => e.tag === "form")!;
  expect(form.attrs).toMatchObject({ role: "search", "aria-label": "Add a source", method: "get", action: "./" });
  const box = els.find((e) => e.tag === "input")!;
  expect(box.attrs).toEqual({ "aria-label": "Add a source", type: "search", name: "data", spellcheck: "false", autocomplete: "off", "aria-describedby": "source-note" });
  expect(els.find((e) => e.id === "source-note")!.textContent).toBe(BOX_NOTE);
  // the box is the first row, then one row per source; no source link opens another tab
  const list = main.kids.find((e) => e.tag === "ol")!;
  expect(list.kids.map((li) => li.className)).toEqual(["prop source", "obj", "obj"]);
  expect(els.filter((e) => e.tag === "a").every((a) => a.target == null)).toBe(true);
  // an address refused comes back in the box, and the note says why
  const no = all(toDom(doc, basisOf(ax, kept, "/fold-data/fleet/page.json", "Fleet", { value: "https://evil.test/x.json", error: "only a path on this site, from /, is read" }, "UTC").tree) as El);
  expect(no.find((e) => e.tag === "input")!.attrs.value).toBe("https://evil.test/x.json");
  expect(no.find((e) => e.id === "source-note")!.textContent).toBe("Not read: only a path on this site, from /, is read");
});

test("a feed is a kind of source named, not read yet: the box says so, and nothing is fetched", () => {
  const meta = { path: "/fold-data/fleet/page.json", source: "fleet-map" }, base = "https://phobos.test/fold/";
  for (const feed of ["https://example.test/podcast.rss", "/fold-data/news/feed.xml", "https://blog.test/feed", "https://blog.test/atom/", "feed:https://x.test/", "podcast://x.test/show"]) {
    expect(kindOf(feed)).toBe("feed");
    expect(dataOf(`?data=${encodeURIComponent(feed)}`, base, meta)).toEqual({ path: feed, kind: "feed", error: `${KINDS.feed} is not read yet; only a fold document is` });
  }
  for (const f of ["/fold-data/demo/page.json", "/fold-data/feeds/page.json", "/fold-data/rss-notes/page.json"]) expect(kindOf(f)).toBe("fold");
});

// Basis's tree as the viewer gives it, held in goldens/viewer/basis.ax.json (FOLD_BLESS=1 writes it from this output): src/html.rs
// writes its HTML (tests/html.rs), and js/html.test.ts holds toDom and lone to that, as for every golden fold.
test("Basis's tree is the golden one (goldens/viewer/basis.ax.json)", () => {
  const kept = [{ kind: "fold", path: "/fold-data/fleet/page.json", name: "Fleet" }, { kind: "fold", path: "/fold-data/demo/page.json", name: "Demo" }];
  const trees = {
    sources: basisOf(ax, kept, "/fold-data/fleet/page.json", "Fleet", {}, "UTC").tree,
    refused: basisOf(ax, kept, "/fold-data/fleet/page.json", "Fleet", { value: "https://example.test/podcast.rss", error: `${KINDS.feed} is not read yet; only a fold document is` }, "UTC").tree,
  };
  const f = join(goldens, "viewer", "basis.ax.json"), text = JSON.stringify(trees, null, 1) + "\n";
  if (process.env.FOLD_BLESS) require("node:fs").writeFileSync(f, text);
  expect(text).toBe(readFileSync(f, "utf8"));
});
