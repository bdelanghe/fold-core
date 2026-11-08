// Basis's add-a-source box takes a stored query by NAME (Bobby 11-07: "this needs to change to be a named query"). Pure view.js: no module, no browser.
import { expect, test } from "bun:test";
import { dataOf, KINDS, kindOf, QUERY_NAME, queryPath } from "./view.js";

const base = "https://basis.example/fold/";
const meta = { path: "/fold-data/fleet/page.json", source: "fleet" };

test("a bare name is a stored query; a path, a URL, a feed and a dotted or slashed word are not", () => {
  for (const q of ["needs-you", "needs-health", "repos-open-prs", "ci-failing", "trip"]) expect(kindOf(q)).toBe("query");
  expect(kindOf("/fold-data/demo/page.json")).toBe("fold");
  expect(kindOf("https://example.test/x")).toBe("fold"); // refused later by dataOf (another site)
  expect(kindOf("demo/page.json")).toBe("fold");
  expect(kindOf("page.json")).toBe("fold"); // a dot: never a name
  expect(kindOf("feed:https://example.test/x")).toBe("feed");
  expect(kindOf("https://example.test/podcast.rss")).toBe("feed");
  expect(kindOf("Needs-You")).toBe("fold"); // names are lowercase: the registry's, exactly
  expect(KINDS.query).toBe("a stored query");
});

test("a name opens the query's fold on this site: no path is ever built from free text beyond a validated name", () => {
  expect(queryPath("needs-you")).toBe("/fold-data/fleet/q.needs-you.jsonld");
  expect(dataOf("?data=needs-you", base, meta)).toEqual({ path: "/fold-data/fleet/q.needs-you.jsonld", url: "https://basis.example/fold-data/fleet/q.needs-you.jsonld", source: null, kind: "query", query: "needs-you" });
  expect(dataOf("?data=%20ci-failing%20", base, meta).query).toBe("ci-failing"); // trimmed
  // nothing else can reach queryPath: a traversal, a scheme, an encoded slash are not names
  for (const bad of ["../etc/passwd", "a/../b", "needs-you/../../x", "x:y", "x%2Fy", "x y", ".hidden", "-lead"]) expect(QUERY_NAME.test(bad)).toBe(false);
  expect(dataOf("?data=..%2F..%2Fsecret", base, meta).kind).not.toBe("query");
});

test("the page's module script parses (the merge of Basis with the rotor, live folds and the notices zone left no syntax error or stale name)", async () => {
  const html = await Bun.file(`${import.meta.dir}/index.html`).text();
  const script = html.match(/<script type="module">([\s\S]*?)<\/script>/)![1];
  expect(() => new Bun.Transpiler({ loader: "js" }).transformSync(script)).not.toThrow();
  // every name the script imports from view.js exists there
  const view = await import("./view.js");
  const imported = script.match(/import \{([^}]*)\} from "\.\/view\.js"/)![1].split(",").map((s) => s.trim().split(/\s+as\s+/)[0]).filter(Boolean);
  expect(imported.filter((n) => !(n in view))).toEqual([]);
  // nothing refers to the Reading box any more
  expect(html).not.toContain('getElementById("reading")');
  expect(html).not.toContain('class="reading"');
});

test("the existing sources are unchanged: the default document, a path on this site, another origin refused, a feed named not read", () => {
  expect(dataOf("", base, meta)).toEqual({ path: meta.path, url: "https://basis.example/fold-data/fleet/page.json", source: "fleet" });
  expect(dataOf("?data=/fold-data/demo/page.json", base, meta)).toMatchObject({ path: "/fold-data/demo/page.json", source: null });
  expect(dataOf("?data=https://evil.test/page.json", base, meta).error).toContain("only a path on this site");
  expect(dataOf("?data=https://example.test/podcast.rss", base, meta)).toMatchObject({ kind: "feed" });
});
