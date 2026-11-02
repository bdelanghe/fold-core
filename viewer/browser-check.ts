// The built viewer in a real browser (checks.viewer-browser): the package served at /fold/ and a golden page at
// /fold-data/fleet/page.json, as on phobos, and headless Chromium at the phone's and the iPad's size. Each location must render the
// fold the core folds there: the page loaded the module, fetched the document, folded it and derived the DOM, all in the browser.
// The document's root is named by the document (view.js nameOf: "Fleet", never "Everything"), every fold's path starts at Basis, and
// Basis (#basis) is the level above, with the add-a-source box and the sources read.
//   VIEWER=<packages.viewer> CHROMIUM=<chromium binary> bun viewer/browser-check.ts <golden case>
//   FOLD_BLESS=1 writes goldens/viewer/box.ax.json, the box's accessibility tree at each address, from what Chrome gives.
import { readFileSync, writeFileSync } from "node:fs";
import { join } from "node:path";
import { core } from "../js/fold-core.ts";
import { chrome } from "./chrome.ts";
import { at, href, nameOf } from "./view.js";

const [name = "sample"] = Bun.argv.slice(2);
const viewer = process.env.VIEWER!, chromium = process.env.CHROMIUM!;
const input = JSON.parse(readFileSync(join(import.meta.dir, "..", "goldens", `${name}.input.json`), "utf8"));
// Basis's box's accessibility tree at each address: [role, name, value, description] for the landmark and its controls.
const boxFile = join(import.meta.dir, "..", "goldens", "viewer", "box.ax.json"), bless = !!process.env.FOLD_BLESS;
const boxes: Record<string, string[][]> = bless ? {} : JSON.parse(readFileSync(boxFile, "utf8"));
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
const fleet = nameOf("/fold-data/fleet/page.json", root.title);
const places = [["root", { ...root, title: fleet }], ...[root.kids[0]].filter(Boolean).map((k) => [k.id, nodes.get(k.id)])] as [string, { title: string; kids?: unknown[] }][];

// Over the DevTools protocol, not --dump-dom: the page renders after its module's awaits (wasm, then the document), which the load
// event doesn't wait for, so a snapshot can catch "Loading…". Here the check waits for data-ready and reads Chrome's own
// accessibility tree (Accessibility.getFullAXTree), the tree VoiceOver gets.
type Seen = { ready: boolean; h1: string[]; objs: number; crumbs: string[]; rows: string[]; box: string[][]; at: string; cur: number; focus: string };
async function look(c: Awaited<ReturnType<typeof chrome>>): Promise<Seen> {
  const { send, value } = c;
  const ready = (await value("document.body?.dataset.ready")) === "1";
  const { nodes } = await send("Accessibility.getFullAXTree");
  const h1 = nodes.filter((x: any) => !x.ignored && x.role?.value === "heading" && x.properties?.some((q: any) => q.name === "level" && q.value.value === 1)).map((x: any) => x.name?.value ?? "");
  const objs = await value(`document.querySelectorAll("ol.rows > li.obj").length`);
  const crumbs = await value(`[...document.querySelectorAll("main nav a")].map(a => a.textContent)`);
  const rows = await value(`[...document.querySelectorAll("ol.rows > li.obj > a")].map(a => a.textContent + " " + a.getAttribute("href") + (a.target ? " " + a.target : ""))`);
  // the add-a-source box: the search landmark, its field (holding an address refused) and its button, as Chrome names them
  const byId = new Map(nodes.map((x: any) => [x.nodeId, x]));
  const under = (x: any): any[] => [x, ...(x.childIds ?? []).flatMap((c: string) => (byId.has(c) ? under(byId.get(c)) : []))];
  const search = nodes.find((x: any) => !x.ignored && x.role?.value === "search");
  const box = (search ? under(search) : []).filter((x: any) => !x.ignored && ["search", "searchbox", "button"].includes(x.role?.value))
    .map((x: any) => [x.role.value, x.name?.value ?? "", x.value?.value, x.description?.value].filter((v) => v != null && v !== ""));
  const at = await value("location.pathname + decodeURIComponent(location.search) + location.hash"); // the box submits the path %2F-encoded
  const cur = await value(`[...document.querySelectorAll("ol.rows > li.obj")].findIndex(li => li.classList.contains("cur"))`);
  const focus = await value(`document.activeElement?.tagName + (document.activeElement?.type ? ":" + document.activeElement.type : "")`);
  return { ready, h1, objs, crumbs, rows, box, at, cur, focus };
}
const until = () => Date.now() + 20_000;
async function settle(c: Awaited<ReturnType<typeof chrome>>, cond = `document.body?.dataset.ready === "1"`) {
  const t = until();
  while (!(await c.value(cond)) && Date.now() < t) await Bun.sleep(100);
}
async function go(c: Awaited<ReturnType<typeof chrome>>, url: string) {
  await c.value("window.__was = 1");
  await c.send("Page.navigate", { url });
  await settle(c, `!window.__was && document.body?.dataset.ready === "1"`);
}
const key = async (c: Awaited<ReturnType<typeof chrome>>, k: string) => {
  // Enter as a keyboard sends it (its code and key code, and a carriage return), so a field's form submits as it would by hand
  const enter = k === "Enter" ? { code: "Enter", windowsVirtualKeyCode: 13, nativeVirtualKeyCode: 13 } : {};
  for (const type of ["keyDown", "keyUp"]) await c.send("Input.dispatchKeyEvent", { type, key: k, ...enter, text: type === "keyDown" ? (k === "Enter" ? "\r" : k.length === 1 ? k : undefined) : undefined });
  await Bun.sleep(150);
};

let failed = 0;
const base = `http://127.0.0.1:${server.port}`;
function check(label: string, s: Seen, want: Partial<Seen>, err: () => string) {
  const got = Object.fromEntries(Object.keys(want).map((k) => [k, (s as any)[k]]));
  const ok = s.ready && JSON.stringify(got) === JSON.stringify(want);
  console.log(`${ok ? "ok  " : "FAIL"} ${label}: ${JSON.stringify(got)}${ok ? "" : ` (want ${JSON.stringify(want)})`}`);
  if (!ok) { failed++; console.log(err().slice(-2000)); }
}
/** The box's tree at an address, held in goldens/viewer/box.ax.json (or written there, blessing). */
const boxAt = (s: Seen) => { if (bless) boxes[s.at] = s.box; return boxes[s.at] ?? ["(no golden)"]; };

// Each document fold at the phone's and the iPad's size: the core's fold, its root named by the document, its path from Basis, and
// no box (it is Basis's, one level up).
for (const [w, h] of [[390, 844], [1024, 1366]]) for (const [id, n] of places) for (const at0 of w === 390 && id === "root" ? ["/fold/", "/fold"] : ["/fold/"]) {
  const c = await chrome(chromium, w, h, `chromium-doc-${w}-${id}-${at0.length}`);
  try {
    await c.send("Page.enable");
    await go(c, `${base}${at0}${href(at(id), "folds")}`);
    check(`${w}x${h} ${at0} ${id}`, await look(c), { h1: [n.title], objs: (n.kids ?? []).length, crumbs: id === "root" ? ["Basis"] : ["Basis", fleet], box: [] }, c.err);
  } finally { await c.close(); }
}

// Basis, in one browser, as a person goes: the fleet read, then the demo named in the address; out of its root (h) to Basis, the
// cursor on the demo's row; / to the box; a path typed and opened (Enter); then an address refused, said in the box.
for (const [w, h] of [[390, 844], [1024, 1366]]) {
  const c = await chrome(chromium, w, h, `chromium-basis-${w}`);
  try {
    await c.send("Page.enable");
    await go(c, `${base}/fold/#/`);
    check(`${w}x${h} the fleet's root`, await look(c), { h1: [fleet], crumbs: ["Basis"] }, c.err);
    await go(c, `${base}/fold/?data=/fold-data/demo/page.json#/`);
    check(`${w}x${h} the demo, named in the address`, await look(c), { h1: ["Demo"], crumbs: ["Basis"], box: [] }, c.err);
    await key(c, "h"); await settle(c, `location.hash === "#basis"`);
    let s = await look(c);
    const sources = [`${fleet} ./#/`, "Demo ./?data=%2Ffold-data%2Fdemo%2Fpage.json#/"];
    check(`${w}x${h} h from the demo's root: Basis, on the demo's row`, s, { h1: ["Basis"], crumbs: [], rows: sources, cur: 1, box: boxAt(s) }, c.err);
    await key(c, "k"); await key(c, "l"); await settle(c, `location.hash === "#/" && !location.search && document.body?.dataset.ready === "1"`);
    check(`${w}x${h} k, l from Basis: the fleet, in this tab`, await look(c), { h1: [fleet], at: "/fold/#/" }, c.err);
    await key(c, "/"); await settle(c, `location.hash === "#basis" && document.activeElement?.type === "search"`);
    s = await look(c);
    check(`${w}x${h} / from a fold: Basis's box, focused`, s, { h1: ["Basis"], focus: "INPUT:search", box: boxAt(s) }, c.err);
    await c.send("Input.insertText", { text: "/fold-data/demo/page.json" });
    await c.value("window.__was = 1");
    await key(c, "Enter"); await settle(c, `!window.__was && document.body?.dataset.ready === "1"`);
    check(`${w}x${h} typed into the box, Enter`, await look(c), { h1: ["Demo"], at: "/fold/?data=/fold-data/demo/page.json" }, c.err);
    await go(c, `${base}/fold/?data=https://evil.test/page.json#basis`);
    s = await look(c);
    check(`${w}x${h} another site's address: refused, back in the box`, s, { h1: ["Basis"], rows: sources, box: boxAt(s) }, c.err);
    await go(c, `${base}/fold/?data=https://example.test/podcast.rss`);
    check(`${w}x${h} a feed: its root says why, its path goes to Basis`, await look(c), { h1: ["Could not read the data"], crumbs: ["Basis"] }, c.err);
    await go(c, `${base}/fold/?data=https://example.test/podcast.rss#basis`);
    s = await look(c);
    check(`${w}x${h} a feed, on Basis: named, not read yet`, s, { h1: ["Basis"], rows: sources, box: boxAt(s) }, c.err);
  } finally { await c.close(); }
}
if (bless) writeFileSync(boxFile, JSON.stringify(boxes, null, 1) + "\n");

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
  const ok = first && kept && again && said && h1.length === 1 && h1[0][0] === fleet && h1[0][1] === note && objs === root.kids.length;
  console.log(`${ok ? "ok  " : "FAIL"} ${w}x${h} /fold/ with its host asleep: opened ${[first, kept, again]}, heading ${JSON.stringify(h1)}, ${objs} rows, note ${JSON.stringify(note)} (want ${JSON.stringify(fleet)}, ${root.kids.length})`);
  if (!ok) { failed++; console.log(err.slice(-2000)); }
}
server.stop(true);
process.exit(failed ? 1 : 0);
