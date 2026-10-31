// Every golden fold's pre-rendered page (goldens/pages, src/page.rs) in headless Chromium with JavaScript off
// (checks.pages-browser), served at its real path under /fold/ beside the viewer's stylesheet, as fold-serve would serve it.
// Each page, in light, dark, light and dark under prefers-contrast:more, and forced-colors:
//   - names the fold and its path in its title;
//   - entered as its links enter it (at #<its h1's id>), has focus on its one h1, with no script to put it there;
//   - moves between folds by plain links in the same tab, each to a page the site has (or one it holds as refused) and to an
//     element on it (its h1, or the row it was left by); a link to a fold the document does not have at all is the data's gap,
//     said but not failed: some goldens name such folds on purpose;
//   - passes lone's colour contrast over every piece of visible text, with the colours Chromium computed from fold.css
//     (lone reads colours off its nodes; here they are the browser's own, so the stylesheet is what is checked), and under
//     prefers-contrast:more every run clears 7:1 too (Baobab's contrastRatio; lone holds AA only);
//   - draws a visible border on every button and field, so each edge survives forced-colors, where fills are the user's Canvas
//     (an act's hidden fields are not drawn at all, so they have no edge to keep).
//   CHROMIUM=<chromium binary> [VIEWER=<packages.viewer>, for its fonts] bun viewer/pages-check.ts [<golden case>…]
import { readdirSync, readFileSync } from "node:fs";
import { join } from "node:path";
import { contrastRatio } from "@bounded-systems/baobab";
import { validateColorContrast } from "@bounded-systems/lone";
import { chrome } from "./chrome.ts";

type Page = { id: string; mode: string; href: string; file?: string; html?: string; error?: string };
const goldens = join(import.meta.dir, "..", "goldens", "pages");
const names = Bun.argv.slice(2).length ? Bun.argv.slice(2) : readdirSync(goldens).filter((f) => f.endsWith(".pages.json")).map((f) => f.replace(/\.pages\.json$/, "")).sort();
const css = readFileSync(join(import.meta.dir, "fold.css"));
const viewer = process.env.VIEWER;
type Known = { case: string; page: string; finding: string; why: string; clears: string };
const known: Known[] = JSON.parse(readFileSync(join(goldens, "known.json"), "utf8")).known;
const seen = new Set<Known>();
let site = new Map<string, Page>();
const server = Bun.serve({
  hostname: "127.0.0.1", port: 0,
  fetch(req) {
    const p = decodeURIComponent(new URL(req.url).pathname);
    if (p === "/fold/fold.css") return new Response(css, { headers: { "content-type": "text/css" } });
    if (p.startsWith("/fold/fonts/") && viewer) return new Response(Bun.file(join(viewer, p.slice("/fold/".length))));
    // as a static server reads a real path: the decoded segments, then index.html
    const page = site.get(p.slice("/fold/".length) + "index.html");
    return page?.html ? new Response(page.html, { headers: { "content-type": "text/html; charset=utf-8" } }) : new Response("", { status: 404 });
  },
});

// In the page: its title, h1s, what has focus, scripts, every link (href, target), and each visible run of text with the
// colours it is drawn in (its own colour, and the first opaque background behind it).
const look = `(() => {
  const bg = (e) => { for (; e; e = e.parentElement) { const c = getComputedStyle(e).backgroundColor; if (!/^rgba\\(.*, 0\\)$|^transparent$/.test(c)) return c; } return "rgb(255, 255, 255)"; };
  const text = [...document.querySelectorAll("body *")].filter((e) => [...e.childNodes].some((n) => n.nodeType === 3 && n.data.trim()) && e.checkVisibility({ visibilityProperty: true }))
    .map((e) => { const s = getComputedStyle(e); return { at: e.localName + (e.className ? "." + e.className.replace(/ /g, ".") : "") + " " + JSON.stringify(e.textContent.slice(0, 40)), color: s.color, backgroundColor: bg(e), fontSize: s.fontSize, fontWeight: s.fontWeight }; });
  return { title: document.title, h1: [...document.querySelectorAll("h1")].map((h) => h.textContent), focused: document.activeElement?.localName ?? null,
    scripts: document.scripts.length, crumbs: [...document.querySelectorAll("nav a")].map((a) => a.textContent),
    links: [...document.querySelectorAll("a")].map((a) => [a.getAttribute("href"), a.getAttribute("target")]), text,
    edgeless: [...document.querySelectorAll("button, input:not([type=hidden])")].filter((e) => { const s = getComputedStyle(e); return s.borderTopStyle === "none" || parseFloat(s.borderTopWidth) === 0; }).map((e) => e.localName + " " + JSON.stringify((e.textContent || e.getAttribute("aria-label") || "").slice(0, 40))) };
})()`;

const off = { name: "prefers-contrast", value: "no-preference" }, more = { name: "prefers-contrast", value: "more" };
const modes: [string, { name: string; value: string }[]][] = [
  ["light", [{ name: "prefers-color-scheme", value: "light" }, off, { name: "forced-colors", value: "none" }]],
  ["dark", [{ name: "prefers-color-scheme", value: "dark" }, off, { name: "forced-colors", value: "none" }]],
  ["light-more", [{ name: "prefers-color-scheme", value: "light" }, more, { name: "forced-colors", value: "none" }]],
  ["dark-more", [{ name: "prefers-color-scheme", value: "dark" }, more, { name: "forced-colors", value: "none" }]],
  ["forced", [{ name: "prefers-color-scheme", value: "light" }, off, { name: "forced-colors", value: "active" }]],
];
// Chromium's computed rgb()/rgba() (opaque here: text over its first opaque background) as Baobab's #RRGGBB
const hex = (rgb: string) => "#" + rgb.match(/\d+(\.\d+)?/g)!.slice(0, 3).map((n) => Math.round(Number(n)).toString(16).padStart(2, "0")).join("");

const c = await chrome(process.env.CHROMIUM!, 390, 844);
let failed = 0, pages = 0;
try {
  await c.send("Page.enable");
  await c.send("Emulation.setScriptExecutionDisabled", { value: true });
  for (const name of names) {
    const all: Page[] = JSON.parse(readFileSync(join(goldens, `${name}.pages.json`), "utf8"));
    site = new Map(all.filter((p) => p.file).map((p) => [p.file!, p]));
    const refused = new Set(all.filter((p) => p.error).map((p) => p.href)), ids = new Set(all.map((p) => p.id)), gaps = new Set<string>();
    for (const [scheme, features] of modes) {
      // A case that ends on the page the next starts on (/fold/, its fragment aside) would be a same-document move: start fresh.
      await c.send("Page.navigate", { url: "about:blank" });
      await c.send("Emulation.setEmulatedMedia", { features });
      for (const p of all.filter((p) => p.html)) {
        const h1 = p.html!.match(/<h1 id="([^"]*)"/)?.[1];
        await c.send("Page.navigate", { url: `http://127.0.0.1:${server.port}${p.href}#${h1}` });
        const until = Date.now() + 10_000;
        // loaded, and its fragment's target focused (a browser runs the focusing steps for it as it scrolls to it)
        while ((await c.value(`document.readyState === "complete" && document.activeElement !== document.body`)) !== true && Date.now() < until) await Bun.sleep(20);
        const v = await c.value(look);
        const why: string[] = [];
        const want = (v.h1[0] + (v.crumbs.length ? ` — ${v.crumbs.join(" / ")}` : "")).replace(/\s+/g, " ").trim(); // as document.title reads it
        if (v.h1.length !== 1) why.push(`${v.h1.length} h1s`);
        if (v.title !== want) why.push(`title ${JSON.stringify(v.title)}, not ${JSON.stringify(want)}`);
        if (v.focused !== "h1") why.push(`focus on ${v.focused}, not the h1`);
        if (v.scripts) why.push(`${v.scripts} scripts`);
        for (const [link, target] of v.links as [string, string | null][]) {
          if (!link.startsWith("/fold/")) continue;
          if (target) why.push(`${link} opens in ${target}`);
          const [href, frag] = link.split("#");
          const to = decodeURIComponent(href).slice("/fold/".length) + "index.html";
          const steps = href.slice("/fold/".length).split("/").filter((x) => x && x !== "@slides").map(decodeURIComponent);
          if (site.has(to) && !site.get(to)!.html!.includes(` id="${frag}"`)) why.push(`${link}: no #${frag} there`);
          if (site.has(to) || refused.has(href)) continue;
          if (ids.has(steps.length ? steps.join("~") : "root")) why.push(`${href} goes nowhere`);
          else gaps.add(href);
        }
        const root = { type: "body", props: {}, children: v.text.map((t: any) => ({ type: "span", props: t, children: [] })) };
        for (const f of validateColorContrast(root as any)) why.push(`${v.text[Number(f.path.match(/\d+/)?.[0])]?.at}: ${f.message}`);
        if (scheme.endsWith("-more")) {
          for (const t of v.text) {
            const r = contrastRatio(hex(t.color), hex(t.backgroundColor));
            if (r < 7) why.push(`${t.at}: ${r.toFixed(2)}:1 under prefers-contrast:more, below 7:1`);
          }
        }
        for (const e of v.edgeless) why.push(`${e} has no visible border`);
        pages++;
        const left = why.filter((w) => { const k = known.find((k) => k.case === name && k.page === p.href && k.finding === w); if (k) seen.add(k); return !k; });
        if (left.length) { failed++; console.log(`FAIL ${name} ${scheme} ${p.href}\n  ${left.join("\n  ")}`); }
      }
    }
    console.log(`${name}: ${all.filter((p) => p.html).length} pages${refused.size ? `, ${refused.size} refused (${[...refused].join(", ")})` : ""}${gaps.size ? `; links to folds the data lacks: ${[...gaps].join(", ")}` : ""}`);
  }
} finally { await c.close(); server.stop(true); }
// a known finding that no longer occurs comes off the list (only when every case ran)
const gone = Bun.argv.slice(2).length ? [] : known.filter((k) => !seen.has(k));
for (const k of gone) console.log(`FAIL known.json lists ${k.case} ${k.page}: ${k.finding}, which no longer occurs`);
failed += gone.length;
console.log(`${pages} page loads, ${failed} failed`);
process.exit(failed ? 1 : 0);
