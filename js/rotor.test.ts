// The rotor and yank through the wasm module (core().rotor), on every `core` case of Allie's goldens (goldens/rotor-yank/), as
// tests/rotor.rs is for the native crate. FOLD_CORE_WASM: the built module; FOLD_CORE_NATIVE instead: the native library (js/native.ts).
import { expect, test } from "bun:test";
import { readFileSync } from "node:fs";
import { join } from "node:path";
import { core, type Kind, type RotorNode } from "./fold-core.ts";
import { nativeCore } from "./native.ts";

const dir = join(import.meta.dir, "..", "goldens", "rotor-yank");
const wasm = process.env.FOLD_CORE_WASM ?? join(import.meta.dir, "..", "target", "wasm32-unknown-unknown", "release", "fold_core.wasm");
const { rotor } = process.env.FOLD_CORE_NATIVE ? nativeCore(process.env.FOLD_CORE_NATIVE) : await core(readFileSync(wasm));
const read = (...p: string[]) => JSON.parse(readFileSync(join(dir, ...p), "utf8"));

type Dump = { axRole: string; name?: string; value?: unknown; children?: Dump[] };
const node = (n: Dump): RotorNode => ({
  role: n.axRole.replace(/^AX/, "").toLowerCase(),
  name: n.name ?? "",
  ...(n.value == null ? {} : { value: n.value as string }),
  kids: (n.children ?? []).map(node),
});
// Allie's ten kinds (allie.model.json meta.kinds): the five VoiceOver kinds by role, the rest Allie's own
const roles: Record<string, string[]> = {
  inputs: ["textfield", "textarea", "searchfield", "combobox"],
  buttons: ["button", "menubutton", "popupbutton", "checkbox", "radiobutton"],
  links: ["link"],
  headings: ["heading"],
  windows: ["window", "sheet", "tabgroup"],
};
const kinds: Kind[] = ["keyboard", "commands", "keys", "inputs", "buttons", "links", "headings", "windows", "apps", "actions"].map((name) => ({ name, say: name, ...(roles[name] ? { roles: roles[name] } : {}) }));
const named = (t: RotorNode, name: string, at: number[] = []): number[] | null => {
  if (t.name === name) return at;
  for (const [i, k] of t.kids.entries()) {
    const r = named(k, name, [...at, i]);
    if (r) return r;
  }
  return null;
};

for (const file of ["kinds", "cycle", "filter", "empty", "yank"]) {
  const g = read(`${file}.json`);
  const tree: RotorNode = { role: "host", name: "phobos", kids: read("trees", `${g.tree}.tree.json`).tree.map(node) };
  for (const c of g.cases.filter((c: { scope: string }) => c.scope === "core")) {
    test(`${file}/${c.id}`, () => {
      let state = rotor.init(tree, named(tree, c.start)!);
      c.expect.forEach((want: Record<string, unknown>, i: number) => {
        let effects = { say: null as string | null, hint: null as string | null, copy: [] as string[] };
        if (i > 0) ({ state, effects } = rotor.key(tree, state, c.keys[i - 1], { kinds }));
        const row = rotor.row(tree, state);
        expect({
          key: want.key,
          view: state.view?.kind ?? "",
          mode: state.view?.mode ?? "",
          filter: state.view?.filter ?? "",
          rotor: state.drawer ? kinds[state.drawer.at].name : null,
          at: row?.address ?? "",
          row: row && [row.name, row.role, row.fact].filter(Boolean).join(", "),
          said: effects.say ?? "",
          hint: effects.hint ?? "",
          copy: effects.copy,
          focus: state.drawer ? "ax-rotor-list" : "ax-tree",
        }).toEqual(want);
      });
    });
  }
}
