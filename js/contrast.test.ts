// Contrast parity (brief item 17): Baobab's own TS (@bounded-systems/baobab 0.2.2, flattenTokens and checkContrast) over the
// viewer's theme (viewer/theme.tokens.json) gives goldens/contrast.json, which the Rust port (src/baobab.rs, tests/contrast.rs)
// writes: the same flattened tokens in the same order, and the same ratio to the last bit and verdict on every pair.

import { expect, test } from "bun:test";
import { readFileSync } from "node:fs";
import { join } from "node:path";
import { checkContrast, flattenTokens } from "@bounded-systems/baobab";

const read = (p: string) => JSON.parse(readFileSync(join(import.meta.dir, "..", p), "utf8"));
const theme = read("viewer/theme.tokens.json");
const golden = read("goldens/contrast.json");

test("Baobab flattens the theme to the Rust port's tokens", () => {
  expect(JSON.stringify(flattenTokens(theme))).toBe(JSON.stringify(golden.tokens));
});

test("Baobab's checkContrast agrees with the Rust port on every pair", () => {
  const results = checkContrast(golden.tokens, golden.pairs);
  expect(results).toHaveLength(golden.results.length);
  expect(JSON.stringify(results)).toBe(JSON.stringify(golden.results));
  expect(results.every((r) => r.pass)).toBe(true);
});
