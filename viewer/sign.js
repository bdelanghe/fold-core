// The sign acts, in the page (BRIEF-stint-2026-11-02-approve-into-fold and its addendum 1; RECOMMEND-consultant-2026-11-01-reversible-
// no-approval-and-sign-in-basis S3b): a Needs-you item's two verdicts, Approve and Decline, are each a passkey, run here against
// chitin-approve's own API (its PROTOCOL.md; the decline is addendum 1's, "the same challenge/assertion flow"):
//   approve: GET /approve/v1/challenge/<id>, POST /approve/v1/assertion/<id>
//   decline: GET /approve/v1/decline/<id>,   POST /approve/v1/decline/<id>
// Each challenge answers {ok, intent: {id, to, text, text_hash?}, frame?: {hash}, verdict?, publicKey}; an assertion {ok, message} or
// {ok: false, reason}. chitin binds each assertion to its verdict and the item's h, so one can't be replayed as the other.
// Find/Say/Do: signing is the Do barrier. What is signed is never read from the document the page folds: the verdict, recipient,
// text and h said before the passkey are the challenge's own, the answer the passkey signs, and the ceremony stops when they are not
// what the item showed, or when a decline's challenge does not say it is one. The endpoints are built from the intent's id (12 hex,
// chitin's), never taken from a document. The seats (and chitin, for a decline) verify the assertion; nothing here holds a key.

/** chitin's intent id (pending.ts `ID`). */
export const intentOk = (id) => typeof id === "string" && /^[0-9a-f]{12}$/.test(id);
/** Each verdict's challenge and assertion paths (src/needs.rs `endpoints`: the same table). */
export const ENDPOINTS = {
  approve: (id) => [`/approve/v1/challenge/${id}`, `/approve/v1/assertion/${id}`],
  decline: (id) => [`/approve/v1/decline/${id}`, `/approve/v1/decline/${id}`],
};
const DONE = { approve: (to) => `Sent to ${to}.`, decline: () => "Declined." };
const VERB = { approve: "Approving", decline: "Declining" };

/** base64url to bytes, and bytes to base64url (chitin-approve's page's `dec`/`enc`). */
export const dec = (s) => { const b = atob(s.replace(/-/g, "+").replace(/_/g, "/") + "===".slice((s.length + 3) % 4)); const u = new Uint8Array(b.length); for (let i = 0; i < b.length; i++) u[i] = b.charCodeAt(i); return u.buffer; };
export const enc = (buf) => { let s = ""; for (const c of new Uint8Array(buf)) s += String.fromCharCode(c); return btoa(s).replace(/\+/g, "-").replace(/\//g, "_").replace(/=+$/, ""); };

const TRIES = 3;
const answerOf = async (r) => { try { return await r.json(); } catch { return { ok: false, reason: `the gateway answered ${r.status}` }; } };
/** chitin-approve's retry rule: a 429 (every chitin slot busy; chitin never ran) is asked again, and so is a challenge chitin was too
 * slow on (504); an assertion is never asked again after a 504, since that run may have spent it. */
async function again(send, also, wait) {
  for (let n = 1; ; n++) {
    const r = await send();
    if (n >= TRIES || !(r.status === 429 || r.status === also)) return answerOf(r);
    await wait(1500 * n);
  }
}

/** Why a ceremony stopped, in words. */
export const why = (e) => (e && e.name === "NotAllowedError") ? "the passkey was cancelled or timed out" : typeof e === "string" ? e : (e && e.message) || String(e);

/** The item's h, as the challenge gives it: its frame's hash (approve), else the intent's text hash. */
const hOf = (c) => (typeof c.frame?.hash === "string" ? c.frame.hash : typeof c.intent?.text_hash === "string" ? c.intent.text_hash : null);

/**
 * One verdict on one intent with the passkey: `verdict` is "approve" or "decline". `shown` is what the item showed ({to, text});
 * `say(line)` is told the challenge's own words before the passkey is asked. Resolves to {ok: true, said} or {ok: false, said,
 * changed?}. Nothing is signed when the challenge's intent is not the one shown, or its verdict is not the one asked for.
 */
export async function sign(id, verdict, shown, { fetch = globalThis.fetch, credentials = globalThis.navigator?.credentials, say = () => {}, wait = (ms) => new Promise((z) => setTimeout(z, ms)) } = {}) {
  const not = verdict === "decline" ? "Not declined" : "Not signed";
  if (typeof verdict !== "string" || !Object.hasOwn(ENDPOINTS, verdict)) return { ok: false, said: "Not signed: that is not a verdict." };
  if (!intentOk(id)) return { ok: false, said: `${not}: that is not an intent.` };
  const [challenge, assertion] = ENDPOINTS[verdict](id);
  try {
    const c = await again(() => fetch(challenge, { cache: "no-store" }), 504, wait);
    if (!c.ok) throw c.reason ?? "the challenge was refused";
    // a decline's challenge must say it is one (addendum 1's binding); an approve's may say so, and may not say otherwise
    if (verdict === "decline" ? c.verdict !== "decline" : c.verdict !== undefined && c.verdict !== "approve") throw `the challenge is not for ${verdict === "decline" ? "a decline" : "an approval"}`;
    const it = c.intent ?? {}, h = hOf(c);
    if (it.id !== undefined && it.id !== id) throw "the challenge is for another intent";
    if (typeof it.text !== "string" || typeof it.to !== "string" || !h) throw "the challenge did not say what it signs";
    if (it.to !== shown?.to || it.text !== shown?.text) return { ok: false, changed: true, said: `${not}: this changed since you opened it. Read it again: to ${it.to}, “${it.text}”.` };
    say(`${VERB[verdict]} for ${it.to}, #${h}: “${it.text}”. Your passkey asks next.`);
    const pk = c.publicKey;
    const cred = await credentials.get({ publicKey: { challenge: dec(pk.challenge), rpId: pk.rpId, allowCredentials: (pk.allowCredentials ?? []).map((x) => ({ type: "public-key", id: dec(x.id) })), userVerification: "required", timeout: pk.timeout } });
    const r = cred.response;
    const body = { id: cred.id, rawId: enc(cred.rawId), type: cred.type, response: { clientDataJSON: enc(r.clientDataJSON), authenticatorData: enc(r.authenticatorData), signature: enc(r.signature), userHandle: r.userHandle ? enc(r.userHandle) : null } };
    const v = await again(() => fetch(assertion, { method: "POST", cache: "no-store", headers: { "content-type": "application/json" }, body: JSON.stringify(body) }), 0, wait);
    if (!v.ok) throw v.reason ?? "the assertion was refused";
    return { ok: true, said: `${DONE[verdict](it.to)}${v.message ? ` ${v.message}` : ""}` };
  } catch (e) {
    return { ok: false, said: `${not}: ${why(e)}.` };
  }
}

/** What fold serve's needs.json answered, as the core's needs entry takes it: {needs} or {error}. Never throws, and never waits past
 * `wait` ms: with the host asleep the page still opens on its kept copy, saying nothing here can be signed. */
export async function readNeeds(url, { fetch = globalThis.fetch, wait = 4000 } = {}) {
  try {
    const r = await fetch(url, { cache: "no-store", signal: AbortSignal.timeout(wait) });
    if (!r.ok) return { error: `${r.status} ${(await r.text().catch(() => "")).trim() || r.statusText || ""}`.trim() };
    return { needs: await r.json() };
  } catch (e) {
    return { error: why(e) };
  }
}

/**
 * Notifications on this device (BRIEF addendum 2): `call` "subscribe" asks the browser for permission and a push subscription with
 * fold serve's VAPID public key (`push/key`), and hands it to `push/subscribe`, which hands it to chitin; "unsubscribe" takes it off
 * both. `base` is the viewer's own URL (the push routes are beside it). Resolves to {ok, said}. Never throws. On an iPhone a web page
 * gets pushes only once it is on the Home Screen: then PushManager is there.
 */
export async function notifyHere(call, base, { navigator: nav = globalThis.navigator, fetch = globalThis.fetch, Notification: N = globalThis.Notification, PushManager: P = globalThis.PushManager } = {}) {
  const url = (p) => new URL(p, base).href;
  const post = async (p, body) => answerOf(await fetch(url(p), { method: "POST", cache: "no-store", headers: { "content-type": "application/json" }, body: JSON.stringify(body) }));
  try {
    if (!nav?.serviceWorker || !P || !N) return { ok: false, said: "This browser can't notify you from this page. On an iPhone, add Basis to the Home Screen and open it from there." };
    const reg = await nav.serviceWorker.ready, had = await reg.pushManager.getSubscription();
    if (call === "unsubscribe") {
      if (!had) return { ok: true, said: "This device wasn't being notified." };
      const v = await post("push/unsubscribe", { endpoint: had.endpoint });
      await had.unsubscribe();
      return v.ok ? { ok: true, said: "This device won't be notified any more." } : { ok: false, said: `Stopped on this device, but the server said: ${v.reason ?? "no"}.` };
    }
    if (call !== "subscribe") return { ok: false, said: "That is not a notification act." };
    if ((await N.requestPermission()) !== "granted") return { ok: false, said: "Notifications are off for this site in the browser, so nothing changed." };
    const k = await answerOf(await fetch(url("push/key"), { cache: "no-store" }));
    if (typeof k.key !== "string") return { ok: false, said: `Not on: ${k.error ?? k.reason ?? "no key"}.` };
    const sub = had ?? (await reg.pushManager.subscribe({ userVisibleOnly: true, applicationServerKey: dec(k.key) }));
    const v = await post("push/subscribe", sub.toJSON ? sub.toJSON() : sub);
    return v.ok ? { ok: true, said: "This device will be notified when something waits for your signature." } : { ok: false, said: `Not on: ${v.reason ?? "refused"}.` };
  } catch (e) {
    return { ok: false, said: `Not on: ${why(e)}.` };
  }
}
