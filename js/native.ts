// The native library (packages.native) through its C ABI, as linked into native/roundtrip.c: one C call per run of that host, its
// input on stdin as `<call>\t<JSON>`, its output one line, read as core() reads the module's (an `{error}` throws). For the tests.
import { type Ax, type AxInput, type FoldInput, type Folds, type WalkInput, type WalkOutput } from "./fold-core.ts";

export function nativeCore(bin: string) {
  const run = (name: string, text: string) => {
    const r = Bun.spawnSync([bin], { stdin: Buffer.from(`${name}\t${text}\n`) });
    if (r.exitCode !== 0) throw new Error(`${bin} exited ${r.exitCode}`);
    const out = JSON.parse(r.stdout.toString());
    if (out && !Array.isArray(out) && "error" in out) throw new Error(`fold-core: ${out.error}`);
    return out;
  };
  const json = (name: string) => (input: unknown) => run(name, JSON.stringify(input));
  return { run, fold: json("fold") as (i: FoldInput) => Folds, walk: json("walk") as (i: WalkInput) => WalkOutput, ax: json("ax") as (i: AxInput) => Ax };
}
