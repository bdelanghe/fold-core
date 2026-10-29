// Records what the fold viewer fold-core was ported from does, as goldens (step 0 of the one-core plan).
//
//   FOLD_SRC=<a home source tree> FOLD_REV=<its rev> bun goldens/gen.ts
//
// Folds: each goldens/fixtures/<case>.json ({root, files, opts, through?}) is synthetic. This writes <case>.input.json ({root,
// files, opts}, root after `through`, if any) and <case>.folds.json: fold(withSources(root, files), opts) as [id, node] pairs in
// Map insertion order.
// Walks (walk.ts): every fold case's folds, and each goldens/walk/fixtures/<case>.json ({nodes: {id: node} or [[id, node]],
// facto?}), give goldens/walk/<case>.input.json ({nodes, opts, facto?}) and <case>.walk.json ({walk, verdict, facto?}), as the
// core's walk_json returns them.
// AX: every walk case's folds through fold-ax.js axOf give goldens/ax/<case>.ax.json ([id, mode, href, tree] rows); the input is the
// walk case's nodes, as the core's ax_json takes them one at a time ({node, path, mode, href, localTimeZone}).
// The machine's zone: each case runs with TZ set to its opts.localTimeZone (UTC when it names none, and then recorded as UTC for
// walks), so fold.ts and walk.ts read times where the core is told to.
import { mkdirSync, readdirSync, readFileSync, writeFileSync } from "node:fs";
import { join } from "node:path";

const SRC = process.env.FOLD_SRC, REV = process.env.FOLD_REV;
if (!SRC || !REV) { console.error("usage: FOLD_SRC=<dir> FOLD_REV=<rev> bun goldens/gen.ts"); process.exit(2); }
const { fold, withSources } = await import(join(SRC, "programs/fold-viewer/fold.ts"));
const { walk, verdict, factoBody } = await import(join(SRC, "programs/fold-viewer/walk.ts"));
const { axOf } = await import(join(SRC, "programs/fold-viewer/fold-ax.js"));
// The two links axOf is given: the walk's (walk.ts, load.ts, coldread.ts) and the viewer's (viewer.html, ids encoded).
const HREFS: Record<string, (p: string[], m: string) => string> = {
  walk: (p, m) => (m === "slides" ? "#slides/" : "#/") + p.join("/"),
  viewer: (p, m) => (m === "slides" ? "#slides/" : "#/") + p.map(encodeURIComponent).join("/"),
};
const { everything } = await import(join(SRC, "programs/fleet-map/everything.ts"));
const through: Record<string, (p: any) => any> = { everything };
const zone = (z?: string) => { process.env.TZ = z ?? "UTC"; return Intl.DateTimeFormat().resolvedOptions().timeZone; };
const budgets = JSON.parse(readFileSync(process.env.WALK_BUDGETS ?? join(SRC, "programs/fleet-map/budgets.json"), "utf8")).budgets
  .filter((b: any) => b.id === "view.words").map(({ id, value }: any) => ({ id, value }));
const json = (v: unknown) => JSON.stringify(v, null, 2) + "\n";

const OUT = import.meta.dir, WALK = join(OUT, "walk"), AX = join(OUT, "ax");
mkdirSync(WALK, { recursive: true });
mkdirSync(AX, { recursive: true });
const walked = (name: string, pairs: [string, any][], facto?: { artifact: string; at: string; rev: string }, tz?: string) => {
  const localTimeZone = zone(tz);
  const r = walk(new Map(structuredClone(pairs)));
  writeFileSync(join(WALK, `${name}.input.json`), json({ nodes: pairs, opts: { budgets, localTimeZone }, ...(facto ? { facto } : {}) }));
  writeFileSync(join(WALK, `${name}.walk.json`), json({ walk: r, verdict: verdict(r), ...(facto ? { facto: factoBody(facto.artifact, facto.at, facto.rev, r) } : {}) }));
  console.log(`walk ${name}: ${r.reached}/${r.folds} reached, ${r.findings.length} findings`);
  // AX (fold-ax.js axOf) of every fold in the walk case's nodes, as folds and (when it presents) as slides, with either link, and
  // of a fold with no data: ax/<name>.ax.json, one [id, mode, href, tree] a line.
  const nodes = new Map(structuredClone(pairs));
  const rows: string[] = [];
  for (const id of [...nodes.keys(), "no-such-fold"]) {
    const n = nodes.get(id), path = id === "root" ? [] : id.split("~");
    for (const mode of n?.present ? ["folds", "slides"] : ["folds"])
      for (const [h, href] of Object.entries(HREFS)) rows.push(JSON.stringify([id, mode, h, axOf(n, path, href, mode)]));
  }
  writeFileSync(join(AX, `${name}.ax.json`), "[\n" + rows.join(",\n") + "\n]\n");
};

for (const f of readdirSync(join(OUT, "fixtures")).filter((f) => f.endsWith(".json")).sort()) {
  const name = f.replace(/\.json$/, "");
  const fx = JSON.parse(readFileSync(join(OUT, "fixtures", f), "utf8"));
  const root = fx.through ? through[fx.through](structuredClone(fx.root)) : fx.root;
  const files: Record<string, string> = fx.files ?? {};
  // The reader a missing file meets in the fold viewer (readFileSync), so a skipped source reads the same.
  const read = (p: string) => {
    if (p in files) return files[p];
    throw new Error(`ENOENT: no such file or directory, open '${p}'`);
  };
  zone(fx.opts?.localTimeZone);
  const nodes: Map<string, any> = fold(withSources(structuredClone(root), read), fx.opts);
  writeFileSync(join(OUT, `${name}.input.json`), json({ root, files, opts: fx.opts }));
  writeFileSync(join(OUT, `${name}.folds.json`), json([...nodes]));
  console.log(`${name}: ${nodes.size} nodes`);
  walked(`fold-${name}`, JSON.parse(JSON.stringify([...nodes])), undefined, fx.opts?.localTimeZone);
}
for (const f of readdirSync(join(WALK, "fixtures")).filter((f) => f.endsWith(".json")).sort()) {
  const fx = JSON.parse(readFileSync(join(WALK, "fixtures", f), "utf8"));
  walked(f.replace(/\.json$/, ""), Array.isArray(fx.nodes) ? fx.nodes : Object.entries(fx.nodes), fx.facto, fx.localTimeZone);
}
writeFileSync(join(OUT, "REV"), `fold viewer source ${REV}\nbun ${Bun.version}\n`);
