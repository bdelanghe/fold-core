// The wasm module at byte parity on every golden, as tests/parity.rs is for the native crate. FOLD_CORE_WASM: the built module.
import { expect, test } from "bun:test";
import { readdirSync, readFileSync } from "node:fs";
import { join } from "node:path";
import { load } from "./fold-core.ts";

const goldens = join(import.meta.dir, "..", "goldens");
const wasm = process.env.FOLD_CORE_WASM ?? join(import.meta.dir, "..", "target", "wasm32-unknown-unknown", "release", "fold_core.wasm");
const fold = await load(readFileSync(wasm));
for (const f of readdirSync(goldens).filter((f) => f.endsWith(".input.json")).sort()) {
  const name = f.replace(/\.input\.json$/, "");
  test(`${name}: wasm at byte parity`, () => {
    const want = JSON.parse(readFileSync(join(goldens, `${name}.folds.json`), "utf8"));
    expect(JSON.stringify(fold(JSON.parse(readFileSync(join(goldens, f), "utf8"))))).toBe(JSON.stringify(want));
  });
}
test("a bad input says why", () => { expect(() => fold({} as any)).toThrow(/fold-core: no root/); });
