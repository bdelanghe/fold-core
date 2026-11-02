// The viewer's service worker (COGNITIVE-BROWSER-SDK-2026-10-30 step 3; brief items 10 and 22). phobos sleeps while Bobby is away,
// so with its host asleep /fold must still open and show the last fold it read, said as not updated since then (index.html).
// - The shell (this build's files) is kept in a cache named by the build's digest (@VERSION@, set by packages.viewer); like the page, it goes
//   to the network first and is answered from that cache only when the network is late or down (shellFile). A new build installs a new
//   cache and drops the old.
// - A page (a navigation) and the data (the document and its `from` files, any other GET on this origin) go to the network first.
//   What comes back is kept; with no answer within WAIT ms, or none at all, the kept copy answers, marked `Fold-Stored: <ISO time>`
//   (when it was kept), which is how the viewer knows it is showing the past. Nothing is kept that was not a 200.
// - Nothing but GET on this origin is touched: no write goes through here (brief item 27). Nor is live state: a request the page
//   makes with `cache: "no-store"` (Needs you's needs.json, sign.js) or anything under /approve/ (chitin-approve's challenge, which a
//   kept copy must never answer) goes to the network alone, as if this worker were not here.
const SHELL = "fold-shell-@VERSION@", DATA = "fold-data", WAIT = 3000;
const FILES = ["./", "view.js", "fold-dom.js", "sign.js", "clax.js", "theme.tokens.json", "fold-core.js", "fold_core.wasm", "fold.css", "fonts/atkinson-hyperlegible-next.woff2", "manifest.webmanifest",
  "favicon.svg", "apple-touch-icon.png", "icon-192.png", "icon-512.png", "icon-maskable-512.png"];
const scope = new URL("./", self.location).href;
const shell = new Set(FILES.filter((f) => f !== "./").map((f) => new URL(f, scope).href));

self.addEventListener("install", (e) => e.waitUntil(caches.open(SHELL).then((c) => c.addAll(FILES)).then(() => self.skipWaiting())));
self.addEventListener("activate", (e) => e.waitUntil(
  caches.keys().then((ks) => Promise.all(ks.filter((k) => k.startsWith("fold-shell-") && k !== SHELL).map((k) => caches.delete(k)))).then(() => self.clients.claim()),
));

// The network's answer within WAIT ms (kept when it is a 200), else the kept copy marked with when it was kept, else the network's late
// answer or its failure.
async function fresh(req) {
  const net = fetch(req).then(async (r) => {
    if (r.status === 200) {
      const h = new Headers(r.headers);
      h.set("Fold-Stored", new Date().toISOString());
      const body = await r.clone().arrayBuffer();
      await (await caches.open(DATA)).put(req.url, new Response(body, { status: 200, statusText: r.statusText, headers: h }));
    }
    return r;
  });
  const late = new Promise((r) => setTimeout(r, WAIT, null));
  const first = await Promise.race([net.catch(() => null), late]);
  if (first) return first;
  const kept = await (await caches.open(DATA)).match(req.url) ?? (req.mode === "navigate" ? await (await caches.open(SHELL)).match("./") : undefined);
  return kept ?? net;
}

// A shell file: the network's answer within WAIT ms, else this build's kept copy, else the network's late answer or its failure.
// Not cache-first: the page itself goes to the network first (fresh), so a cache-first shell paired a new index.html with the last
// build's modules until the new worker installed, and the page died on an import ("view.js does not provide an export named
// 'haecceityOf'", stuck at Loading…, 2026-11-02). The page and its shell now come from the same place: both from the network while
// the host is up, both from the kept copies while it sleeps.
async function shellFile(req, key) {
  const net = fetch(req);
  const late = new Promise((r) => setTimeout(r, WAIT, null));
  const first = await Promise.race([net.then((r) => (r.ok ? r : null), () => null), late]);
  return first ?? (await caches.match(key, { cacheName: SHELL })) ?? net;
}

self.addEventListener("fetch", (e) => {
  const req = e.request, u = new URL(req.url);
  if (req.method !== "GET" || u.origin !== self.location.origin || req.headers.get("accept") === "text/event-stream") return;
  if (req.cache === "no-store" || u.pathname.startsWith("/approve/")) return;
  if (shell.has(u.origin + u.pathname)) e.respondWith(shellFile(req, u.origin + u.pathname));
  else e.respondWith(fresh(req));
});
