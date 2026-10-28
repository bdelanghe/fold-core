// Records what the fold viewer fold-core was ported from does, as goldens (step 0 of the one-core plan).
//
//   FOLD_SRC=<dir holding programs/fold-viewer/fold.ts and programs/fleet-map/everything.ts> FOLD_REV=<its rev> bun goldens/gen.ts
//
// Each goldens/fixtures/<case>.json ({root, files, opts, through?}) is synthetic. This writes <case>.input.json ({root, files, opts},
// root after `through`, if any) and <case>.folds.json: fold(withSources(root, files), opts) as [id, node] pairs in Map insertion order.
import { readdirSync, readFileSync, writeFileSync } from "node:fs";
import { join } from "node:path";

const SRC = process.env.FOLD_SRC, REV = process.env.FOLD_REV;
if (!SRC || !REV) { console.error("usage: FOLD_SRC=<dir> FOLD_REV=<rev> bun goldens/gen.ts"); process.exit(2); }
const { fold, withSources } = await import(join(SRC, "programs/fold-viewer/fold.ts"));
const { everything } = await import(join(SRC, "programs/fleet-map/everything.ts"));
const through: Record<string, (p: any) => any> = { everything };

const OUT = import.meta.dir;
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
  writeFileSync(join(OUT, `${name}.input.json`), JSON.stringify({ root, files, opts: fx.opts }, null, 2) + "\n");
  writeFileSync(join(OUT, `${name}.folds.json`), JSON.stringify([...nodes], null, 2) + "\n");
  console.log(`${name}: ${nodes.size} nodes`);
}
writeFileSync(join(OUT, "REV"), `fold viewer source ${REV}\nbun ${Bun.version}\n`);
