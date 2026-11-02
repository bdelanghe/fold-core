// The sign act, in the page (BRIEF-stint-2026-11-02-approve-into-fold; RECOMMEND-consultant-2026-11-01-reversible-no-approval-and-
// sign-in-basis S3b): a Needs-you item's passkey runs here, against chitin-approve's own API, unchanged (its PROTOCOL.md):
//   GET  /approve/v1/challenge/<id>  -> {ok, intent: {to, text, …}, frame: {hash, …}, publicKey}
//   POST /approve/v1/assertion/<id>  -> {ok, message} | {ok: false, reason}
// Find/Say/Do: signing is the Do barrier. What is signed is never read from the document the page folds: the text, the recipient
// and the hash said before the passkey are the challenge's own, the answer the passkey signs, and the ceremony stops when they are
// not what the item showed. The endpoints are built from the intent's id (12 hex, chitin's), never taken from a document. The
// seats verify the assertion themselves; nothing here holds a key.
// The decline is fold serve's (src/serve.rs `decline`): unsigned, as declining narrows.

/** chitin's intent id (pending.ts `ID`). */
export const intentOk = (id) => typeof id === "string" && /^[0-9a-f]{12}$/.test(id);
export const challengeUrl = (id) => `/approve/v1/challenge/${id}`;
export const assertionUrl = (id) => `/approve/v1/assertion/${id}`;

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

/**
 * Approve one intent with the passkey. `shown` is what the item showed ({to, text}); `say(line)` is told each step, the challenge's
 * own words before the passkey is asked. Resolves to {ok: true, said} or {ok: false, said}. Nothing is signed when the challenge's
 * intent is not the one shown.
 */
export async function approve(id, shown, { fetch = globalThis.fetch, credentials = globalThis.navigator?.credentials, say = () => {}, wait = (ms) => new Promise((z) => setTimeout(z, ms)) } = {}) {
  if (!intentOk(id)) return { ok: false, said: "Not signed: that is not an intent." };
  try {
    const c = await again(() => fetch(challengeUrl(id), { cache: "no-store" }), 504, wait);
    if (!c.ok) throw c.reason ?? "the challenge was refused";
    const it = c.intent ?? {}, hash = c.frame?.hash;
    if (it.id !== undefined && it.id !== id) throw "the challenge is for another intent";
    if (typeof it.text !== "string" || typeof it.to !== "string" || typeof hash !== "string") throw "the challenge did not say what it signs";
    if (it.to !== shown?.to || it.text !== shown?.text) return { ok: false, changed: true, said: `Not signed: this changed since you opened it. Read it again: to ${it.to}, “${it.text}”.` };
    say(`Signing for ${it.to}, #${hash}: “${it.text}”. Your passkey asks next.`);
    const pk = c.publicKey;
    const cred = await credentials.get({ publicKey: { challenge: dec(pk.challenge), rpId: pk.rpId, allowCredentials: (pk.allowCredentials ?? []).map((x) => ({ type: "public-key", id: dec(x.id) })), userVerification: "required", timeout: pk.timeout } });
    const r = cred.response;
    const body = { id: cred.id, rawId: enc(cred.rawId), type: cred.type, response: { clientDataJSON: enc(r.clientDataJSON), authenticatorData: enc(r.authenticatorData), signature: enc(r.signature), userHandle: r.userHandle ? enc(r.userHandle) : null } };
    const v = await again(() => fetch(assertionUrl(id), { method: "POST", cache: "no-store", headers: { "content-type": "application/json" }, body: JSON.stringify(body) }), 0, wait);
    if (!v.ok) throw v.reason ?? "the assertion was refused";
    return { ok: true, said: `Sent to ${it.to}.${v.message ? ` ${v.message}` : ""}` };
  } catch (e) {
    return { ok: false, said: `Not signed: ${why(e)}.` };
  }
}

/** Decline one intent: fold serve's `decline/<id>` (`url` is it, under the viewer's mount), as JSON. */
export async function decline(id, url, { fetch = globalThis.fetch } = {}) {
  if (!intentOk(id)) return { ok: false, said: "Not declined: that is not an intent." };
  try {
    const v = await answerOf(await fetch(url, { method: "POST", cache: "no-store", headers: { "content-type": "application/json" }, body: "{}" }));
    return v.ok ? { ok: true, said: `Declined. ${v.message ?? ""}`.trim() } : { ok: false, said: `Not declined: ${v.reason ?? "refused"}.` };
  } catch (e) {
    return { ok: false, said: `Not declined: ${why(e)}.` };
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
