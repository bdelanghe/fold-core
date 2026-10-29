// Records what the fold viewer fold-core was ported from does, as goldens (step 0 of the one-core plan).
//
//   TZ=UTC FOLD_SRC=<a home source tree> FOLD_REV=<its rev> bun goldens/gen.ts
//
// Folds: each goldens/fixtures/<case>.json ({root, files, opts, through?}) is synthetic. This writes <case>.input.json ({root,
// files, opts}, root after `through`, if any) and <case>.folds.json: fold(withSources(root, files), opts) as [id, node] pairs in
// Map insertion order.
// Walks (walk.ts): every fold case's folds, and each goldens/walk/fixtures/<case>.json ({nodes: {id: node} or [[id, node]],
// facto?}), give goldens/walk/<case>.input.json ({nodes, opts, facto?}) and <case>.walk.json ({walk, verdict, facto?}), as the
// core's walk_json returns them. TZ=UTC: the "Data from …" line is said in the machine's zone, recorded as opts.localTimeZone.
import { mkdirSync, readdirSync, readFileSync, writeFileSync } from "node:fs";
import { join } from "node:path";

const SRC = process.env.FOLD_SRC, REV = process.env.FOLD_REV;
if (!SRC || !REV) { console.error("usage: TZ=UTC FOLD_SRC=<dir> FOLD_REV=<rev> bun goldens/gen.ts"); process.exit(2); }
const { fold, withSources } = await import(join(SRC, "programs/fold-viewer/fold.ts"));
const { walk, verdict, factoBody } = await import(join(SRC, "programs/fold-viewer/walk.ts"));
const { everything } = await import(join(SRC, "programs/fleet-map/everything.ts"));
const through: Record<string, (p: any) => any> = { everything };
const localTimeZone = Intl.DateTimeFormat().resolvedOptions().timeZone;
const budgets = JSON.parse(readFileSync(process.env.WALK_BUDGETS ?? join(SRC, "programs/fleet-map/budgets.json"), "utf8")).budgets
  .filter((b: any) => b.id === "view.words").map(({ id, value }: any) => ({ id, value }));
const json = (v: unknown) => JSON.stringify(v, null, 2) + "\n";

const OUT = import.meta.dir, WALK = join(OUT, "walk");
mkdirSync(WALK, { recursive: true });
const walked = (name: string, pairs: [string, any][], facto?: { artifact: string; at: string; rev: string }) => {
  const r = walk(new Map(structuredClone(pairs)));
  writeFileSync(join(WALK, `${name}.input.json`), json({ nodes: pairs, opts: { budgets, localTimeZone }, ...(facto ? { facto } : {}) }));
  writeFileSync(join(WALK, `${name}.walk.json`), json({ walk: r, verdict: verdict(r), ...(facto ? { facto: factoBody(facto.artifact, facto.at, facto.rev, r) } : {}) }));
  console.log(`walk ${name}: ${r.reached}/${r.folds} reached, ${r.findings.length} findings`);
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
  const nodes: Map<string, any> = fold(withSources(structuredClone(root), read), fx.opts);
  writeFileSync(join(OUT, `${name}.input.json`), json({ root, files, opts: fx.opts }));
  writeFileSync(join(OUT, `${name}.folds.json`), json([...nodes]));
  console.log(`${name}: ${nodes.size} nodes`);
  walked(`fold-${name}`, JSON.parse(JSON.stringify([...nodes])));
}
for (const f of readdirSync(join(WALK, "fixtures")).filter((f) => f.endsWith(".json")).sort()) {
  const fx = JSON.parse(readFileSync(join(WALK, "fixtures", f), "utf8"));
  walked(f.replace(/\.json$/, ""), Array.isArray(fx.nodes) ? fx.nodes : Object.entries(fx.nodes), fx.facto);
}
writeFileSync(join(OUT, "REV"), `fold viewer source ${REV}\nbun ${Bun.version}\n`);
