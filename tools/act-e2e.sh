#!/usr/bin/env bash
# act-e2e.sh <fold binary> <site dir>: an act end to end with no JavaScript (S2: allie-s2-fold-act-write §6). It starts `fold serve`
# acting for one login on a loopback port, with a stub chitin that records its argv. Then it uses curl to GET the item's page with
# tailscale serve's headers, read the ack form's token, and POST it: 303 to the fold with did=ack&as=ok, and the stub saw exactly
# one ack. The same POST again: 303 as=already. A typed say (Bobby 07c5f8dc) the same way: its line sent once, as one argument, and
# never logged. After the act map changes under the page: 409, and nothing more is sent. At the end the key and the act log are 0600.
# Then Needs you read live (--needs-as, src/needs.rs): needs.json lists the stub's waiting intent, never stored; a decline from
# another login, cross-site, as a form, or of an intent that isn't waiting is refused and declines nothing; the one that waits is
# declined once, and needs.json no longer lists it.
set -euo pipefail
fold=$1 src=$2
work=$(mktemp -d)
trap 'kill "$pid" 2>/dev/null || true; rm -rf "$work"' EXIT
cp -R "$src" "$work/site"
chmod -R u+w "$work/site"

# chitin's `ack` as it answers: `written <id>` the first time, a noop on the same move again
cat > "$work/chitin" <<'EOF'
#!/bin/sh
# Needs you's two (kept apart from the acts' record): one intent waits for bobby until it is declined
case "$1" in
  pending) echo "$*" >> "$STUB_RECORD.needs"
    if [ -f "$STUB_RECORD.declined" ]; then echo '[]'; else
      echo '[{"id":"0123456789ab","as":"bobby","to":"personal-assistant","text":"Route the plan.","text_hash":"deadbeefcafe","age_s":5,"queued_by":"stub","declined":false,"expired":false},{"id":"bbbbbbbbbbbb","as":"bobby","to":"seat-b","text":"x","declined":true,"expired":false}]'; fi
    exit 0 ;;
  decline) echo "$*" >> "$STUB_RECORD.needs"; touch "$STUB_RECORD.declined"; echo "$2 declined"; exit 0 ;;
esac
echo "$*" >> "$STUB_RECORD"
if [ "$(grep -cxF -- "$*" "$STUB_RECORD")" -gt 1 ]; then echo "refused: already acting"; exit 1; fi
echo "written 0001"
EOF
chmod +x "$work/chitin"
export STUB_RECORD=$work/record
: > "$STUB_RECORD"

login=bobby@example.com host=phobos.example.ts.net
"$fold" serve --root "$work/site" --mount /fold --port 0 \
  --act-as "$login" --act-host "$host" --act-key "$work/state/act.key" --chitin "$work/chitin" \
  --act-seat fold-ack --needs-as bobby 2> "$work/serve.log" &
pid=$!
for _ in $(seq 50); do grep -q ' on 127.0.0.1:' "$work/serve.log" && break; sleep 0.1; done
port=$(grep -o ' on 127.0.0.1:[0-9]*' "$work/serve.log" | cut -d: -f2)
[ -n "$port" ] || { cat "$work/serve.log"; echo "fold serve did not start"; exit 1; }
base=http://127.0.0.1:$port
serve=(-H "Tailscale-User-Login: $login" -H "X-Forwarded-Proto: https" -H "X-Forwarded-Host: $host" -H "Sec-Fetch-Site: same-origin")

fail() { echo "act-e2e: $*"; cat "$work/serve.log"; exit 1; }
# token [fold] [act]: the token the fold's page carries for that act's form
token() {
  curl -fsS "${serve[@]}" "$base/${1:-m1}/" | grep -o "action=\"/fold/act/${1:-m1}/${2:-ack}\"><input type=\"hidden\" name=\"token\" value=\"[^\"]*\"" | cut -d'"' -f8
}
post() { curl -sS -o /dev/null -w '%{http_code} %{redirect_url}' "${serve[@]}" --data-urlencode "token=$1" --data "cap=chitin&call=ack" "$base/act/m1/ack"; }

tok=$(token)
[ -n "$tok" ] || fail "the page carried no token"
got=$(post "$tok")
[ "$got" = "303 $base/fold/m1/?did=ack&as=ok" ] || fail "first POST: $got"
[ "$(cat "$STUB_RECORD")" = "ack m-1 acting --to seat-a --as fold-ack --ref fold:m1/ack" ] || fail "chitin saw: $(cat "$STUB_RECORD")"
curl -fsS "${serve[@]}" "$base/m1/?did=ack&as=ok" | grep -q 'role="status">Acknowledged.</span>' || fail "the fold did not say Acknowledged."

got=$(post "$tok")
[ "$got" = "303 $base/fold/m1/?did=ack&as=already" ] || fail "second POST: $got"

# a typed say: the line is the stub's last argument, sent once, and as fold-act though --act-seat names fold-ack (Bobby 38e765d3)
line="Ship it, and say so: 100% (ok?)"
say() { curl -sS -o /dev/null -w '%{http_code} %{redirect_url}' "${serve[@]}" --data-urlencode "token=$1" --data "cap=chitin&call=say" --data-urlencode "args.line=$2" "$base/act/talk/say"; }
curl -fsS "${serve[@]}" "$base/talk/" | grep -q '<textarea aria-label="Your reply" name="args.line" required="" maxlength="96"></textarea><button' || fail "no text box before the say's button"
stok=$(token talk say)
[ -n "$stok" ] || fail "the say's page carried no token"
got=$(say "$stok" "$line")
[ "$got" = "303 $base/fold/talk/?did=say&as=ok" ] || fail "say: $got"
[ "$(tail -n 1 "$STUB_RECORD")" = "say --as fold-act --in-reply-to m-1 --meta ref=fold:talk/say seat-a $line" ] || fail "chitin saw: $(tail -n 1 "$STUB_RECORD")"
got=$(say "$stok" "$line")
[ "$got" = "303 $base/fold/talk/?did=say&as=already" ] || fail "say again: $got"
got=$(say "$(token talk say)" "café")
[ "${got%% *}" = 422 ] || fail "a non-ASCII say: $got"
[ "$(wc -l < "$STUB_RECORD" | tr -d ' ')" = 3 ] || fail "chitin saw more than one say: $(cat "$STUB_RECORD")"

# the item's act changes under the open page: its old token is refused, and nothing is sent
zeros=$(printf '0%.0s' $(seq 64))
awk -v z="$zeros" '/"fixity":/ { sub(/"fixity": "[0-9a-f]+"/, "\"fixity\": \"" z "\"") } 1' "$work/site/_acts.json" > "$work/acts.json"
mv "$work/acts.json" "$work/site/_acts.json"
got=$(post "$tok")
[ "${got%% *}" = 409 ] || fail "after the change: $got"
[ "$(wc -l < "$STUB_RECORD" | tr -d ' ')" = 3 ] || fail "chitin was called after a refusal: $(cat "$STUB_RECORD")"

mode() { stat -c %a "$1" 2>/dev/null || stat -f %Lp "$1"; }
[ "$(mode "$work/state/act.key")" = 600 ] || fail "act.key is $(mode "$work/state/act.key")"
[ "$(mode "$work/state/acts.jsonl")" = 600 ] || fail "acts.jsonl is $(mode "$work/state/acts.jsonl")"
[ "$(wc -l < "$work/state/acts.jsonl" | tr -d ' ')" = 6 ] || fail "acts.jsonl: $(cat "$work/state/acts.jsonl")"
grep -q "$tok" "$work/state/acts.jsonl" "$work/serve.log" && fail "a token was logged"
grep -qF "Ship it" "$work/state/acts.jsonl" "$work/serve.log" && fail "a typed line was logged"
# Needs you, live
needs() { curl -sS -D "$work/needs.h" "$@" "$base/needs.json"; }
body=$(needs "${serve[@]}")
echo "$body" | grep -q '"waiting":\[{"id":"0123456789ab","as":"bobby"' || fail "needs.json: $body"
echo "$body" | grep -q bbbbbbbbbbbb && fail "needs.json listed a declined intent: $body"
grep -qi '^cache-control: no-store' "$work/needs.h" || fail "needs.json may be stored: $(cat "$work/needs.h")"
grep -qi '^etag' "$work/needs.h" && fail "needs.json has a validator"
code=$(curl -sS -o /dev/null -w '%{http_code}' -H "Tailscale-User-Login: eve@example.com" -H "X-Forwarded-Proto: https" -H "X-Forwarded-Host: $host" -H "Sec-Fetch-Site: same-origin" "$base/needs.json")
[ "$code" = 403 ] || fail "needs.json for another login: $code"
dec() { curl -sS -o "$work/dec.out" -w '%{http_code}' "$@"; }
j=(-H "Content-Type: application/json" --data '{}')
[ "$(dec -H "Tailscale-User-Login: eve@example.com" -H "X-Forwarded-Proto: https" -H "X-Forwarded-Host: $host" -H "Sec-Fetch-Site: same-origin" "${j[@]}" "$base/decline/0123456789ab")" = 403 ] || fail "a decline from another login"
[ "$(dec -H "Tailscale-User-Login: $login" -H "X-Forwarded-Proto: https" -H "X-Forwarded-Host: $host" -H "Sec-Fetch-Site: cross-site" "${j[@]}" "$base/decline/0123456789ab")" = 403 ] || fail "a cross-site decline"
[ "$(dec "${serve[@]}" --data "x=1" "$base/decline/0123456789ab")" = 415 ] || fail "a decline as a form"
[ "$(dec "${serve[@]}" "${j[@]}" "$base/decline/bbbbbbbbbbbb")" = 409 ] || fail "a decline of one already declined: $(cat "$work/dec.out")"
grep -q '^decline' "$STUB_RECORD.needs" && fail "chitin declined after a refusal: $(cat "$STUB_RECORD.needs")"
[ "$(dec "${serve[@]}" "${j[@]}" "$base/decline/0123456789ab")" = 200 ] || fail "the decline: $(cat "$work/dec.out")"
grep -q '"ok":true' "$work/dec.out" || fail "the decline said: $(cat "$work/dec.out")"
[ "$(grep -c '^decline' "$STUB_RECORD.needs")" = 1 ] || fail "chitin declined: $(cat "$STUB_RECORD.needs")"
grep -qx 'decline 0123456789ab' "$STUB_RECORD.needs" || fail "chitin declined: $(cat "$STUB_RECORD.needs")"
body=$(needs "${serve[@]}")
echo "$body" | grep -q '"waiting":\[\]' || fail "needs.json after the decline: $body"
grep -q '"decline":"0123456789ab".*"result":"ok"' "$work/state/acts.jsonl" || fail "the decline left no log line: $(cat "$work/state/acts.jsonl")"
grep -qF "Route the plan" "$work/state/acts.jsonl" "$work/serve.log" && fail "an intent's text was logged"
echo "act-e2e: ok (303 ok, 303 already, a say sent once and a non-ASCII one refused, 409 changed; key and log 0600; no token or line logged; needs.json live and never stored, a decline refused four ways and done once)"
