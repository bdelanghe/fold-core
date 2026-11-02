// The built viewer in a real browser (checks.viewer-browser): the package served at /fold/ and a golden page at
// /fold-data/fleet/page.json, as on phobos, and headless Chromium at the phone's and the iPad's size. Each location must render the
// fold the core folds there: the page loaded the module, fetched the document, folded it and derived the DOM, all in the browser.
// Needs you is read live beside it (/fold/needs.json, two intents waiting), so every run shows Approvals; then one intent is approved
// with Enter and the other declined with its passkey button (chitin-approve's API faked here, its decline as addendum 1 names it,
// the passkey a stub: nothing real is signed),
// and each leaves, its result said in the notices zone. Through it all the zone keeps its place and size and the fold does not move:
// a notice never bumps the page (the brief's semantic-zones addendum).
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
// Needs you, live: what fold serve's needs.json would list, and chitin-approve's two endpoints, faked; every call to them is kept.
const intent = (id: string, to: string, text: string) => ({ id, as: "bobby", declined: false, expired: false, to, text, queued_by: "claude-break-glass", text_hash: "deadbeefcafe", created_at: "2026-10-30T11:57:55.000Z" });
let waiting = [intent("0123456789ab", "personal-assistant", "Route the plan to the hubs."), intent("0a0a0a0a0a0a", "personal-cake-goal", "Land the edge grant.")];
const seen: string[] = [];
const answer = (v: unknown) => new Response(JSON.stringify(v), { headers: { "content-type": "application/json", "cache-control": "no-store" } });
async function live(req: Request, p: string): Promise<Response | null> {
  if (p === "/fold/needs.json") return answer({ as: "bobby", waiting });
  const m = p.match(/^\/approve\/v1\/(challenge|assertion|decline)\/([0-9a-f]{12})$/);
  if (!m) return null;
  const [, what, id] = m, it = waiting.find((w) => w.id === id);
  seen.push(`${req.method} ${p}${req.method === "POST" ? " " + req.headers.get("content-type") : ""}`);
  if (!it) return answer({ ok: false, reason: `${id} is not waiting` });
  const pk = { challenge: "AQID", rpId: "127.0.0.1", allowCredentials: [{ type: "public-key", id: "CQ" }], userVerification: "required", timeout: 60000 };
  if (what === "decline" && req.method === "GET") return answer({ ok: true, verdict: "decline", intent: it, publicKey: pk });
  if (what === "challenge") return answer({ ok: true, intent: it, frame: { hop: it.to, hash: "f00dfeed", iat: "2026-11-02T00:00:00.000Z" },
    publicKey: { challenge: "AQID", rpId: "127.0.0.1", allowCredentials: [{ type: "public-key", id: "CQ" }], userVerification: "required", timeout: 60000 } });
  const body = await req.json().catch(() => null);
  if (body?.response?.signature !== "Aw") return answer({ ok: false, reason: "not the stub's assertion" });
  waiting = waiting.filter((w) => w.id !== id);
  return answer(what === "assertion" ? { ok: true, message: "written m-9", outcome: "written" } : { ok: true, message: `${id} declined` });
}
const server = Bun.serve({
  hostname: "127.0.0.1", port: 0,
  fetch(req) {
    if (asleep) return new Promise<Response>(() => {});
    const p = new URL(req.url).pathname;
    const l = live(req, p);
    if (p === "/fold/needs.json" || p.startsWith("/approve/")) return l.then((r) => r ?? new Response("", { status: 404 }));
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
const { fold, needs } = await core(readFileSync(join(viewer, "fold_core.wasm")));
// the fleet's page as the viewer folds it: with Needs you read live
const nodes = new Map(fold({ root: needs({ root: input.root, needs: { as: "bobby", waiting }, now: Date.now() }), files: {}, opts: { localTimeZone: "UTC" } }));
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
    const zone = await value(`[...document.querySelectorAll("#notices p")].map((p) => p.textContent).filter(Boolean).join(" | ")`);
    return { ready, h1, objs, box, href, zone, err: c.err() };
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
  const { ready, h1, objs, box, href: landed, zone, err } = await inChrome(w, h, url, submit);
  const want = reading[landed];
  // the notices zone: why a document was not read is said there, never in the Reading box; otherwise it says there is nothing
  const wantZone = landed.includes("evil.test") ? "Not read: only a path on this site, from /, is read" : "No notices";
  const ok = ready && h1.length === 1 && h1[0] === n.title && objs === (n.kids ?? []).length && !!want && JSON.stringify(box) === JSON.stringify(want) && zone === wantZone;
  console.log(`${ok ? "ok  " : "FAIL"} ${w}x${h} ${at0}${submit ? ` opened ${submit}` : ""} ${id}: heading ${JSON.stringify(h1)}, ${objs} rows, reading ${JSON.stringify(box)} at ${landed} notices ${JSON.stringify(zone)} (want ${JSON.stringify(n.title)}, ${(n.kids ?? []).length}, ${JSON.stringify(want)}, ${JSON.stringify(wantZone)})`);
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
    const zones = () => value(`JSON.stringify([document.getElementById("notices").getBoundingClientRect(), document.querySelector("main").getBoundingClientRect()].flatMap((r) => [Math.round(r.top), Math.round(r.height)]).slice(0, 3))`);
    const fresh = await zones();
    asleep = true;
    const again = await opened();
    const note = await value(`document.getElementById("stale").textContent || null`), stale = await zones();
    const { nodes } = await send("Accessibility.getFullAXTree");
    const h1 = nodes.filter((x: any) => !x.ignored && x.role?.value === "heading" && x.properties?.some((q: any) => q.name === "level" && q.value.value === 1))
      .map((x: any) => [x.name?.value ?? "", x.description?.value ?? ""]);
    const objs = await value(`document.querySelectorAll("ol.rows > li.obj").length`);
    return { first, kept, again, note, h1, objs, still: fresh === stale, zones: [fresh, stale], err: c.err() };
  } finally { asleep = false; await c.close(); }
}
for (const [w, h] of [[390, 844], [1024, 1366]]) {
  const { first, kept, again, note, h1, objs, still, zones, err } = await whileAsleep(w, h);
  const said = /^Not updated since \d\d:\d\d, less than a minute ago\.$/.test(note ?? "");
  const ok = first && kept && again && said && h1.length === 1 && h1[0][0] === root.title && h1[0][1] === note && objs === root.kids.length && still;
  console.log(`${ok ? "ok  " : "FAIL"} ${w}x${h} /fold/ with its host asleep: zones fresh/stale ${JSON.stringify(zones)}, opened ${[first, kept, again]}, heading ${JSON.stringify(h1)}, ${objs} rows, note ${JSON.stringify(note)} (want ${JSON.stringify(root.title)}, ${root.kids.length})`);
  if (!ok) { failed++; console.log(err.slice(-2000)); }
}
// Needs you, live, in the browser: the item's two acts are form landmarks named by their labels; Enter on the item runs its passkey
// (the stub authenticator; the words said before it are the challenge's), it leaves, and its group says what was done; ]] then steps
// from Approvals to the next fold beside it; the other item's "Decline with passkey" button runs its passkey against the decline
// endpoint, Approvals goes, and Needs you says so.
async function signs(w: number, h: number) {
  const c = await chrome(chromium, w, h, `chromium-needs-${w}`), { send, value } = c;
  const until = Date.now() + 30_000, wait = async (e: string) => { while (!(await value(e)) && Date.now() < until) await Bun.sleep(100); return !!(await value(e)); };
  const key = async (k: string, code: string, vk: number) => { for (const type of ["keyDown", "keyUp"]) await send("Input.dispatchKeyEvent", { type, key: k, code, windowsVirtualKeyCode: vk, ...(type === "keyDown" && k.length === 1 ? { text: k } : {}) }); };
  try {
    await send("Page.enable");
    await send("Page.navigate", { url: `http://127.0.0.1:${server.port}/fold/#/needs/approvals/needs-sign-0123456789ab` });
    const ready = await wait(`document.body?.dataset.ready === "1" && document.querySelector("main h1")?.textContent === "To personal-assistant"`);
    // the passkey, stubbed: what it was asked for is kept on the page
    await value(`navigator.credentials.get = async (o) => { window.__asked = [...new Uint8Array(o.publicKey.challenge)].join(",") + " " + o.publicKey.userVerification;
      const b = (n) => new Uint8Array([n]).buffer; return { id: "cred", rawId: b(9), type: "public-key", response: { clientDataJSON: b(1), authenticatorData: b(2), signature: b(3), userHandle: null } }; }`);
    const { nodes: ax } = await send("Accessibility.getFullAXTree");
    const forms = ax.filter((x: any) => !x.ignored && x.role?.value === "form").map((x: any) => x.name?.value);
    // where the zones are: the notices zone's top and height and the fold's top, in CSS px
    const rects = () => value(`JSON.stringify([document.getElementById("notices").getBoundingClientRect(), document.querySelector("main").getBoundingClientRect()].flatMap((r) => [Math.round(r.top), Math.round(r.height)]).slice(0, 3))`);
    const empty = await value(`document.getElementById("notice").textContent`), before = await rects();
    // the passkey held until released, so the zone can be measured while it says what is being signed
    await value(`window.__hold = new Promise((r) => { window.__release = r; }); const g = navigator.credentials.get; navigator.credentials.get = async (o) => { await window.__hold; return g(o); }`);
    await value(`document.querySelector("main h1").focus()`);
    await key("Enter", "Enter", 13);
    const during = (await wait(`document.getElementById("notice").textContent.startsWith("Approving for personal-assistant")`)) ? await rects() : "never said";
    await value("window.__release()");
    const approved = await wait(`location.hash === "#/needs/approvals" && document.getElementById("notice").textContent.startsWith("Sent to personal-assistant")`);
    const asked = await value("window.__asked");
    const said1 = await value(`document.getElementById("notice").textContent`), after = await rects();
    await value(`document.querySelector("main h1").focus()`);
    await key("]", "BracketRight", 221); await key("]", "BracketRight", 221);
    const stepped = await wait(`location.hash === "#/needs/needs-ask-0"`);
    await send("Page.navigate", { url: `http://127.0.0.1:${server.port}/fold/#/needs/approvals/needs-sign-0a0a0a0a0a0a` });
    await wait(`document.querySelector("main h1")?.textContent === "To personal-cake-goal"`);
    await value(`document.querySelector('form[data-call="decline"] button').click()`);
    const declined = await wait(`location.hash === "#/needs" && document.getElementById("notice").textContent.startsWith("Declined.")`);
    const groups = await value(`[...document.querySelectorAll("ol.rows > li.obj > a")].map((a) => a.textContent).join("|")`);
    const last = await rects();
    return { ready, forms, approved, asked, said1, stepped, declined, groups, empty, still: [before, during, after, last], err: c.err() };
  } finally { await c.close(); }
}
{
  const r = await signs(390, 844);
  const ok = r.ready && JSON.stringify(r.forms) === JSON.stringify(["Approve with passkey", "Decline with passkey"]) && r.approved && r.asked === "1,2,3 required"
    && r.stepped && r.declined && !r.groups.includes("Approvals") && waiting.length === 0
    && r.empty === "No notices" && r.still.every((x: string) => x === r.still[0])
    && JSON.stringify(seen) === JSON.stringify(["GET /approve/v1/challenge/0123456789ab", "POST /approve/v1/assertion/0123456789ab application/json", "GET /approve/v1/decline/0a0a0a0a0a0a", "POST /approve/v1/decline/0a0a0a0a0a0a application/json"]);
  console.log(`${ok ? "ok  " : "FAIL"} 390x844 Needs you: forms ${JSON.stringify(r.forms)}, approved by Enter ${r.approved} (passkey asked ${JSON.stringify(r.asked)}, said ${JSON.stringify(r.said1)}), ]] ${r.stepped}, declined ${r.declined}, rows ${JSON.stringify(r.groups)}, zones [notices top, height; fold top] before/during/after/last ${JSON.stringify(r.still)}, calls ${JSON.stringify(seen)}`);
  if (!ok) { failed++; console.log(r.err.slice(-2000)); }
}
server.stop(true);
process.exit(failed ? 1 : 0);
