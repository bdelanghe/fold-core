// fold-core's WebAssembly module, loaded with no generated glue. Runs in bun, deno and browsers.
//   fold({root, files, opts}) -> the fold list ([[id, node], ...], insertion order), exactly as goldens/<case>.folds.json holds it
//   walk({nodes, opts, facto?}) -> {walk, verdict, facto?}, exactly as goldens/walk/<case>.walk.json holds it
export type Folds = [string, Record<string, unknown>][];
export type FoldInput = { root: unknown; files?: Record<string, string>; opts?: Record<string, unknown> };
export type Finding = { fold: string; check: string; kind: "remove" | "rename" | "restructure"; blocks: boolean; said: string; reduction: string };
export type WalkInput = {
  nodes: Folds | Record<string, unknown>;
  opts: { budgets: { id: string; value: number }[]; localTimeZone?: string };
  facto?: { artifact: string; at: string; rev: string };
};
export type WalkOutput = { walk: { folds: number; reached: number; findings: Finding[] }; verdict: string; facto?: { vertices: unknown[]; edges: unknown[] } };

export async function core(wasm: BufferSource): Promise<{ fold: (input: FoldInput) => Folds; walk: (input: WalkInput) => WalkOutput }> {
  const { instance } = await WebAssembly.instantiate(wasm, {});
  type Call = (p: number, n: number) => bigint;
  const x = instance.exports as unknown as { memory: WebAssembly.Memory; alloc(n: number): number; dealloc(p: number, n: number): void; fold_json: Call; walk_json: Call };
  const call = (f: Call, input: unknown) => {
    const bytes = new TextEncoder().encode(JSON.stringify(input));
    const p = x.alloc(bytes.length);
    new Uint8Array(x.memory.buffer, p, bytes.length).set(bytes);
    const r = f(p, bytes.length);
    x.dealloc(p, bytes.length);
    const op = Number(r >> 32n), on = Number(r & 0xffffffffn);
    const text = new TextDecoder().decode(new Uint8Array(x.memory.buffer, op, on));
    x.dealloc(op, on);
    const out = JSON.parse(text);
    if (out && !Array.isArray(out) && "error" in out) throw new Error(`fold-core: ${out.error}`);
    return out;
  };
  return { fold: (input) => call(x.fold_json, input), walk: (input) => call(x.walk_json, input) };
}

/** The fold alone, as before `core`. */
export async function load(wasm: BufferSource): Promise<(input: FoldInput) => Folds> {
  return (await core(wasm)).fold;
}
