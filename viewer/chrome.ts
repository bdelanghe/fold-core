// Headless Chromium over the DevTools protocol, for the checks that read what a browser makes of a page (browser-check.ts,
// pages-check.ts): one browser at one window size, its first tab, and `send` (a protocol call) and `value` (an expression's
// value) on it, in its own profile (by default one per width). `close` ends the browser; `err` is what it said on stderr, for a failure's log.
export type Chrome = {
  send: (method: string, params?: unknown) => Promise<any>;
  value: (expression: string) => Promise<any>;
  err: () => string;
  close: () => Promise<void>;
};

export async function chrome(chromium: string, w: number, h: number, profile = `chromium-${w}`): Promise<Chrome> {
  const p = Bun.spawn([chromium, "--headless=new", "--no-sandbox", "--disable-gpu", "--disable-dev-shm-usage", "--no-first-run", `--user-data-dir=${process.env.TMPDIR ?? "/tmp"}/${profile}`,
    `--window-size=${w},${h}`, "--remote-debugging-port=0", "about:blank"], { stdout: "ignore", stderr: "pipe", timeout: 600_000 });
  let err = "";
  try {
    const rd = p.stderr.getReader();
    let ws = "";
    while (!ws) { const { value, done } = await rd.read(); if (done) throw new Error("chromium exited before DevTools listened"); err += new TextDecoder().decode(value); ws = err.match(/ws:\/\/\S+/)?.[0] ?? ""; }
    const port = new URL(ws).port;
    const target = (await (await fetch(`http://127.0.0.1:${port}/json/list`)).json()).find((t: { type: string }) => t.type === "page");
    const s = new WebSocket(target.webSocketDebuggerUrl);
    await new Promise((r, j) => { s.onopen = r; s.onerror = j; });
    let n = 0; const pending = new Map<number, (m: any) => void>();
    s.onmessage = (e) => { const m = JSON.parse(String(e.data)); if (m.id) pending.get(m.id)?.(m); };
    const send = (method: string, params = {}) => new Promise<any>((r) => { const i = ++n; pending.set(i, (m) => r(m.result)); s.send(JSON.stringify({ id: i, method, params })); });
    const value = async (expression: string) => (await send("Runtime.evaluate", { expression, returnByValue: true }))?.result?.value;
    return { send, value, err: () => err, close: async () => { s.close(); p.kill(); await p.exited; } };
  } catch (e) { p.kill(); await p.exited; throw new Error(`${e}\n${err.slice(-2000)}`); }
}
