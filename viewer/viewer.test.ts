// The viewer's own path on every golden, with the built module (FOLD_CORE_WASM): the document fetched from a data URL and folded here
// gives the golden folds; every location's tree is the golden one (the "viewer" links, read back by pathOf); every fold it can show
// derives a DOM without a refusal (fold-dom.js toDom: the AX gate); and the artificial viewer's walk over what it shows is the golden
// walk (the load gate: view.words and the rest, blocking where the golden blocks).
import { expect, test } from "bun:test";
import { readdirSync, readFileSync } from "node:fs";
import { join } from "node:path";
import { core } from "../js/fold-core.ts";
import { toDom } from "./fold-dom.js";
import { at, beside, dataOf, haecceityOf, href, open, pathOf, segmentOf, since, viewOf } from "./view.js";
import { ENDPOINTS, dec, enc, readNeeds, sign } from "./sign.js";
import { aboutOf, contextOf, embed, flagUnnamed, pinOf, reportToken } from "./clax.js";

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
    ["Approve with passkey", "get", "", "0123456789ab", "approve"],
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
  expect(said).toEqual(["Approving for personal-assistant, #f00dfeed: “Route the plan.”. Your passkey asks next."]);
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
  expect(await sign(id, "decline", shown, { ...g, say: (m: string) => said.push(m) })).toEqual({ ok: true, said: "Declined. written m-9" });
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
