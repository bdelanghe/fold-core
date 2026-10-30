// The wasm module at byte parity on every golden, as tests/parity.rs and tests/walk.rs are for the native crate. FOLD_CORE_WASM: the built module.
// FOLD_CORE_NATIVE instead: the native static library (packages.native) through its C ABI, as linked into native/roundtrip.c.
import { expect, test } from "bun:test";
import { readdirSync, readFileSync } from "node:fs";
import { join } from "node:path";
import { core } from "./fold-core.ts";
import { nativeCore } from "./native.ts";

const goldens = join(import.meta.dir, "..", "goldens");
const wasm = process.env.FOLD_CORE_WASM ?? join(import.meta.dir, "..", "target", "wasm32-unknown-unknown", "release", "fold_core.wasm");
const native = process.env.FOLD_CORE_NATIVE;
const via = native ? "native" : "wasm";
// the C host carries fold, walk and ax; html and site (and the needs overlay) are the wasm module's, so their cases run on the module only
const { fold, walk, ax, html, site, run } = native
  ? { ...nativeCore(native), html: undefined as any, site: undefined as any }
  : { ...(await core(readFileSync(wasm))), run: null };
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
// The same HTML as the native crate writes (tests/html.rs), from the same trees: native and WASM, byte for byte.
for (const f of native ? [] : readdirSync(join(goldens, "html")).filter((f) => f.endsWith(".html.json")).sort()) {
  const name = f.replace(/\.html\.json$/, "");
  test(`html ${name}: wasm writes the native bytes`, () => {
    const trees = read(goldens, "ax", `${name}.ax.json`), want = read(goldens, "html", f);
    expect(want.length).toBe(trees.length);
    trees.forEach(([, , , tree]: unknown[], i: number) => {
      const w = want[i][3];
      if (typeof w === "string") expect(html(tree as any)).toBe(w);
      else expect(() => html(tree as any)).toThrow(`fold-core: ${w.error}`);
    });
  });
}
// Every fold's page as the native crate writes it (tests/pages.rs): the same input, the same bytes.
for (const f of native ? [] : readdirSync(join(goldens, "pages")).filter((f) => f.endsWith(".pages.json")).sort()) {
  const name = f.replace(/\.pages\.json$/, "");
  test(`pages ${name}: wasm writes the native bytes`, () => {
    const input = read(goldens, "walk", `${name}.input.json`);
    expect(JSON.stringify(site({ nodes: input.nodes, source: name, localTimeZone: input.opts.localTimeZone }))).toBe(JSON.stringify(read(goldens, "pages", f)));
  });
}
test("a bad input says why", () => {
  expect(() => fold({} as any)).toThrow(/fold-core: no root/);
  expect(() => walk({} as any)).toThrow(/fold-core: no nodes/);
  expect(() => ax({ href: "other" } as any)).toThrow(/fold-core: href: other/);
  if (!native) expect(() => site({ nodes: [] } as any)).toThrow(/fold-core: no source/);
});
if (native)
  test("text that is not JSON says why, from C", () => {
    for (const call of ["fold", "walk", "ax"]) expect(() => run!(call, "{")).toThrow(/fold-core: input is not JSON/);
  });
