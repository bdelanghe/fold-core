// The rotor (BRIEF-stint-2026-11-03-basis-rotor), Allie's (bdelanghe/ax-rebuild fold.js, "the rotor is a path" and "the rotor's
// drawer (r)"), over the folded document instead of an app's tree. A kind is a view of the fold you are on: a fold of its own whose
// rows are the matches found anywhere under it (not only on screen), drawn through the core's ax like any fold, so its heading, rows,
// cursor and keys are the viewer's own and VoiceOver's own rotor (VO-U: headings, links) lists the same things. No panel of rows, no
// tabs. Leaving goes back to where you were.
//
// Allie's keys, unchanged:
//   r                  opens the drawer: the kinds as a listbox, on the kind you are in (else the last one)
//   in the drawer      j ↓ ]  next kind, k ↑ [  previous kind (no wrap: at either end it stays and says so);
//                      Enter or a digit picks; r picks the last kind (r r); Esc h ← close it, back to where you were
//   in a kind          j k ↓ ↑ move between its items (focus moves to each); l Enter go to the item's fold;
//                      ] [ the next and previous kind; h Esc ← Backspace leave, back to the fold and row you came from
// Entering a kind says its word once ("Actions"); each item then says itself as it takes focus (its title and where it is), so the
// live region never says what focus already does (Allie: VoiceOver would hear it twice). Never how many.

/** A fold's acts (src/ax.rs acts_of: `act` and `acts`). */
const actsOf = (n) => [n?.act, ...(Array.isArray(n?.acts) ? n.acts : [])].filter(Boolean);
const last = (p) => p[p.length - 1] ?? "";

/** The kinds, in the drawer's order (its digits). Each is a word and which folds it holds: `is(path, node)`, path the fold's ids. The
 * fleet page's ids (home fleet-map: `needs/…`, `goal/<seat>`, `needs/landing-<i>`, `inbox/<i>`) fold to one segment each, `/` as `-`. */
export const KINDS = [
  // what waits for you: each item of Needs you (signing, the asks, the landings and steps it lists)
  { name: "needs", word: "Needs", one: "need", is: (p) => p[0] === "needs" && p.length > 1 },
  // every fold with an act of its own (a Sign or a Run button)
  { name: "actions", word: "Actions", one: "action", is: (p, n) => actsOf(n).length > 0 },
  // every fold with rows to go into: a section or a group, each its own heading
  { name: "sections", word: "Sections", one: "section", is: (p, n) => p.length > 0 && (n?.kids ?? []).length > 0 },
  // the goal sessions: seat-runner's stints (Goal sessions' rows)
  { name: "stints", word: "Stints", one: "stint", is: (p) => (p[0] === "goals" && p.length === 2) || /^(goal|stint)-/.test(last(p)) },
  // what is landing: a landing's own row, wherever it is listed
  { name: "landings", word: "Landings", one: "landing", is: (p) => /(^|-)landing(s)?(-|$)/.test(last(p)) },
  // the messages to you (Messages to you's rows)
  { name: "messages", word: "Messages", one: "message", is: (p) => (p[0] === "inbox" && p.length === 2) || /^inbox-\d+$/.test(last(p)) },
  // every fold with a link out of the page
  { name: "links", word: "Links", one: "link", is: (p, n) => typeof n?.href === "string" && !!n.href.trim() && !n.href.startsWith("#") },
];
export const kindOf = (name) => KINDS.find((k) => k.name === name) ?? null;

/** The items of `kind` under the fold at `under` (its ids; [] the whole document), in reading order (depth first, as the folds are
 * listed): each `{id, path, node}`. The fold you are on is never one of its own items. */
export function kindItems(nodes, kind, under = []) {
  const k = kindOf(kind); if (!k) return [];
  const out = [], seen = new Set();
  const walk = (path) => {
    const id = path.length ? path.join("~") : "root";
    if (seen.has(id)) return; seen.add(id);
    const n = nodes.get(id); if (!n) return;
    if (path.length > under.length && k.is(path, n)) out.push({ id, path, node: n });
    for (const c of n.kids ?? []) if (c && typeof c.id === "string" && !c.href) walk([...path, c.id]);
  };
  walk(under);
  return out;
}

/** A kind as a fold to draw (the core's ax takes it as it takes any fold): its word as the title, one row per item, each going to
 * its fold, said with where it is (its trail under the document's title). None: a line saying so, and a row back. */
export function kindView(nodes, kind, under = [], back = { href: "#/", title: "" }) {
  const k = kindOf(kind), items = kindItems(nodes, kind, under);
  const where = (n) => (Array.isArray(n.trail) ? n.trail.slice(1) : []).join(" › ");
  const kids = items.map(({ id, path, node }) => ({ id, title: node.title ?? last(path), href: "#/" + path.map(encodeURIComponent).join("/"), ...(where(node) ? { hint: where(node) } : {}) }));
  const n = { title: k.word, trail: [], kids };
  if (!kids.length) { n.text = [`No ${k.name} here.`]; n.kids = [{ id: "back", title: `Back to ${back.title || "where you were"}`, href: back.href }]; }
  return n;
}

/** The rotor's state: the drawer (open, and the kind it highlights), the kind you are in ("" none), where you entered it from, and
 * the last kind picked (what r r and the drawer open on). */
export const rotorStart = () => ({ drawer: false, at: 0, kind: "", from: null, last: KINDS[0].name });

/**
 * One key, Allie's way. Returns null when the rotor does not take it (it goes on to the viewer's own keys: j k l Enter in a kind are
 * the viewer's), else `{state, fx}`: `fx.view` enter that kind, `fx.leave` go back to where you were, `fx.drawer` "open" | "close" |
 * "draw", `fx.say` words for the live region. `from` is where you are now (`{hash, cur, title}`), kept when a kind is entered.
 */
export function rotorKey(s, key, from = null, acts = []) {
  const names = KINDS.map((k) => k.name);
  // `acts`: actions of the page itself, listed after the kinds in the drawer (the notification act, whose word follows state). Picking one
  // closes the drawer and returns `fx.act` = its id; it is not a kind, so no kind is entered and nothing is remembered as the last kind.
  const total = names.length + acts.length;
  const pick = (st, i) => (i < names.length ? enter(st, names[i]) : { state: { ...st, drawer: false }, fx: { drawer: "close", act: acts[i - names.length].id } });
  const say = (d) => (d < 0 ? "First kind" : "Last kind");
  const enter = (st, kind) => {
    if (kind === st.kind) return { state: { ...st, drawer: false }, fx: { drawer: "close", say: st.kind ? kindOf(kind).word : "" } };
    return { state: { ...st, drawer: false, kind, last: kind, from: st.kind ? st.from : from }, fx: { drawer: "close", view: kind, say: kindOf(kind).word } };
  };
  if (s.drawer) {
    const move = (d) => { const j = s.at + d; return j < 0 || j >= total ? { state: s, fx: { say: say(d) } } : { state: { ...s, at: j }, fx: { drawer: "draw" } }; };
    if (key === "j" || key === "ArrowDown" || key === "]") return move(1);
    if (key === "k" || key === "ArrowUp" || key === "[") return move(-1);
    if (key === "Enter") return pick(s, s.at);
    if (/^[1-9]$/.test(key) && Number(key) <= total) return pick(s, Number(key) - 1);
    if (key === "r") return enter(s, s.last);
    if (key === "Escape" || key === "h" || key === "ArrowLeft") return { state: { ...s, drawer: false }, fx: { drawer: "close", say: s.kind ? kindOf(s.kind).word : "" } };
    if (key === "Tab") return null; // Tab still leaves it, as anywhere
    return { state: s, fx: {} }; // nothing else reaches the page while it is open
  }
  if (key === "r") return { state: { ...s, drawer: true, at: Math.max(0, names.indexOf(s.kind || s.last)) }, fx: { drawer: "open" } };
  if (!s.kind) return null;
  if (key === "]" || key === "[") {
    const d = key === "]" ? 1 : -1, i = names.indexOf(s.kind) + d;
    return i < 0 || i >= names.length ? { state: s, fx: { say: say(d) } } : enter(s, names[i]);
  }
  if (key === "h" || key === "Escape" || key === "ArrowLeft" || key === "Backspace") return { state: { ...s, kind: "", from: null }, fx: { leave: s.from } };
  return null;
}
