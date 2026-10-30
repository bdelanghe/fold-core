// The built viewer in a real browser (checks.viewer-browser): the package served at /fold/ and a golden page at
// /fold-data/fleet/page.json, as on phobos, and headless Chromium at the phone's and the iPad's size. Each location must render the
// fold the core folds there: the page loaded the module, fetched the document, folded it and derived the DOM, all in the browser.
//   VIEWER=<packages.viewer> CHROMIUM=<chromium binary> bun viewer/browser-check.ts <golden case>
import { readFileSync } from "node:fs";
import { join } from "node:path";
import { core } from "../js/fold-core.ts";
import { at, href } from "./view.js";

const [name = "sample"] = Bun.argv.slice(2);
const viewer = process.env.VIEWER!, chromium = process.env.CHROMIUM!;
const input = JSON.parse(readFileSync(join(import.meta.dir, "..", "goldens", `${name}.input.json`), "utf8"));
const page = JSON.stringify(input.root);
const server = Bun.serve({
  hostname: "127.0.0.1", port: 0,
  fetch(req) {
    const p = new URL(req.url).pathname;
    if (p === "/fold-data/fleet/page.json") return new Response(page, { headers: { "content-type": "application/json" } });
    // /fold, with no slash, is the page too (as a static server or a proxy may answer it): the page must go on to /fold/
    if (p === "/fold" || p.startsWith("/fold/")) {
      const f = Bun.file(join(viewer, p.slice("/fold/".length) || "index.html"));
      return f.exists().then((ok) => (ok ? new Response(f) : new Response("", { status: 404 })));
    }
    return new Response("", { status: 404 });
  },
});
const { fold } = await core(readFileSync(join(viewer, "fold_core.wasm")));
const nodes = new Map(fold({ root: input.root, files: {}, opts: { localTimeZone: "UTC" } }));
const root = nodes.get("root") as { title: string; kids: { id: string }[] };
const places = [["root", root], ...[root.kids[0]].filter(Boolean).map((k) => [k.id, nodes.get(k.id)])] as [string, { title: string; kids?: unknown[] }][];

const text = (s: string) => s.replace(/<[^>]*>/g, "").replace(/&amp;/g, "&").replace(/&lt;/g, "<").replace(/&gt;/g, ">").replace(/&quot;/g, '"').replace(/&#39;/g, "'");
let failed = 0;
const runs = [[390, 844], [1024, 1366]].flatMap(([w, h]) => places.map(([id, n]) => [w, h, id, n, "/fold/"] as const));
runs.push([390, 844, places[0][0], places[0][1], "/fold"]);
for (const [w, h, id, n, at0] of runs) {
  const url = `http://127.0.0.1:${server.port}${at0}${href(at(id), "folds")}`;
  // async, so this process's server answers the page while Chromium runs; a run past 60 s is killed and fails
  const p = Bun.spawn([chromium, "--headless=new", "--no-sandbox", "--disable-gpu", "--disable-dev-shm-usage", "--no-first-run", `--user-data-dir=${process.env.TMPDIR ?? "/tmp"}/chromium-${w}`,
    `--window-size=${w},${h}`, "--virtual-time-budget=10000", "--dump-dom", url], { stdout: "pipe", stderr: "pipe", timeout: 60_000 });
  const [dom, err] = await Promise.all([new Response(p.stdout).text(), new Response(p.stderr).text()]);
  await p.exited;
  const h1 = [...dom.matchAll(/<h1[^>]*>([\s\S]*?)<\/h1>/g)].map((m) => text(m[1]));
  const objs = (dom.match(/<li class="obj[ "]/g) ?? []).length;
  const ok = /data-ready="1"/.test(dom) && h1.length === 1 && h1[0] === n.title && objs === (n.kids ?? []).length;
  console.log(`${ok ? "ok  " : "FAIL"} ${w}x${h} ${at0} ${id}: heading ${JSON.stringify(h1)}, ${objs} rows (want ${JSON.stringify(n.title)}, ${(n.kids ?? []).length})`);
  if (!ok) { failed++; console.log(dom.slice(0, 2000), err.slice(-2000)); }
}
server.stop(true);
process.exit(failed ? 1 : 0);
