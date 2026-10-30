// fold-core's WebAssembly module, loaded with no generated glue. Runs in bun, deno and browsers. The module carries no zone data:
// it asks this loader for a zone's offset (fold_core.tz_offset), answered from Intl, the zone rules fold.ts itself reads.
//   fold({root, files, opts}) -> the fold list ([[id, node], ...], insertion order), exactly as goldens/<case>.folds.json holds it
//   walk({nodes, opts, facto?}) -> {walk, verdict, facto?}, exactly as goldens/walk/<case>.walk.json holds it
//   ax({node, path, mode, href, localTimeZone?}) -> one fold's accessibility tree (fold-ax.js axOf), as goldens/ax/<case>.ax.json holds it
//   html(tree) -> that tree's HTML (fold-dom.js toDom, as text: src/html.rs), as goldens/html/<case>.html.json holds it
export type Folds = [string, Record<string, unknown>][];
export type FoldInput = { root: unknown; files?: Record<string, string>; opts?: Record<string, unknown> };
export type Finding = { fold: string; check: string; kind: "remove" | "rename" | "restructure"; blocks: boolean; said: string; reduction: string };
export type WalkInput = {
  nodes: Folds | Record<string, unknown>;
  opts: { budgets: { id: string; value: number }[]; localTimeZone?: string };
  facto?: { artifact: string; at: string; rev: string };
};
export type WalkOutput = { walk: { folds: number; reached: number; findings: Finding[] }; verdict: string; facto?: { vertices: unknown[]; edges: unknown[] } };

/** Seconds east of UTC in `zone` at `ms`, from Intl; null for a zone Intl does not know. */
const formats = new Map<string, Intl.DateTimeFormat | null>();
export function offset(zone: string, ms: number): number | null {
  let f = formats.get(zone);
  if (f === undefined) {
    try {
      f = new Intl.DateTimeFormat("en-US", { timeZone: zone, hourCycle: "h23", era: "short", year: "numeric", month: "numeric", day: "numeric", hour: "numeric", minute: "numeric", second: "numeric" });
    } catch {
      f = null;
    }
    formats.set(zone, f);
  }
  if (!f) return null;
  const t = Math.floor(ms / 1000) * 1000;
  const p = Object.fromEntries(f.formatToParts(t).map((x) => [x.type, x.value]));
  const year = p.era === "BC" || p.era === "B" ? 1 - Number(p.year) : Number(p.year);
  const local = new Date(0);
  local.setUTCFullYear(year, Number(p.month) - 1, Number(p.day));
  local.setUTCHours(Number(p.hour), Number(p.minute), Number(p.second), 0);
  return (local.getTime() - t) / 1000;
}

export type AxInput = { node: unknown; path: string[]; mode: "folds" | "slides"; href: "walk" | "viewer"; localTimeZone?: string };
export type Ax = { role: string; name?: string; children?: Ax[]; [k: string]: unknown };

export async function core(wasm: BufferSource): Promise<{ fold: (input: FoldInput) => Folds; walk: (input: WalkInput) => WalkOutput; ax: (input: AxInput) => Ax; html: (tree: Ax) => string }> {
  type Call = (p: number, n: number) => bigint;
  let x: { memory: WebAssembly.Memory; alloc(n: number): number; dealloc(p: number, n: number): void; fold_json: Call; walk_json: Call; ax_json: Call; html_json: Call };
  const tz_offset = (p: number, n: number, ms: number) => offset(new TextDecoder().decode(new Uint8Array(x.memory.buffer, p, n)), ms) ?? -2147483648;
  const { instance } = await WebAssembly.instantiate(wasm, { fold_core: { tz_offset } });
  x = instance.exports as unknown as typeof x;
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
  return { fold: (input) => call(x.fold_json, input), walk: (input) => call(x.walk_json, input), ax: (input) => call(x.ax_json, input), html: (tree) => call(x.html_json, tree).html };
}

/** The fold alone, as before `core`. */
export async function load(wasm: BufferSource): Promise<(input: FoldInput) => Folds> {
  return (await core(wasm)).fold;
}
