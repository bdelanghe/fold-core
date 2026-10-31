// The built viewer in a real browser (checks.viewer-browser): the package served at /fold/ and a golden page at
// /fold-data/fleet/page.json, as on phobos, and headless Chromium at the phone's and the iPad's size. Each location must render the
// fold the core folds there: the page loaded the module, fetched the document, folded it and derived the DOM, all in the browser.
//   VIEWER=<packages.viewer> CHROMIUM=<chromium binary> bun viewer/browser-check.ts <golden case>
import { readFileSync } from "node:fs";
import { join } from "node:path";
import { core } from "../js/fold-core.ts";
import { chrome } from "./chrome.ts";
import { at, href } from "./view.js";

const [name = "sample"] = Bun.argv.slice(2);
const viewer = process.env.VIEWER!, chromium = process.env.CHROMIUM!;
const input = JSON.parse(readFileSync(join(import.meta.dir, "..", "goldens", `${name}.input.json`), "utf8"));
// The Reading box's accessibility tree at each address: [role, name, value, description] for the landmark and its controls.
const reading: Record<string, string[][]> = JSON.parse(readFileSync(join(import.meta.dir, "..", "goldens", "viewer", "reading.ax.json"), "utf8"));
const page = JSON.stringify(input.root), demo = JSON.stringify({ ...input.root, title: "Demo" });
// Asleep, the host answers nothing, as phobos asleep: no refusal, no reply.
let asleep = false;
const server = Bun.serve({
  hostname: "127.0.0.1", port: 0,
  fetch(req) {
    if (asleep) return new Promise<Response>(() => {});
    const p = new URL(req.url).pathname;
    if (p === "/fold-data/fleet/page.json") return new Response(page, { headers: { "content-type": "application/json" } });
    if (p === "/fold-data/demo/page.json") return new Response(demo, { headers: { "content-type": "application/json" } });
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

// Over the DevTools protocol, not --dump-dom: the page renders after its module's awaits (wasm, then the document), which the load
// event doesn't wait for, so a snapshot can catch "Loading…". Here the check waits for data-ready and reads Chrome's own
// accessibility tree (Accessibility.getFullAXTree), the tree VoiceOver gets.
async function inChrome(w: number, h: number, url: string, submit?: string) {
  const c = await chrome(chromium, w, h), { send, value } = c;
  try {
    await send("Page.enable");
    await send("Page.navigate", { url });
    const until = Date.now() + 20_000;
    while ((await value("document.body?.dataset.ready")) !== "1" && Date.now() < until) await Bun.sleep(100);
    // typed into the Reading box and opened, as a person would: the page goes to ?data=<path> and reads that
    if (submit) {
      await value(`document.getElementById("reading").value = ${JSON.stringify(submit)}; document.querySelector("form.reading").requestSubmit()`);
      while ((await value(`location.search.includes("data=") && document.body?.dataset.ready`)) !== "1" && Date.now() < until) await Bun.sleep(100);
    }
    const ready = (await value("document.body?.dataset.ready")) === "1";
    const { nodes } = await send("Accessibility.getFullAXTree");
    const h1 = nodes.filter((x: any) => !x.ignored && x.role?.value === "heading" && x.properties?.some((q: any) => q.name === "level" && q.value.value === 1)).map((x: any) => x.name?.value ?? "");
    const objs = await value(`document.querySelectorAll("ol.rows > li.obj").length`);
    // the document read, on every fold: the "Reading" search landmark, its box holding the path, and Open
    const byId = new Map(nodes.map((x: any) => [x.nodeId, x]));
    const under = (x: any): any[] => [x, ...(x.childIds ?? []).flatMap((c: string) => (byId.has(c) ? under(byId.get(c)) : []))];
    const search = nodes.find((x: any) => !x.ignored && x.role?.value === "search");
    const box = (search ? under(search) : []).filter((x: any) => !x.ignored && ["search", "searchbox", "button"].includes(x.role?.value))
      .map((x: any) => [x.role.value, x.name?.value ?? "", x.value?.value, x.description?.value].filter((v) => v != null && v !== ""));
    const href = await value("location.pathname + decodeURIComponent(location.search)"); // Open submits the path %2F-encoded
    return { ready, h1, objs, box, href, err: c.err() };
  } finally { await c.close(); }
}

let failed = 0;
const runs = [[390, 844], [1024, 1366]].flatMap(([w, h]) => places.map(([id, n]) => [w, h, id, n, "/fold/"] as const));
runs.push([390, 844, places[0][0], places[0][1], "/fold"]);
// one viewer, another document: ?data= names it by path (the demo's root is retitled, so the heading says which was read)
runs.push([390, 844, places[0][0], { ...places[0][1], title: "Demo" }, "/fold/?data=/fold-data/demo/page.json"]);
// another origin named is refused, never fetched: the root says why, and the box says so
runs.push([390, 844, "root", { title: "Could not read the data", kids: [] }, "/fold/?data=https://evil.test/page.json"]);
// the box typed into and opened from the fleet's root: the demo, read at its own address
const typed = [390, 844, "root", { ...places[0][1], title: "Demo" }, "/fold/", "/fold-data/demo/page.json"] as const;
for (const [w, h, id, n, at0, submit] of [...runs, typed] as (readonly [number, number, string, { title: string; kids?: unknown[] }, string, string?])[]) {
  const url = `http://127.0.0.1:${server.port}${at0}${href(at(id), "folds")}`;
  const { ready, h1, objs, box, href: landed, err } = await inChrome(w, h, url, submit);
  const want = reading[landed];
  const ok = ready && h1.length === 1 && h1[0] === n.title && objs === (n.kids ?? []).length && !!want && JSON.stringify(box) === JSON.stringify(want);
  console.log(`${ok ? "ok  " : "FAIL"} ${w}x${h} ${at0}${submit ? ` opened ${submit}` : ""} ${id}: heading ${JSON.stringify(h1)}, ${objs} rows, reading ${JSON.stringify(box)} at ${landed} (want ${JSON.stringify(n.title)}, ${(n.kids ?? []).length}, ${JSON.stringify(want)})`);
  if (!ok) { failed++; console.log(err.slice(-2000)); }
}
// The host asleep (brief items 10 and 22): /fold opened once through its service worker (sw.js), then its host answers nothing.
// Opened again, it shows the last fold it read, its rows, and a note of since when and how long ago, which is the heading's
// description in Chrome's accessibility tree, so VoiceOver hears it with the title as the page opens.
async function whileAsleep(w: number, h: number) {
  const c = await chrome(chromium, w, h, `chromium-asleep-${w}`), { send, value } = c;
  const until = Date.now() + 60_000, wait = async (e: string) => { while (!(await value(e)) && Date.now() < until) await Bun.sleep(100); return !!(await value(e)); };
  // a fresh document each time: the old one's marker gone and the new one rendered
  const opened = async () => { await value("window.__was = 1"); await send("Page.navigate", { url: `http://127.0.0.1:${server.port}/fold/` }); return wait(`!window.__was && document.body?.dataset.ready === "1"`); };
  try {
    await send("Page.enable");
    const first = (await opened()) && (await wait("!!navigator.serviceWorker.controller"));
    const kept = await opened(); // read through the worker, so the page and the document are kept
    asleep = true;
    const again = await opened();
    const note = await value(`document.getElementById("stale").hidden ? null : document.getElementById("stale").textContent`);
    const { nodes } = await send("Accessibility.getFullAXTree");
    const h1 = nodes.filter((x: any) => !x.ignored && x.role?.value === "heading" && x.properties?.some((q: any) => q.name === "level" && q.value.value === 1))
      .map((x: any) => [x.name?.value ?? "", x.description?.value ?? ""]);
    const objs = await value(`document.querySelectorAll("ol.rows > li.obj").length`);
    return { first, kept, again, note, h1, objs, err: c.err() };
  } finally { asleep = false; await c.close(); }
}
for (const [w, h] of [[390, 844], [1024, 1366]]) {
  const { first, kept, again, note, h1, objs, err } = await whileAsleep(w, h);
  const said = /^Not updated since \d\d:\d\d, less than a minute ago\.$/.test(note ?? "");
  const ok = first && kept && again && said && h1.length === 1 && h1[0][0] === root.title && h1[0][1] === note && objs === root.kids.length;
  console.log(`${ok ? "ok  " : "FAIL"} ${w}x${h} /fold/ with its host asleep: opened ${[first, kept, again]}, heading ${JSON.stringify(h1)}, ${objs} rows, note ${JSON.stringify(note)} (want ${JSON.stringify(root.title)}, ${root.kids.length})`);
  if (!ok) { failed++; console.log(err.slice(-2000)); }
}
server.stop(true);
process.exit(failed ? 1 : 0);
