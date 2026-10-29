// The wasm module at byte parity on every golden, as tests/parity.rs and tests/walk.rs are for the native crate. FOLD_CORE_WASM: the built module.
import { expect, test } from "bun:test";
import { readdirSync, readFileSync } from "node:fs";
import { join } from "node:path";
import { core } from "./fold-core.ts";

const goldens = join(import.meta.dir, "..", "goldens");
const wasm = process.env.FOLD_CORE_WASM ?? join(import.meta.dir, "..", "target", "wasm32-unknown-unknown", "release", "fold_core.wasm");
const { fold, walk } = await core(readFileSync(wasm));
const cases = (dir: string) => readdirSync(dir).filter((f) => f.endsWith(".input.json")).sort();
const read = (...p: string[]) => JSON.parse(readFileSync(join(...p), "utf8"));
for (const f of cases(goldens)) {
  const name = f.replace(/\.input\.json$/, "");
  test(`${name}: wasm at byte parity`, () => {
    expect(JSON.stringify(fold(read(goldens, f)))).toBe(JSON.stringify(read(goldens, `${name}.folds.json`)));
  });
}
for (const f of cases(join(goldens, "walk"))) {
  const name = f.replace(/\.input\.json$/, "");
  test(`walk ${name}: wasm at byte parity`, () => {
    expect(JSON.stringify(walk(read(goldens, "walk", f)))).toBe(JSON.stringify(read(goldens, "walk", `${name}.walk.json`)));
  });
}
test("a bad input says why", () => {
  expect(() => fold({} as any)).toThrow(/fold-core: no root/);
  expect(() => walk({} as any)).toThrow(/fold-core: no nodes/);
});
