// The viewer's own path on every golden, with the built module (FOLD_CORE_WASM): the document fetched from a data URL and folded here
// gives the golden folds; every location's tree is the golden one (the "viewer" links, read back by pathOf); every fold it can show
// derives a DOM without a refusal (fold-dom.js toDom: the AX gate); and the artificial viewer's walk over what it shows is the golden
// walk (the load gate: view.words and the rest, blocking where the golden blocks).
import { expect, test } from "bun:test";
import { readdirSync, readFileSync } from "node:fs";
import { join } from "node:path";
import { core } from "../js/fold-core.ts";
import { toDom } from "./fold-dom.js";
import { at, beside, dataOf, foldNodes, graft, haecceityOf, href, liveFolds, liveNodes, manifestRoot, open, pathOf, placeIn, revalidate, segmentOf, shows, since, stamp, underived, viewOf } from "./view.js";
import { ENDPOINTS, dec, enc, notifyHere, notifyRefresh, readNeeds, sign } from "./sign.js";
import { aboutOf, contextOf, embed, flagUnnamed, pinOf, reportToken } from "./clax.js";
import { KINDS, kindItems, kindView, rotorKey, rotorStart } from "./rotor.js";

const goldens = join(import.meta.dir, "..", "goldens");
const wasm = process.env.FOLD_CORE_WASM ?? join(import.meta.dir, "..", "target", "wasm32-unknown-unknown", "release", "fold_core.wasm");
const { fold, walk, ax, needs } = await core(readFileSync(wasm));
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
  // Basis shows its reports in its own fold, so the overlay is told to leave out its Your reports button (clax OverlayOptions.reports)
  expect(readFileSync(join(import.meta.dir, "index.html"), "utf8")).toContain('<meta name="clax-reports" content="off">');

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

// Needs you, live (src/needs.rs, goldens/needs/overlay.json): the module's needs entry is the core's overlay, and the viewer folds the
// document it gives; the live item's two acts are forms naming the intent, and ]] [[ step between the folds beside one.
test("needs: the overlay through the module is the golden, and the viewer folds it to Approvals' item with its two acts", async () => {
  const cases = read(goldens, "needs", "overlay.json");
  for (const c of cases) expect(JSON.stringify(needs(c.input))).toBe(JSON.stringify(c.root));
  const live = cases[0].input, body = JSON.stringify(live.root);
  const nodes = await open(fold, "https://data.test/page.json", { fetch: async () => new Response(body), opts: () => ({ localTimeZone: "UTC" }),
    root: (root: unknown) => needs({ root, needs: live.needs, now: live.now }) });
  const at = ["needs", "approvals", "needs-sign-0123456789ab"];
  expect(nodes.has(at.join("~"))).toBe(true);
  expect([...nodes.keys()].some((k) => k.includes("999999999999"))).toBe(false);
  const els = all(toDom(doc, viewOf(ax, nodes, href(at, "folds"), "UTC").tree) as El);
  expect(els.filter((e) => e.tag === "form").map((e) => [e.attrs["aria-label"], e.attrs.method, e.attrs.action, e.attrs["data-intent"], e.attrs["data-call"]])).toEqual([
    ["Sign with passkey", "get", "", "0123456789ab", "approve"],
    ["Decline with passkey", "get", "", "0123456789ab", "decline"],
  ]);
  // ]] and [[: path arithmetic over the parent's folds, null at either end, at the root, and onto a row that is a link
  expect(beside(nodes, ["needs", "approvals"], 1)).toEqual(["needs", "needs-ask-0"]);
  expect(beside(nodes, ["needs", "needs-ask-0"], -1)).toEqual(["needs", "approvals"]);
  expect(beside(nodes, at, 1)).toEqual(["needs", "approvals", "needs-sign-0a0a0a0a0a0a"]);
  expect(beside(nodes, at, -1)).toBeNull();
  expect(beside(nodes, ["needs"], 1)).toEqual(["agents"]);
  expect(beside(nodes, [], 1)).toBeNull();
  expect(beside(new Map([["root", { kids: [{ id: "a" }, { id: "top", href: "#/" }] }]]), ["a"], 1)).toBeNull();
});

/** chitin-approve, faked: one verdict's challenge for one intent and its assertion, recording every call in order. `verdict` is what the
 * challenge says it is for (absent: says nothing). */
function gateway(intent: { id: string; to: string; text: string }, { challenge = [200], assertion = [200], refuse = "", verdict = undefined as string | undefined } = {}) {
  const calls: string[] = [];
  const fetch = async (url: string, init?: RequestInit) => {
    const post = init?.method === "POST";
    calls.push(`${post ? "POST" : "GET"} ${url}${post ? " " + init!.body : ""}`);
    const status = (post ? assertion : challenge).shift() ?? 200;
    if (status !== 200) return new Response(JSON.stringify({ ok: false, reason: `status ${status}` }), { status });
    if (refuse) return new Response(JSON.stringify({ ok: false, reason: refuse }), { status: 409 });
    return new Response(JSON.stringify(post ? { ok: true, message: "written m-9", outcome: "written" }
      : { ok: true, ...(verdict ? { verdict } : {}), intent: { ...intent, as: "bobby", text_hash: "deadbeefcafe" }, ...(verdict === "decline" ? {} : { frame: { hop: intent.to, hash: "f00dfeed", iat: "x" } }),
          publicKey: { challenge: enc(new Uint8Array([1, 2, 3]).buffer), rpId: "phobos.test", allowCredentials: [{ type: "public-key", id: enc(new Uint8Array([9]).buffer) }], timeout: 60000 } }));
  };
  const credentials = { get: async (o: any) => {
    calls.push(`passkey ${new Uint8Array(o.publicKey.challenge).join(",")} ${o.publicKey.rpId} ${o.publicKey.userVerification} ${new Uint8Array(o.publicKey.allowCredentials[0].id).join(",")}`);
    const b = (n: number) => new Uint8Array([n]).buffer;
    return { id: "cred", rawId: b(9), type: "public-key", response: { clientDataJSON: b(1), authenticatorData: b(2), signature: b(3), userHandle: null } };
  } };
  return { calls, fetch: fetch as unknown as typeof globalThis.fetch, credentials, wait: async () => {} };
}

test("sign: approve signs only the challenge's own intent, said before it asks, at endpoints built from the id", async () => {
  const id = "0123456789ab", it = { id, to: "personal-assistant", text: "Route the plan." }, shown = { to: it.to, text: it.text };
  expect(ENDPOINTS.approve(id)).toEqual(["/approve/v1/challenge/0123456789ab", "/approve/v1/assertion/0123456789ab"]);
  expect(new Uint8Array(dec(enc(new Uint8Array([0, 250, 255]).buffer)))).toEqual(new Uint8Array([0, 250, 255]));
  // the one path that signs: the challenge, its words said, the passkey, the assertion
  let g = gateway(it), said: string[] = [];
  const say = (m: string) => { said.push(m); g.calls.push("said"); };
  expect(await sign(id, "approve", shown, { ...g, say })).toEqual({ ok: true, said: "Sent to personal-assistant. written m-9" });
  expect(g.calls.map((c) => c.split(" {")[0])).toEqual(["GET /approve/v1/challenge/0123456789ab", "said", "passkey 1,2,3 phobos.test required 9", "POST /approve/v1/assertion/0123456789ab"]);
  expect(said).toEqual(["Signing for personal-assistant, #f00dfeed: “Route the plan.”. Your passkey asks next."]);
  expect(JSON.parse(g.calls[3].slice(g.calls[3].indexOf("{")))).toEqual({ id: "cred", rawId: "CQ", type: "public-key", response: { clientDataJSON: "AQ", authenticatorData: "Ag", signature: "Aw", userHandle: null } });
  // a challenge that says it is for a decline is never signed as an approval
  g = gateway(it, { verdict: "decline" });
  expect(await sign(id, "approve", shown, g)).toEqual({ ok: false, said: "Not signed: the challenge is not for an approval." });
  expect(g.calls.length).toBe(1);
  // what the item showed is not what the challenge would sign: nothing is asked of the passkey
  g = gateway({ ...it, text: "Route the plan, and grant an edge." });
  const changed = await sign(id, "approve", shown, g);
  expect(changed.ok).toBe(false); expect(changed.changed).toBe(true);
  expect(g.calls.some((c) => c.startsWith("passkey") || c.startsWith("POST"))).toBe(false);
  g = gateway({ ...it, to: "personal-cake-goal" });
  expect((await sign(id, "approve", shown, g)).changed).toBe(true);
  // a challenge for another intent
  g = gateway({ ...it, id: "bbbbbbbbbbbb" });
  expect((await sign(id, "approve", shown, g)).said).toBe("Not signed: the challenge is for another intent.");
  // refused: chitin's reason, nothing signed
  g = gateway(it, { refuse: "recipient personal-assistant is not running" });
  expect(await sign(id, "approve", shown, g)).toEqual({ ok: false, said: "Not signed: recipient personal-assistant is not running." });
  expect(g.calls.length).toBe(1);
  // no longer waiting (signed, sent, declined, being signed or expired): nothing to sign, and `changed`, so the page reads Needs you
  // again and the row goes, rather than failing the same way on every press (Bobby 11-03: "I can't sign these two, it always fails")
  for (const reason of [`no waiting item ${id}`, `${id} was declined and is never retried; queue it again`, `${id} is already being approved`, `${id} is expired (the limit is 24 hours): queue it again`]) {
    for (const verdict of ["approve", "decline"]) {
      g = gateway(it, { refuse: reason });
      expect(await sign(id, verdict, shown, g)).toEqual({ ok: false, changed: true, said: `Nothing to sign: ${reason}.` });
      expect(g.calls.length).toBe(1);
    }
  }
  // busy (429) and a slow challenge (504) are asked again; an assertion that timed out (504) never is
  g = gateway(it, { challenge: [429, 504, 200] });
  expect((await sign(id, "approve", shown, g)).ok).toBe(true);
  g = gateway(it, { assertion: [504, 200] });
  expect(await sign(id, "approve", shown, g)).toEqual({ ok: false, said: "Not signed: status 504." });
  expect(g.calls.filter((c) => c.startsWith("POST")).length).toBe(1);
  // not an intent, or not a verdict: nothing is fetched
  g = gateway(it);
  for (const bad of ["../x", "0123456789AB", "", "0123456789abc"]) expect((await sign(bad, "approve", shown, g)).ok).toBe(false);
  for (const v of ["ack", "toString", "__proto__", ""]) expect((await sign(id, v, shown, g)).ok).toBe(false);
  expect(g.calls).toEqual([]);
});

// Addendum 1 (Bobby: "we should add a decline with passkey in the approval flow"): a decline is the passkey too, at its own endpoint,
// and only a challenge that says it is a decline is signed as one, so an approval can never be passed off as a decline or back.
test("sign: decline is a passkey verdict at its own endpoint, bound to a challenge that says it is one, and the item's h", async () => {
  const id = "0123456789ab", it = { id, to: "personal-assistant", text: "Route the plan." }, shown = { to: it.to, text: it.text };
  expect(ENDPOINTS.decline(id)).toEqual(["/approve/v1/decline/0123456789ab", "/approve/v1/decline/0123456789ab"]);
  let g = gateway(it, { verdict: "decline" }), said: string[] = [];
  expect(await sign(id, "decline", shown, { ...g, say: (m: string) => said.push(m) })).toEqual({ ok: true, said: "Declined 0123456789ab." });
  expect(g.calls.map((c) => c.split(" {")[0])).toEqual(["GET /approve/v1/decline/0123456789ab", "passkey 1,2,3 phobos.test required 9", "POST /approve/v1/decline/0123456789ab"]);
  // with no frame (nothing is sent), the h said is the intent's text hash
  expect(said).toEqual(["Declining for personal-assistant, #deadbeefcafe: “Route the plan.”. Your passkey asks next."]);
  // a challenge that does not say it is a decline (an approve's, or one that says nothing) is never signed as one
  for (const verdict of [undefined, "approve"]) {
    g = gateway(it, { verdict });
    expect(await sign(id, "decline", shown, g)).toEqual({ ok: false, said: "Not declined: the challenge is not for a decline." });
    expect(g.calls.length).toBe(1);
  }
  // changed since it was shown: nothing asked of the passkey
  g = gateway({ ...it, text: "something else" }, { verdict: "decline" });
  expect((await sign(id, "decline", shown, g)).changed).toBe(true);
  expect(g.calls.length).toBe(1);
});

test("sign: needs.json is read as the core's needs entry takes it, never stored and never waited on long", async () => {
  const calls: string[] = [];
  const fetch = (async (url: string, init?: RequestInit) => {
    calls.push(`${init?.method ?? "GET"} ${url} ${init?.cache} ${init?.signal ? "signal" : ""}`);
    if (url.endsWith("/needs.json")) return new Response(JSON.stringify({ as: "bobby", waiting: [] }));
    return new Response("not who this server acts for\n", { status: 403 });
  }) as unknown as typeof globalThis.fetch;
  expect(await readNeeds("https://phobos.test/fold/needs.json", { fetch })).toEqual({ needs: { as: "bobby", waiting: [] } });
  expect(calls[0]).toBe("GET https://phobos.test/fold/needs.json no-store signal");
  expect(await readNeeds("https://phobos.test/fold/forbidden.json", { fetch })).toEqual({ error: "403 not who this server acts for" });
  expect(await readNeeds("x", { fetch: (async () => { throw new TypeError("Load failed"); }) as any })).toEqual({ error: "Load failed" });
});

// Notifications (BRIEF addendum 2): Notify me asks the browser, then hands the subscription to fold serve's push/subscribe; Stop
// takes it off both. A browser without push (an iPhone page not on the Home Screen) is told how, and nothing is asked.
test("notify: this device subscribes with the server's key, and stops", async () => {
  const calls: string[] = [];
  let sub: any = null, perm = "granted";
  const reg = { pushManager: {
    getSubscription: async () => sub,
    subscribe: async (o: any) => { calls.push(`subscribe ${o.userVisibleOnly} ${new Uint8Array(o.applicationServerKey).join(",")}`); sub = { endpoint: "https://web.push.apple.com/x", toJSON: () => ({ endpoint: "https://web.push.apple.com/x", keys: { p256dh: "P", auth: "A" } }), unsubscribe: async () => { calls.push("browser unsubscribe"); sub = null; } }; return sub; },
  } };
  const env = {
    navigator: { serviceWorker: { ready: Promise.resolve(reg) } }, PushManager: function () {},
    Notification: { requestPermission: async () => { calls.push("permission"); return perm; } },
    fetch: (async (url: string, init?: RequestInit) => {
      calls.push(`${init?.method ?? "GET"} ${url}${init?.body ? " " + init.body : ""}`);
      return new Response(url.endsWith("/push/key") ? JSON.stringify({ key: enc(new Uint8Array([4, 1, 2]).buffer) }) : JSON.stringify({ ok: true }));
    }) as any,
  };
  const base = "https://phobos.test/fold/#/needs";
  expect(await notifyHere("subscribe", base, env)).toEqual({ ok: true, said: "This device will be notified when something waits for your signature." });
  expect(calls).toEqual(["permission", "GET https://phobos.test/fold/push/key", "subscribe true 4,1,2",
    'POST https://phobos.test/fold/push/subscribe {"endpoint":"https://web.push.apple.com/x","keys":{"p256dh":"P","auth":"A"}}']);
  calls.length = 0;
  expect((await notifyHere("unsubscribe", base, env)).said).toBe("This device won't be notified any more.");
  expect(calls).toEqual(['POST https://phobos.test/fold/push/unsubscribe {"endpoint":"https://web.push.apple.com/x"}', "browser unsubscribe"]);
  expect((await notifyHere("unsubscribe", base, env)).said).toBe("This device wasn't being notified.");
  // permission refused: nothing subscribed, nothing sent
  calls.length = 0; perm = "denied";
  expect((await notifyHere("subscribe", base, env)).ok).toBe(false);
  expect(calls).toEqual(["permission"]);
  // no push in this browser: told how, nothing asked
  calls.length = 0;
  expect((await notifyHere("subscribe", base, { ...env, PushManager: undefined } as any)).said).toContain("Home Screen");
  expect(calls).toEqual([]);
});

// A key chitin minted again (vapid-self-mint addendum 1.3): a subscription made with key A while the server's key is B is never reused.
// It is taken off (server, then browser), a new one is made with B, and that one is posted. On page load the same happens with no prompt,
// once permission is already granted; with the same key, or no subscription, nothing is fetched beyond the key and nothing is said.
test("notify: a subscription under an old key is replaced, on Notify me and on page load", async () => {
  const A = new Uint8Array([4, 1, 1]), B = new Uint8Array([4, 2, 2]);
  const calls: string[] = [];
  const made = (key: Uint8Array, endpoint: string) => ({
    endpoint, options: { applicationServerKey: key.buffer },
    toJSON: () => ({ endpoint, keys: { p256dh: "P", auth: "A" } }),
    unsubscribe: async () => { calls.push(`browser unsubscribe ${endpoint}`); sub = null; },
  });
  let sub: any = null, serverKey = B;
  const reg = { pushManager: {
    getSubscription: async () => sub,
    subscribe: async (o: any) => { const k = new Uint8Array(o.applicationServerKey); calls.push(`subscribe ${k.join(",")}`); sub = made(k, `https://web.push.apple.com/${k.join("")}`); return sub; },
  } };
  const env = {
    navigator: { serviceWorker: { ready: Promise.resolve(reg) } }, PushManager: function () {},
    Notification: { permission: "granted", requestPermission: async () => { calls.push("permission"); return "granted"; } },
    fetch: (async (url: string, init?: RequestInit) => {
      calls.push(`${init?.method ?? "GET"} ${url}${init?.body ? " " + init.body : ""}`);
      return new Response(url.endsWith("/push/key") ? JSON.stringify({ key: enc(serverKey.buffer) }) : JSON.stringify({ ok: true }));
    }) as any,
  };
  const base = "https://phobos.test/fold/";
  // Notify me, holding A while the server's key is B
  sub = made(A, "https://web.push.apple.com/old");
  expect((await notifyHere("subscribe", base, env)).ok).toBe(true);
  expect(calls).toEqual(["permission", "GET https://phobos.test/fold/push/key",
    'POST https://phobos.test/fold/push/unsubscribe {"endpoint":"https://web.push.apple.com/old"}', "browser unsubscribe https://web.push.apple.com/old",
    "subscribe 4,2,2", 'POST https://phobos.test/fold/push/subscribe {"endpoint":"https://web.push.apple.com/422","keys":{"p256dh":"P","auth":"A"}}']);
  // Notify me again with the same key: the subscription is reused, only posted again
  calls.length = 0;
  expect((await notifyHere("subscribe", base, env)).ok).toBe(true);
  expect(calls).toEqual(["permission", "GET https://phobos.test/fold/push/key", 'POST https://phobos.test/fold/push/subscribe {"endpoint":"https://web.push.apple.com/422","keys":{"p256dh":"P","auth":"A"}}']);
  // page load, same key: nothing to do, nothing said
  calls.length = 0;
  expect(await notifyRefresh(base, env)).toBe(null);
  expect(calls).toEqual(["GET https://phobos.test/fold/push/key"]);
  // page load after chitin's key changed (B -> A): replaced with no prompt
  calls.length = 0; serverKey = A;
  expect((await notifyRefresh(base, env))?.ok).toBe(true);
  expect(calls).toEqual(["GET https://phobos.test/fold/push/key",
    'POST https://phobos.test/fold/push/unsubscribe {"endpoint":"https://web.push.apple.com/422"}', "browser unsubscribe https://web.push.apple.com/422",
    "subscribe 4,1,1", 'POST https://phobos.test/fold/push/subscribe {"endpoint":"https://web.push.apple.com/411","keys":{"p256dh":"P","auth":"A"}}']);
  // page load without permission, or with no subscription: nothing fetched, nothing asked
  calls.length = 0;
  expect(await notifyRefresh(base, { ...env, Notification: { permission: "default", requestPermission: env.Notification.requestPermission } })).toBe(null);
  sub = null;
  expect(await notifyRefresh(base, env)).toBe(null);
  expect(calls).toEqual([]);
});

/** sw.js run in a fake worker: its listeners, and what they did. */
function worker() {
  const on: Record<string, (e: any) => void> = {}, did: any[] = [];
  const self: any = { location: new URL("https://phobos.test/fold/sw.js"), addEventListener: (k: string, f: any) => { on[k] = f; },
    registration: { showNotification: async (t: string, o: any) => { did.push(["show", t, o]); } }, clients: { claim: async () => {} }, skipWaiting: () => {} };
  new Function("self", "caches", "fetch", readFileSync(join(import.meta.dir, "sw.js"), "utf8").replace("@VERSION@", "t"))(self, {}, () => {});
  const fire = async (k: string, e: any) => { let w: Promise<unknown> = Promise.resolve(); on[k]({ ...e, waitUntil: (p: Promise<unknown>) => { w = p; } }); await w; };
  return { self, did, fire };
}

test("sw: a push shows the item's one line, tagged by it, and a tap opens its fold in the viewer", async () => {
  const w = worker(), push = (d: unknown) => w.fire("push", { data: { json: () => d } });
  await push({ text: "To personal-assistant", path: "/fold/needs/approvals/needs-sign-0123456789ab/" });
  await push({ title: "To x", url: "/fold/../../etc/", tag: "t1" });
  await push({ text: "", path: "https://evil.test/fold/x/" });
  await w.fire("push", { data: { json: () => { throw new Error("not JSON"); } } });
  expect(w.did).toEqual([
    ["show", "To personal-assistant", { tag: "https://phobos.test/fold/#/needs/approvals/needs-sign-0123456789ab", data: { url: "https://phobos.test/fold/#/needs/approvals/needs-sign-0123456789ab" } }],
    ["show", "To x", { tag: "t1", data: { url: "https://phobos.test/fold/#/needs" } }],
    ["show", "Something needs you", { tag: "https://phobos.test/fold/#/needs", data: { url: "https://phobos.test/fold/#/needs" } }],
    ["show", "Something needs you", { tag: "https://phobos.test/fold/#/needs", data: { url: "https://phobos.test/fold/#/needs" } }],
  ]);
  // no sound, vibration or renotify of its own
  for (const [, , o] of w.did) expect(Object.keys(o).sort()).toEqual(["data", "tag"]);
  // a tap: an open window of this app goes there; with none, one opens there
  const opened: string[] = [], url = "https://phobos.test/fold/#/needs/approvals/needs-sign-0123456789ab";
  const note = { close: () => opened.push("closed"), data: { url } };
  w.self.clients.matchAll = async () => [{ url: "https://phobos.test/fold/#/", focus: async () => opened.push("focus"), navigate: async (u: string) => opened.push(`navigate ${u}`) }];
  await w.fire("notificationclick", { notification: note });
  w.self.clients.matchAll = async () => [];
  w.self.clients.openWindow = async (u: string) => opened.push(`open ${u}`);
  await w.fire("notificationclick", { notification: note });
  // a window this worker doesn't control: navigate refuses, so one opens instead
  w.self.clients.matchAll = async () => [{ url: "https://phobos.test/fold/#/", focus: async () => opened.push("focus"), navigate: async () => { throw new TypeError("not controlled"); } }];
  await w.fire("notificationclick", { notification: note });
  expect(opened).toEqual(["closed", "focus", `navigate ${url}`, "closed", `open ${url}`, "closed", "focus", `open ${url}`]);
});

/** fold serve's nodes/ and its nodes, as src/serve.rs answers them: a strong ETag of the bytes, 304 on If-None-Match. Every request
 * is recorded with the answer's status. */
function nodeHost(base: string) {
  const files = new Map<string, string>(), asked: string[] = [];
  const etag = (t: string) => `"sha-256:${new Bun.CryptoHasher("sha256").update(t).digest("hex")}"`;
  const index = () => JSON.stringify({ nodes: [...files].sort(([a], [b]) => (a < b ? -1 : 1)).map(([u, t]) => ({ id: JSON.parse(t).id, href: u.slice(base.length), etag: etag(t) })) });
  const fetch = async (url: string, o: { cache?: string; headers?: Record<string, string> } = {}) => {
    expect(o.cache).toBe("no-store");
    const t = url === base ? index() : files.get(url);
    if (t == null) { asked.push(`404 ${url}`); return { ok: false, status: 404, statusText: "Not Found" }; }
    if (o.headers?.["If-None-Match"] === etag(t)) { asked.push(`304 ${url}`); return { ok: false, status: 304 }; }
    asked.push(`200 ${url}`);
    return { ok: true, status: 200, headers: new Headers({ etag: etag(t) }), json: async () => JSON.parse(t) };
  };
  const put = (id: string, node: object) => files.set(`${base}${id}.json`, JSON.stringify({ id, ...node }));
  return { fetch: fetch as unknown as typeof globalThis.fetch, put, asked, etag };
}

test("live folds: a node is revalidated by its ETag, a 304 is nothing, a 200 is that node alone", async () => {
  const base = "https://phobos.test/fold-data/fleet/nodes/", host = nodeHost(base);
  host.put("needs", { source: "chitin pending", updated: "2026-11-03T18:00:00Z", ok: true, cadence_s: 30, data: { title: "Needs you", items: [{ id: "a", title: "Sign A" }] } });
  host.put("desk", { source: "desk.sh", updated: "2026-11-03T17:00:00Z", ok: true, data: { title: "Desk" } });
  const r = await revalidate(`${base}needs.json`, null, { fetch: host.fetch });
  expect(r.status).toBe(200);
  expect(await revalidate(`${base}needs.json`, r.etag, { fetch: host.fetch })).toEqual({ status: 304 });
  const timers: [() => void, number][] = [], seen: string[][] = [];
  const live = liveNodes(base, { fetch: host.fetch, later: (f, ms) => timers.push([f, ms]), changed: (ids) => seen.push(ids) });
  host.asked.length = 0;
  expect(await live.check()).toEqual(["desk", "needs"]);
  expect(host.asked).toEqual([`200 ${base}`, `200 ${base}desk.json`, `200 ${base}needs.json`]);
  // each node on its own cadence: needs its 30 s, desk the 60 s default
  expect(timers.map(([, ms]) => ms)).toEqual([60_000, 30_000]);
  // nothing changed: the index alone, a 304, and no change said
  host.asked.length = 0; seen.length = 0;
  expect(await live.check()).toEqual([]);
  expect(host.asked).toEqual([`304 ${base}`]);
  expect(seen).toEqual([]);
  // needs changed: the index, then needs alone; desk is not asked
  host.put("needs", { source: "chitin pending", updated: "2026-11-03T18:01:00Z", ok: true, cadence_s: 30, data: { title: "Needs you", items: [{ id: "b", title: "Sign B" }] } });
  host.asked.length = 0;
  expect(await live.check()).toEqual(["needs"]);
  expect(host.asked).toEqual([`200 ${base}`, `200 ${base}needs.json`]);
  expect(seen).toEqual([["needs"]]);
  // its own cadence: a 304, and no change said; a failed read keeps the last good node
  host.asked.length = 0; seen.length = 0;
  await timers[1][0]();
  expect(host.asked).toEqual([`304 ${base}needs.json`]);
  expect(seen).toEqual([]);
  expect(live.nodes().find((n: any) => n.id === "needs").data.items[0].id).toBe("b");
});

test("live folds: a 304 leaves the DOM untouched; a 200 replaces only the fold that shows that node", async () => {
  const base = "https://phobos.test/fold-data/fleet/nodes/", host = nodeHost(base);
  const root = { title: "Fleet", sections: [{ id: "needs", title: "Needs you", items: [{ id: "old", title: "From page.json" }] }, { id: "desk", title: "Desk", items: [{ id: "x", title: "Desk row" }] }] };
  const doc_ = origin("live", { root });
  host.put("needs", { source: "chitin pending --as bobby", updated: "2026-11-03T18:00:00Z", ok: true, data: { title: "Needs you", items: [{ id: "a", title: "Sign A" }] } });
  const opts = () => ({ source: "fleet-map", updated: "2026-11-03T12:00:00Z", localTimeZone: "UTC" });
  const live = liveNodes(base, { fetch: host.fetch, later: () => {} });
  await live.check();
  const folded = async () => stamp(await open(fold, doc_.url, { fetch: doc_.fetch, root: (r: unknown) => graft(r, live.nodes()), opts }), live.nodes());
  // the viewer's render: the fold at `hash` drawn again only when the change shows there (index.html's `changed`)
  let nodes = await folded();
  const drawn: Record<string, El> = {};
  const draw = (hash: string) => (drawn[hash] = toDom(doc, viewOf(ax, nodes, hash, "UTC").tree) as El);
  const text = (e: El) => all(e).map((x) => x.textContent).filter(Boolean);
  for (const h of ["#/needs", "#/desk", "#/"]) draw(h);
  // the node's data is the fold, and the fold says the node's source and time, not the document's
  expect(text(drawn["#/needs"])).toContain("Sign A");
  expect(text(drawn["#/needs"])).not.toContain("From page.json");
  expect(text(drawn["#/needs"]).some((t) => t.startsWith("Data from chitin pending --as bobby, "))).toBe(true);
  expect(text(drawn["#/desk"]).some((t) => t.startsWith("Data from fleet-map, "))).toBe(true);
  const before = { ...drawn }, deskText = JSON.stringify(text(drawn["#/desk"]));
  const apply = async (ids: string[]) => {
    if (!ids.length) return;
    nodes = await folded();
    for (const h of Object.keys(drawn)) if (ids.some((id) => shows(id, pathOf(h)))) draw(h);
  };
  // unchanged: a 304, nothing re-folded, every fold's DOM the same object
  await apply(await live.check());
  for (const h of Object.keys(drawn)) expect(drawn[h]).toBe(before[h]);
  // needs changed: its fold and its parent's (the row that names it) are drawn again; desk's is the same object, the same words
  host.put("needs", { source: "chitin pending --as bobby", updated: "2026-11-03T18:05:00Z", ok: false, stale_since: "2026-11-03T18:04:00Z", data: { title: "Needs you", items: [{ id: "b", title: "Sign B" }] } });
  await apply(await live.check());
  expect(drawn["#/desk"]).toBe(before["#/desk"]);
  expect(JSON.stringify(text(drawn["#/desk"]))).toBe(deskText);
  expect(drawn["#/needs"]).not.toBe(before["#/needs"]);
  expect(text(drawn["#/needs"])).toContain("Sign B");
  expect(text(drawn["#/needs"])).toContain("Stale since 2026-11-03T18:04:00Z");
  expect(drawn["#/"]).not.toBe(before["#/"]);
});

test("live folds: a node goes where its id says, and only there", () => {
  const root = { title: "Fleet", sections: [{ id: "fleet", title: "Fleet", items: [{ id: "inbox", title: "Old" }, { id: "other", title: "Other" }] }] };
  const out = graft(root, [{ id: "fleet/inbox", data: { title: "Inbox" } }, { id: "new", data: { title: "New" } }, { id: "gone/x", data: { title: "X" } }]) as any;
  expect(out.sections[0].items).toEqual([{ title: "Inbox", id: "inbox" }, { id: "other", title: "Other" }]);
  expect(out.sections[1]).toEqual({ title: "New", id: "new" });
  expect(out.sections.length).toBe(2);
  expect(root.sections[0].items[0].title).toBe("Old"); // the document itself is not changed
  expect([shows("fleet/inbox", ["fleet", "inbox"]), shows("fleet/inbox", ["fleet"]), shows("fleet/inbox", []), shows("fleet/inbox", ["fleet", "other"])]).toEqual([true, true, false, false]);
});

/** fold serve's per-fold JSON-LD files (src/serve.rs: strong ETag of the bytes, 304 on If-None-Match), each request recorded. A
 * `hold(url)` makes that file's next answer wait until `release()`. */
function foldHost(base: string) {
  const files = new Map<string, string>(), asked: string[] = [], holds = new Map<string, Promise<void>>();
  const etag = (t: string) => `"sha-256:${new Bun.CryptoHasher("sha256").update(t).digest("hex")}"`;
  const fetch = async (url: string, o: { cache?: string; headers?: Record<string, string> } = {}) => {
    expect(o.cache).toBe("no-store");
    const u = url.split("?")[0];
    if (holds.has(u)) await holds.get(u);
    const t = files.get(u);
    if (t == null) { asked.push(`404 ${u}`); return { ok: false, status: 404, statusText: "Not Found" }; }
    if (o.headers?.["If-None-Match"] === etag(t)) { asked.push(`304 ${u}`); return { ok: false, status: 304 }; }
    asked.push(`200 ${u}`);
    return { ok: true, status: 200, headers: new Headers({ etag: etag(t) }), json: async () => JSON.parse(t) };
  };
  const put = (id: string, doc: object) => files.set(`${base}${id}.jsonld`, JSON.stringify({ "@id": `fold:fleet/${id}`, id, ...doc }));
  const ref = (id: string) => ({ "@id": `fold:fleet/${id}`, "@type": "FoldRef", url: `${id}.jsonld`, etag: files.has(`${base}${id}.jsonld`) ? etag(files.get(`${base}${id}.jsonld`)!) : null, updated: null });
  const hold = (id: string) => { let release = () => {}; holds.set(`${base}${id}.jsonld`, new Promise<void>((r) => { release = () => { holds.delete(`${base}${id}.jsonld`); r(); }; })); return release; };
  return { fetch: fetch as unknown as typeof globalThis.fetch, put, ref, hold, asked };
}
const made = (query: string) => ({ query, version: 1, href: `/fold-data/fleet/${query}.cypher` });

test("folds by reference: each part read on its own, a ref's ETag spares a read, a slow part never holds the rest", async () => {
  const base = "https://phobos.test/fold-data/fleet/", host = foldHost(base), top = `${base}everything.jsonld`;
  host.put("needs", { "@type": "Fold", title: "Needs you", source: "chitin pending --as bobby", updated: "2026-11-03T18:00:00Z", ok: true, madeBy: made("needs"), data: [{ id: "a", title: "Sign A" }] });
  host.put("inbox", { "@type": "Fold", title: "Messages to you", source: "chitin inbox --as bobby", updated: "2026-11-03T18:00:00Z", ok: true, madeBy: made("inbox"), data: [{ id: "m1", title: "Hello" }] });
  host.put("everything", { "@type": "FoldManifest", title: "Everything", parts: [host.ref("needs"), host.ref("inbox")] });
  const seen: string[][] = [];
  const live = liveFolds(top, { fetch: host.fetch, changed: (k) => seen.push(k) });
  // inbox is slow: needs lands, and is said changed, while inbox is still being read
  const release = host.hold("inbox");
  const first = live.check();
  await new Promise((r) => setTimeout(r, 10));
  expect(seen).toEqual([["needs"]]);
  release();
  expect((await first).sort()).toEqual(["everything", "inbox", "needs"]);
  expect(seen).toEqual([["needs"], ["inbox"]]);
  // nothing changed: the manifest alone, a 304; its refs name the ETags held, so no part is asked
  host.asked.length = 0; seen.length = 0;
  expect(await live.check()).toEqual([]);
  expect(host.asked).toEqual([`304 ${top}`]);
  // on focus (deep), each part is asked too: a 304 each, still no change
  host.asked.length = 0;
  expect(await live.check({ deep: true })).toEqual([]);
  expect(host.asked.sort()).toEqual([`304 ${top}`, `304 ${base}inbox.jsonld`, `304 ${base}needs.jsonld`]);
  // inbox changed (renamed over, the manifest rewritten with its new ETag): the manifest and inbox alone are read
  host.put("inbox", { "@type": "Fold", title: "Messages to you", source: "chitin inbox --as bobby", updated: "2026-11-03T18:02:00Z", ok: true, madeBy: made("inbox"), data: [{ id: "m2", title: "Later" }] });
  host.put("everything", { "@type": "FoldManifest", title: "Everything", parts: [host.ref("needs"), host.ref("inbox")] });
  host.asked.length = 0; seen.length = 0;
  expect((await live.check()).sort()).toEqual(["everything", "inbox"]);
  expect(host.asked).toEqual([`200 ${top}`, `200 ${base}inbox.jsonld`]);
  expect(seen).toEqual([["inbox"]]);
});

test("folds by reference: placed where page.json has them, drawn with 'Made by', and a fold no query made is not drawn", async () => {
  const base = "https://phobos.test/fold-data/fleet/", host = foldHost(base), top = `${base}everything.jsonld`;
  host.put("needs", { "@type": "Fold", title: "Needs you", source: "chitin pending --as bobby", updated: "2026-11-03T18:00:00Z", ok: true, madeBy: made("needs"), data: [{ id: "a", title: "Sign A" }] });
  host.put("inbox", { "@type": "Fold", title: "Messages to you", source: "chitin inbox", updated: "2026-11-03T18:00:00Z", ok: true, data: [{ id: "m1", title: "Unproven" }] }); // no madeBy
  host.put("loop", { "@type": "FoldManifest", title: "Loop", parts: [{ "@id": "fold:fleet/everything", "@type": "FoldRef", url: "everything.jsonld", etag: null, updated: null }] });
  host.put("everything", { "@type": "FoldManifest", title: "Everything", parts: [host.ref("needs"), host.ref("inbox"), host.ref("gone"), host.ref("loop")] });
  const live = liveFolds(top, { fetch: host.fetch });
  await live.check();
  // page.json as it is today: Needs you at the top, Messages to you under Agent fleet
  const root = { title: "Fleet", sections: [
    { id: "needs", title: "Needs you", purpose: "What the agents need from you.", groups: [{ title: "", items: [{ id: "old", title: "Scraped row" }] }] },
    { id: "fleet", title: "Agent fleet", sections: [{ id: "inbox", title: "Messages to you", items: [{ id: "s", title: "Scraped message" }] }] },
  ] };
  expect(placeIn(root, "inbox")).toEqual(["fleet", "inbox"]);
  const placed = foldNodes(root, live.folds());
  expect(placed.map((n: any) => n.id).sort()).toEqual(["fleet/inbox", "gone", "loop", "loop/everything", "needs"]);
  const doc_ = origin("refs", { root: graft(root, placed) });
  const nodes = stamp(await open(fold, doc_.url, { fetch: doc_.fetch, opts: () => ({ source: "fleet-map", updated: "2026-11-03T12:00:00Z", localTimeZone: "UTC" }) }), placed);
  const text = (h: string) => all(toDom(doc, viewOf(ax, nodes, h, "UTC").tree) as El).map((x) => x.textContent).filter(Boolean);
  const links = (h: string) => all(toDom(doc, viewOf(ax, nodes, h, "UTC").tree) as El).filter((x) => x.tag === "a").map((x) => [x.textContent, x.href ?? x.attrs.href]);
  // needs: the query's rows, not page.json's; its purpose kept; its source and the query that made it said, linked to the query text
  expect(text("#/needs")).toContain("Sign A");
  expect(text("#/needs")).not.toContain("Scraped row");
  expect(text("#/needs")).toContain("What the agents need from you.");
  expect(text("#/needs").some((t) => t.startsWith("Data from chitin pending --as bobby, "))).toBe(true);
  expect(links("#/needs")).toContainEqual(["Made by needs, version 1", "/fold-data/fleet/needs.cypher"]);
  // inbox has no derivation: not drawn at all, neither its rows nor page.json's, and the slot says why
  expect(text("#/fleet/inbox")).not.toContain("Unproven");
  expect(text("#/fleet/inbox")).not.toContain("Scraped message");
  expect(text("#/fleet/inbox").some((t) => t.startsWith("Not shown: it names no query that made it"))).toBe(true);
  // a missing part and a cycle: a notice in their own slots, never a crash
  expect(text("#/gone").some((t) => t.startsWith("Not read: 404"))).toBe(true);
  expect(text("#/loop/everything").some((t) => t.startsWith("Not followed: it refers back to itself"))).toBe(true);
  // the derivation rule on its own
  expect(underived({ madeBy: made("needs"), updated: "2026-11-03T18:00:00Z" })).toBeNull();
  expect(underived({ madeBy: { query: "needs", href: "x" }, updated: "2026-11-03T18:00:00Z" })).toBe("its query needs has no version");
  expect(underived({ madeBy: { ...made("needs"), frame: { id: "bobby" } }, updated: "2026-11-03T18:00:00Z" })).toBe("its frame has no id and version");
  expect(underived({ "@type": "FoldManifest" })).toBeNull();
  // a manifest opened as the document: one placeholder per part, in its order, until each lands
  expect(manifestRoot({ title: "Everything", parts: [host.ref("needs"), host.ref("inbox")] }).sections.map((s: any) => [s.id, s.text])).toEqual([["needs", "Not read yet."], ["inbox", "Not read yet."]]);
});

// The rotor (rotor.js, BRIEF-stint-2026-11-03-basis-rotor): Allie's kinds and keys over a fixture page shaped like the fleet page.
const rotorPage = async () => {
  const input = read(import.meta.dir, "rotor.fixture.json"), o = origin("rotor", input);
  return open(fold, o.url, { fetch: o.fetch, opts: () => input.opts });
};
test("rotor: each kind holds its folds from the whole page, in reading order", async () => {
  const nodes = await rotorPage();
  const titles = (k: string, under: string[] = []) => kindItems(nodes, k, under).map((x: any) => x.node.title);
  expect(KINDS.map((k) => k.name)).toEqual(["needs", "actions", "sections", "stints", "landings", "messages", "links"]);
  // An item with no name is labelled by its action (src/lib.rs LABEL_KEYS), as on the page itself (the live fleet page's asks have
  // no name): the rotor says each item as the page does.
  expect(titles("needs")).toEqual(["To a seat", "Answer in chitin.", "Merge it."]);
  expect(titles("actions")).toEqual(["To a seat", "work-basis-rotor-stint"]);
  expect(titles("sections")).toEqual(["Needs you", "Goal sessions", "Messages to you"]);
  expect(titles("stints")).toEqual(["work-basis-rotor-stint"]);
  expect(titles("landings")).toEqual(["Merge it."]);
  expect(titles("messages")).toEqual(["claude-break-glass", "allie-lead"]);
  expect(titles("links")).toEqual(["Merge it."]);
  // from the fold you are on: only what is under it, never the fold itself
  expect(titles("actions", ["needs"])).toEqual(["To a seat"]);
  expect(titles("sections", ["needs"])).toEqual([]);
  expect(titles("nonsense")).toEqual([]);
});
test("rotor: a kind is drawn as a fold, through the core's ax and the AX gate, each row going to its item's fold", async () => {
  const nodes = await rotorPage();
  const els = (n: unknown) => all(toDom(doc, ax({ node: n, path: [], mode: "folds", href: "viewer", localTimeZone: "UTC" })) as El);
  const v = els(kindView(nodes, "actions"));
  expect(v.filter((e) => e.tag === "h1").map((e) => e.textContent)).toEqual(["Actions"]);
  const rows = v.filter((e) => e.tag === "li" && e.className === "obj");
  expect(rows.length).toBe(2);
  const links = v.filter((e) => e.tag === "a" && String(e.href).startsWith("#/")).map((e) => [e.textContent, e.href]);
  for (const { path } of kindItems(nodes, "actions")) expect(links.map(([, h]) => pathOf(String(h)))).toContainEqual(path);
  // each item says where it is (its trail under the document's title), read after its name
  expect(v.filter((e) => e.tag === "span" && e.className === "hint").map((e) => e.textContent)).toEqual(["Needs you", "Goal sessions"]);
  // none here: a line says so, and a row goes back to where you were
  const none = kindView(nodes, "sections", ["needs"], { href: "#/needs", title: "Needs you" });
  expect(none.text).toEqual(["No sections here."]);
  expect(none.kids).toEqual([{ id: "back", title: "Back to Needs you", href: "#/needs" }]);
  expect(els(none).filter((e) => e.tag === "h1").map((e) => e.textContent)).toEqual(["Sections"]);
});
test("rotor: Allie's keys (r, the drawer's j k [ ] Enter digits r r Esc, a kind's ] [ h), and what each says", () => {
  const from = { hash: "#/needs", path: ["needs"], cur: 2, title: "Needs you" };
  let s = rotorStart();
  // off the rotor, only r is taken: j k l h Enter ] [ stay the viewer's
  for (const k of ["j", "k", "l", "h", "Enter", "]", "[", "Escape"]) expect(rotorKey(s, k, from)).toBeNull();
  let r = rotorKey(s, "r", from)!;
  expect([r.state.drawer, r.state.at, r.fx.drawer]).toEqual([true, 0, "open"]);
  // the drawer stops at either end and says so, never wraps
  expect(rotorKey(r.state, "k", from)!.fx).toEqual({ say: "First kind" });
  expect(rotorKey(r.state, "[", from)!.fx).toEqual({ say: "First kind" });
  r = rotorKey(r.state, "j", from)!; expect([r.state.at, r.fx.drawer]).toEqual([1, "draw"]);
  r = rotorKey(r.state, "ArrowDown", from)!; r = rotorKey(r.state, "]", from)!; expect(r.state.at).toBe(3);
  // anything else is swallowed while it is open; Tab still leaves it
  expect(rotorKey(r.state, "x", from)!.fx).toEqual({});
  expect(rotorKey(r.state, "Tab", from)).toBeNull();
  // Enter picks: the kind's word said once, where you came from kept
  r = rotorKey(r.state, "Enter", from)!;
  expect([r.state.drawer, r.state.kind, r.fx.view, r.fx.say, r.fx.drawer]).toEqual([false, "stints", "stints", "Stints", "close"]);
  expect(r.state.from).toEqual(from);
  s = r.state;
  // in a kind, j k l Enter are the viewer's (they move between its items and go to one)
  for (const k of ["j", "k", "l", "Enter", "ArrowDown"]) expect(rotorKey(s, k, from)).toBeNull();
  // ] [ the next and previous kind, keeping where you came from
  r = rotorKey(s, "]", { ...from, hash: "#/x" })!;
  expect([r.state.kind, r.fx.view, r.fx.say, r.state.from]).toEqual(["landings", "landings", "Landings", from]);
  r = rotorKey(r.state, "[", from)!; r = rotorKey(r.state, "[", from)!; r = rotorKey(r.state, "[", from)!; r = rotorKey(r.state, "[", from)!;
  expect(r.state.kind).toBe("needs");
  expect(rotorKey(r.state, "[", from)!.fx).toEqual({ say: "First kind" });
  // h (or Esc ← Backspace) leaves, back to the fold and row it was entered from
  for (const k of ["h", "Escape", "ArrowLeft", "Backspace"]) {
    const out = rotorKey(r.state, k, from)!;
    expect([out.state.kind, out.fx.leave]).toEqual(["", from]);
  }
  // r in a kind opens the drawer on that kind; r r picks the last kind; a digit picks its kind; picking the kind you are in only closes
  let d = rotorKey(r.state, "r", from)!; expect([d.state.drawer, d.state.at]).toEqual([true, 0]);
  expect(rotorKey(d.state, "r", from)!.fx).toEqual({ drawer: "close", say: "Needs" });
  expect(rotorKey(d.state, "7", from)!.fx).toEqual({ drawer: "close", view: "links", say: "Links" });
  expect(rotorKey(d.state, "9", from)!.fx).toEqual({});
  expect(rotorKey(d.state, "Escape", from)!.fx).toEqual({ drawer: "close", say: "Needs" });
  d = rotorKey(rotorStart(), "r", from)!;
  r = rotorKey(d.state, "2", from)!; expect(r.state.last).toBe("actions");
  r = rotorKey(rotorKey({ ...r.state, kind: "" }, "r", from)!.state, "r", from)!;
  expect(r.fx.view).toBe("actions");
});
test("rotor: no layout shift: the drawer is fixed out of the page's flow, outside the fold, and speaks in the notices zone", () => {
  const html = readFileSync(join(import.meta.dir, "index.html"), "utf8"), css = readFileSync(join(import.meta.dir, "fold.css"), "utf8");
  expect(css).toMatch(/section\.rotor\{position:fixed;/);
  expect(html).toContain('<section id="rotor" class="rotor" aria-label="Kinds" hidden><ol id="rotor-kinds" role="listbox"');
  expect(html.indexOf('id="rotor"')).toBeGreaterThan(html.indexOf("</main>"));
  // its words go through say(), the notices zone's one live region (#notice, role=status), never a region of its own
  expect(html).toContain("if (fx.say) say(fx.say);");
  expect((html.match(/aria-live|role="status"/g) ?? []).length).toBe(1);
});
