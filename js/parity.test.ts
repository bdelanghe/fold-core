// The wasm module at byte parity on every golden, as tests/parity.rs and tests/walk.rs are for the native crate. FOLD_CORE_WASM: the built module.
// FOLD_CORE_NATIVE instead: the native static library (packages.native) through its C ABI, as linked into native/roundtrip.c.
import { expect, test } from "bun:test";
import { readdirSync, readFileSync } from "node:fs";
import { join } from "node:path";
import { core } from "./fold-core.ts";

const goldens = join(import.meta.dir, "..", "goldens");
const wasm = process.env.FOLD_CORE_WASM ?? join(import.meta.dir, "..", "target", "wasm32-unknown-unknown", "release", "fold_core.wasm");
const native = process.env.FOLD_CORE_NATIVE;
const via = native ? "native" : "wasm";
// one C call per run of the host: its input on stdin as `<call>\t<JSON>`, its output one line, read as core() reads the module's
const run = (name: string, text: string) => {
  const r = Bun.spawnSync([native!], { stdin: Buffer.from(`${name}\t${text}\n`) });
  if (r.exitCode !== 0) throw new Error(`${native} exited ${r.exitCode}`);
  const out = JSON.parse(r.stdout.toString());
  if (out && !Array.isArray(out) && "error" in out) throw new Error(`fold-core: ${out.error}`);
  return out;
};
const { fold, walk, ax } = native
  ? { fold: (i: unknown) => run("fold", JSON.stringify(i)), walk: (i: unknown) => run("walk", JSON.stringify(i)), ax: (i: unknown) => run("ax", JSON.stringify(i)) }
  : await core(readFileSync(wasm));
const cases = (dir: string) => readdirSync(dir).filter((f) => f.endsWith(".input.json")).sort();
const read = (...p: string[]) => JSON.parse(readFileSync(join(...p), "utf8"));
for (const f of cases(goldens)) {
  const name = f.replace(/\.input\.json$/, "");
  test(`${name}: ${via} at byte parity`, () => {
    expect(JSON.stringify(fold(read(goldens, f)))).toBe(JSON.stringify(read(goldens, `${name}.folds.json`)));
  });
}
for (const f of cases(join(goldens, "walk"))) {
  const name = f.replace(/\.input\.json$/, "");
  test(`walk ${name}: ${via} at byte parity`, () => {
    expect(JSON.stringify(walk(read(goldens, "walk", f)))).toBe(JSON.stringify(read(goldens, "walk", `${name}.walk.json`)));
  });
}
for (const f of readdirSync(join(goldens, "ax")).sort()) {
  const name = f.replace(/\.ax\.json$/, "");
  test(`ax ${name}: ${via} at byte parity`, () => {
    const input = read(goldens, "walk", `${name}.input.json`), nodes = new Map(input.nodes);
    for (const [id, mode, href, tree] of read(goldens, "ax", f))
      expect(JSON.stringify(ax({ node: nodes.get(id) ?? null, path: id === "root" ? [] : id.split("~"), mode, href, localTimeZone: input.opts.localTimeZone }))).toBe(JSON.stringify(tree));
  });
}
test("a bad input says why", () => {
  expect(() => fold({} as any)).toThrow(/fold-core: no root/);
  expect(() => walk({} as any)).toThrow(/fold-core: no nodes/);
  expect(() => ax({ href: "other" } as any)).toThrow(/fold-core: href: other/);
});
if (native)
  test("text that is not JSON says why, from C", () => {
    for (const call of ["fold", "walk", "ax"]) expect(() => run(call, "{")).toThrow(/fold-core: input is not JSON/);
  });
