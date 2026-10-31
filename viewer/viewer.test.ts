// The viewer's own path on every golden, with the built module (FOLD_CORE_WASM): the document fetched from a data URL and folded here
// gives the golden folds; every location's tree is the golden one (the "viewer" links, read back by pathOf); every fold it can show
// derives a DOM without a refusal (fold-dom.js toDom: the AX gate); and the artificial viewer's walk over what it shows is the golden
// walk (the load gate: view.words and the rest, blocking where the golden blocks).
import { expect, test } from "bun:test";
import { readdirSync, readFileSync } from "node:fs";
import { join } from "node:path";
import { core } from "../js/fold-core.ts";
import { toDom } from "./fold-dom.js";
import { at, dataOf, href, open, pathOf, since, viewOf } from "./view.js";

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
