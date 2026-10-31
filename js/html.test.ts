// The Rust HTML (goldens/html, written by src/html.rs, native and WASM alike) held against the two things that are not it:
//   1. fold-dom.js toDom, the browser's own derivation from the same tree, put through a DOM (linkedom): the same element tree,
//      or the same refusal, for every golden tree;
//   2. lone (@bounded-systems/lone, `bless`: every validator), which must bless every golden fold's <main> with zero findings.
//   3. the same for every fold's whole page (goldens/pages, src/page.rs): lone blesses each page's <main>, and toDom derives a page's
//      ids, IRIs and same-tab links as src/page.rs writes them.
// What each proves: (1) that the one role table is one table, for these trees; (2) that lone's rules find nothing in these
// folds as written, with no stylesheet (so colour contrast is checked against no colours here, not proven: viewer/pages-check.ts
// checks it in Chromium, over the colours fold.css gives).
import { expect, test } from "bun:test";
import { readdirSync, readFileSync } from "node:fs";
import { join } from "node:path";
import { bless } from "@bounded-systems/lone";
import { parseHTML } from "linkedom";
import { toDom } from "../viewer/fold-dom.js";

const goldens = join(import.meta.dir, "..", "goldens");
const read = (...p: string[]) => JSON.parse(readFileSync(join(...p), "utf8"));
type Known = { case: string; ids: string[]; code: string; why: string; clears: string };
const known: Known[] = read(goldens, "html", "lone.known.json").known;
const { document } = parseHTML("<!doctype html><html><body></body></html>");
/** linkedom's `a.href = v` runs decodeURI on v (a URIError on a lone `%`); a browser keeps the attribute as given. */
const dom = {
  createElement(tag: string) {
    const e = document.createElement(tag);
    if (tag === "a") Object.defineProperty(e, "href", { get: () => e.getAttribute("href"), set: (v: string) => e.setAttribute("href", v) });
    return e;
  },
};
/** A DOM node as a canonical tree: tag, attributes by name, children; text as text. Compares trees, not escapings or the
 *  order a DOM keeps attributes in (linkedom writes them last-set first; browsers and src/html.rs, first-set first). */
const canon = (n: any): unknown => n.nodeType === 3 ? n.data : [n.localName, [...n.attributes].map((a: any) => [a.name, a.value]).sort(), [...n.childNodes].map(canon)];
const parsed = (h: string) => { const t = document.createElement("template"); t.innerHTML = h; return [...t.content.childNodes].map(canon); };

for (const f of readdirSync(join(goldens, "html")).filter((f) => f.endsWith(".html.json")).sort()) {
  const name = f.replace(/\.html\.json$/, "");
  const trees = read(goldens, "ax", `${name}.ax.json`), rows = read(goldens, "html", f);
  test(`html ${name}: toDom derives the same elements, or the same refusal`, () => {
    rows.forEach(([id, mode, href, want]: [string, string, string, string | { error: string }], i: number) => {
      const tree = trees[i][3], at = `${id} ${mode} ${href}`;
      if (typeof want !== "string") return expect(() => toDom(dom, tree), at).toThrow(want.error);
      expect(parsed(toDom(dom, tree).outerHTML), at).toEqual(parsed(want));
    });
  });
  test(`html ${name}: lone blesses every fold, but for the known findings in lone.known.json`, async () => {
    const mine = known.filter((k) => k.case === name), unknown: string[] = [], seen = new Set<string>();
    for (const [id, mode, href, want] of rows) {
      if (typeof want !== "string") continue;
      const { document: d } = parseHTML(`<!doctype html><html lang="en"><head><title>${id}</title></head><body>${want}</body></html>`);
      const r = await bless(d.querySelector("main") as any);
      for (const x of r.findings) {
        const k = mine.find((k) => k.code === x.code && k.ids.includes(id));
        if (k) seen.add(`${k.code} ${id}`);
        else unknown.push(`${id} ${mode} ${href}: ${x.severity} ${x.code} ${x.path} ${x.message.slice(0, 100)}`);
      }
    }
    expect(unknown).toEqual([]);
    // A known finding that no longer occurs comes off the list.
    expect(mine.flatMap((k) => k.ids.map((id) => `${k.code} ${id}`)).filter((k) => !seen.has(k))).toEqual([]);
  });
}

// Each page's <main>, blessed as each fold's is, with the same known findings (by case and fold id) and the same rule that a known
// finding which no longer occurs comes off the list.
for (const f of readdirSync(join(goldens, "pages")).filter((f) => f.endsWith(".pages.json")).sort()) {
  const name = f.replace(/\.pages\.json$/, "");
  test(`pages ${name}: lone blesses every page, but for the known findings in lone.known.json`, async () => {
    const mine = known.filter((k) => k.case === name), unknown: string[] = [], seen = new Set<string>();
    for (const p of read(goldens, "pages", f) as { id: string; mode: string; href: string; html?: string }[]) {
      if (!p.html) continue;
      const { document: d } = parseHTML(p.html);
      for (const x of (await bless(d.querySelector("main") as any)).findings) {
        const k = mine.find((k) => k.code === x.code && k.ids.includes(p.id));
        if (k) seen.add(`${k.code} ${p.id}`);
        else unknown.push(`${p.href}: ${x.severity} ${x.code} ${x.path} ${x.message.slice(0, 100)}`);
      }
    }
    expect(unknown).toEqual([]);
    expect(mine.flatMap((k) => k.ids.map((id) => `${k.code} ${id}`)).filter((k) => !seen.has(k))).toEqual([]);
  });
}

test("toDom writes a page's row as src/page.rs does (tests/pages.rs holds the same bytes)", () => {
  const tree = { role: "main", children: [
    { role: "heading", name: "Fleet", focus: true, id: "doc" },
    { role: "list", name: "Fleet", cls: "rows", children: [{ role: "listitem", index: 0, cls: "obj", id: "a%20b", haecceity: "https://ipso.bounded.tools/doc/a%20b", children: [
      { role: "decoration", cls: "n", name: "1" }, { role: "link", name: "Desk", href: "/fold/a%20b/#a%20b", description: "4 open", local: true },
    ] }] },
  ] };
  expect(parsed(toDom(dom, tree).outerHTML)).toEqual(parsed('<main><h1 id="doc" tabindex="-1">Fleet</h1><ol class="rows" role="list" aria-label="Fleet">' +
    '<li id="a%20b" resource="#a%20b" typeof="schema:WebPageElement" class="obj" data-i="0"><span property="schema:about" resource="https://ipso.bounded.tools/doc/a%20b"></span><span class="n" aria-hidden="true">1</span>' +
    '<a href="/fold/a%20b/#a%20b" aria-describedby="a%20b:hint">Desk</a><span class="hint" id="a%20b:hint">4 open</span></li></ol></main>'));
});

// The page fold serve answers an act's form with while acting is off (src/serve.rs not_yet; goldens/serve/act.html): lone blesses it.
test("serve act.html: lone blesses the page an act's form gets, with zero findings", async () => {
  const d = parseHTML(readFileSync(join(goldens, "serve", "act.html"), "utf8")).document;
  expect((await bless(d.querySelector("main") as any)).findings.map((x: any) => x.code)).toEqual([]);
});
