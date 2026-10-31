// The DOM side of the viewer, from home's programs/fold-viewer/fold-ax.js (bdelanghe/home bf7517f), which stays there until home's
// TS fold is deleted (migration step 3). The tree itself now comes from the core (ax_json, src/ax.rs), at byte parity with that
// file's axOf; this derives the DOM from it and refuses any node that would not be accessible.

// AX first: a fold is described as an accessibility tree (role, name,
// children) and the DOM is derived from that tree alone. Each role maps to
// exactly one semantic element, roles that need a name refuse to render
// without one, and anything only drawn for the eye is a "decoration" that
// assistive tech never sees. So a fold the viewer can show is one a screen
// reader can read: there is no other path to the page. (viewer.test.ts
// puts every golden fold through it.)

// A fold is an object: its name as the heading, then ONE list (Bobby 10-28, 01M620P439: "this should say everything and the rest
// should be a list. because there should be nothing that isn't an object ... semantic nested scope always"). The list's rows are
// the object's properties (its own words, its link, when its data is from: plain rows, read in order) and then its child objects
// (numbered rows you go into). No paragraph, footer, status line or banner exists outside it: toDom refuses them.
const ELEMENT = {
  main: "main", navigation: "nav", heading: "h1", list: "ol", listitem: "li", link: "a", button: "button", form: "form", status: "span", text: "span", decoration: "span",
};
const NEEDS_NAME = new Set(["navigation", "heading", "list", "link", "button", "form"]);
// What a fold's main may hold, in order: the path (its scope, as links), its heading, its one list.
const MAIN_ROLES = ["navigation", "heading", "list"];

/** Derives the DOM from an accessibility tree; throws on any node that would not be accessible, or that is not part of an object. */
export function toDom(doc, ax, ids = { n: 0 }, row = null) {
  const tag = ELEMENT[ax.role];
  if (!tag) throw new Error(`fold-ax: no element for role "${ax.role}"`);
  if (NEEDS_NAME.has(ax.role) && !(ax.name ?? "").trim()) throw new Error(`fold-ax: a ${ax.role} needs a name`);
  if (ax.role === "main") {
    const roles = (ax.children ?? []).map((c) => c.role);
    const shape = roles.filter((r) => r !== "navigation");
    if (shape.join(",") !== "heading,list" || roles.some((r) => !MAIN_ROLES.includes(r)))
      throw new Error(`fold-ax: a fold is its heading and one list, not ${roles.join(", ")}`);
  }
  const e = doc.createElement(tag);
  // A node's own id, as a page writes it (src/page.rs): its path under the document, an element id and never the data's identity.
  // A row is the RDFa subject, a WebPageElement whose schema:about is the IRI of the thing it shows (brief item 20): the element
  // presents that thing, it is not it. The about is an empty span's `resource`, since the row's own link goes to a page, not to
  // the thing, and a body <link> is one lone reads as a nameless link.
  if (ax.id) e.id = ax.id;
  if (ax.haecceity) {
    if (ax.id) e.setAttribute("resource", `#${ax.id}`);
    e.setAttribute("typeof", "schema:WebPageElement");
    const s = doc.createElement("span"); s.setAttribute("property", "schema:about"); s.setAttribute("resource", ax.haecceity); e.append(s);
  }
  if (ax.cls) e.className = ax.cls;
  if (ax.role === "decoration") e.setAttribute("aria-hidden", "true");
  if (ax.role === "list" && (ax.children ?? []).some((c) => c.role !== "listitem")) throw new Error(`fold-ax: a list holds rows, not ${(ax.children ?? []).map((c) => c.role).join(", ")}`);
  if (ax.role === "list") { e.setAttribute("role", "list"); e.setAttribute("aria-label", ax.name); } // WebKit drops list semantics under list-style:none without it
  if (ax.role === "navigation") e.setAttribute("aria-label", ax.name);
  // An act (src/ax.rs act_form): named, so it is a form landmark; its cap, call and args ride as hidden fields, ahead of its button.
  if (ax.role === "form") {
    e.setAttribute("aria-label", ax.name); e.setAttribute("method", String(ax.method)); e.setAttribute("action", String(ax.action));
    for (const [k, v] of ax.fields ?? []) { const i = doc.createElement("input"); i.setAttribute("type", "hidden"); i.setAttribute("name", k); i.setAttribute("value", v); e.append(i); }
  }
  if (ax.role === "heading" && ax.focus) e.tabIndex = -1;
  if (ax.role === "link") e.href = ax.href;
  if (ax.role === "link" && ax.ping) e.setAttribute("ping", ax.ping);
  if (ax.role === "button") e.type = "submit"; // a button is only ever an act's, and submits its form
  if (ax.role === "status") e.setAttribute("role", "status");
  // Off-page links open a new tab: clax frames the page without allow-top-navigation, so a plain link there goes nowhere.
  // A link to another fold's page (`local`, src/page.rs) is a move within the fold, as a `#` one is.
  if (ax.role === "link" && !ax.href.startsWith("#") && !ax.local) { e.target = "_blank"; e.rel = "noopener"; }
  if (ax.index != null) e.dataset.i = ax.index;
  if (ax.name && !(ax.children ?? []).length && ax.role !== "list" && ax.role !== "navigation") e.textContent = ax.name;
  for (const c of ax.children ?? []) { const x = toDom(doc, c, ids, ax.id || row); e.append(x); if (x._desc) e.append(x._desc); }
  if (ax.role === "link" && ax.description) {
    // The hint is read after the link's name, as its description, and shown under it.
    const d = doc.createElement("span"); d.className = "hint"; d.id = row ? `${row}:hint` : `d${++ids.n}`; d.textContent = ax.description;
    e.setAttribute("aria-describedby", d.id); e._desc = d;
  }
  return e;
}

/**
 * The live line for one seat, from a live snapshot ({ now, stale_s, seats: [{ seat, activity, thread?, heard, since }] }):
 * what it is doing, how long since it was heard, and whether that is an alert. No snapshot, or no row for the seat, is
 * "Status unknown.": never a made-up state. `down` (the stream itself dropped, at that time) reads as offline since then.
 */
export function liveLine(snap, seat, now = Date.now(), down = null) {
  const hhmm = (t) => new Date(t).toLocaleTimeString([], { hour: "2-digit", minute: "2-digit" });
  if (down != null) return { state: `Offline since ${hhmm(down)}.`, age: "", alert: true };
  const s = snap?.seats?.find((x) => x.seat === seat);
  if (!s) return { state: "Status unknown.", age: "", alert: false };
  const heard = Math.max(0, Math.round((now - Date.parse(s.heard)) / 1000));
  const quiet = heard > (snap.stale_s ?? Infinity) || s.activity === "offline" || s.activity === "stopped";
  const state = quiet ? `Offline since ${hhmm(s.since)}` : s.activity === "working" ? (s.thread ? "Working on your comment" : "Working") : "Listening";
  return { state: state + ".", age: `Last heard ${heard < 120 ? `${heard} s` : `${Math.round(heard / 60)} min`} ago.`, alert: quiet };
}
