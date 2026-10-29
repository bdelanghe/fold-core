// fold-core's WebAssembly module, loaded with no generated glue: `fold({root, files, opts})` returns the fold list
// (`[[id, node], ...]`, insertion order) exactly as goldens/<case>.folds.json holds it. Runs in bun, deno and browsers.
export type Folds = [string, Record<string, unknown>][];
export async function load(wasm: BufferSource): Promise<(input: { root: unknown; files?: Record<string, string>; opts?: Record<string, unknown> }) => Folds> {
  const { instance } = await WebAssembly.instantiate(wasm, {});
  const x = instance.exports as unknown as { memory: WebAssembly.Memory; alloc(n: number): number; dealloc(p: number, n: number): void; fold_json(p: number, n: number): bigint };
  return (input) => {
    const bytes = new TextEncoder().encode(JSON.stringify(input));
    const p = x.alloc(bytes.length);
    new Uint8Array(x.memory.buffer, p, bytes.length).set(bytes);
    const r = x.fold_json(p, bytes.length);
    x.dealloc(p, bytes.length);
    const op = Number(r >> 32n), on = Number(r & 0xffffffffn);
    const text = new TextDecoder().decode(new Uint8Array(x.memory.buffer, op, on));
    x.dealloc(op, on);
    const out = JSON.parse(text);
    if (!Array.isArray(out)) throw new Error(`fold-core: ${out.error}`);
    return out;
  };
}
