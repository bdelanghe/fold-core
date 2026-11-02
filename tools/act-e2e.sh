#!/usr/bin/env bash
# act-e2e.sh <fold binary> <site dir>: an act end to end with no JavaScript (S2: allie-s2-fold-act-write §6). It starts `fold serve`
# acting for one login on a loopback port, with a stub chitin that records its argv. Then it uses curl to GET the item's page with
# tailscale serve's headers, read the ack form's token, and POST it: 303 to the fold with did=ack&as=ok, and the stub saw exactly
# one ack. The same POST again: 303 as=already. A typed say (Bobby 07c5f8dc) the same way: its line sent once, as one argument, and
# never logged. After the act map changes under the page: 409, and nothing more is sent. At the end the key and the act log are 0600.
# Then Needs you read live (--needs-as, src/needs.rs): needs.json lists the stub's waiting intent, never stored, and is refused to
# another login; a decline posted here is refused unread (413), since a decline is a passkey verdict at chitin-approve, and chitin is not run.
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
  decline) echo "$*" >> "$STUB_RECORD.needs"; echo "$2 declined"; exit 0 ;;
  notify) echo "$*" >> "$STUB_RECORD.needs"; echo "written n-1"; exit 0 ;;
  push) echo "$*" >> "$STUB_RECORD.needs"
    case "$2" in vapid-public) echo "BPubKey_0-1" ;; subscribe) cat > "$STUB_RECORD.sub"; echo subscribed ;; unsubscribe) echo removed ;; esac
    exit 0 ;;
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
  --act-seat fold-ack --needs-as bobby --notify-as fold-notify 2> "$work/serve.log" &
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
[ "$(grep -c '"act":' "$work/state/acts.jsonl")" = 6 ] || fail "acts.jsonl: $(cat "$work/state/acts.jsonl")"  # the act lines (the watcher's notify line is beside them)
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
# no unsigned decline: a decline is the passkey's, at chitin-approve (BRIEF addendum 1), so this server has no route for one
code=$(curl -sS -o /dev/null -w '%{http_code}' "${serve[@]}" -H "Content-Type: application/json" --data '{}' "$base/decline/0123456789ab")
[ "$code" = 413 ] || fail "a decline posted here: $code (no route takes a body there, so it is refused unread)"
grep -q '^decline' "$STUB_RECORD.needs" && fail "chitin declined: $(cat "$STUB_RECORD.needs")"
# notifications (BRIEF addendum 2): the watcher's first look sent one Needs-you record for the one intent that waits, none for the
# declined one; a device's subscription goes to chitin cut to endpoint and keys, and comes off again
for _ in $(seq 50); do grep -q '^notify' "$STUB_RECORD.needs" && break; sleep 0.1; done
[ "$(grep -c '^notify' "$STUB_RECORD.needs")" = 1 ] || fail "notify: $(cat "$STUB_RECORD.needs")"
grep -qx "notify --as fold-notify --severity attention --dedupe needs-you:sign-0123456789ab --url https://$host/fold/needs/approvals/needs-sign-0123456789ab/ To personal-assistant" "$STUB_RECORD.needs" || fail "notify: $(cat "$STUB_RECORD.needs")"
echo "$(needs "${serve[@]}")" | grep -q '"push":true' || fail "needs.json does not say notifications are on"
[ "$(curl -sS "${serve[@]}" "$base/push/key")" = '{"key":"BPubKey_0-1"}' ] || fail "push/key"
j=(-H "Content-Type: application/json")
got=$(curl -sS "${serve[@]}" "${j[@]}" --data '{"endpoint":"https://web.push.apple.com/abc","keys":{"p256dh":"P","auth":"A"},"extra":1}' "$base/push/subscribe")
[ "$got" = '{"ok":true,"said":"subscribed"}' ] || fail "push/subscribe: $got"
[ "$(cat "$STUB_RECORD.sub")" = '{"endpoint":"https://web.push.apple.com/abc","keys":{"p256dh":"P","auth":"A"}}' ] || fail "chitin got: $(cat "$STUB_RECORD.sub")"
code=$(curl -sS -o /dev/null -w '%{http_code}' -H "Tailscale-User-Login: eve@example.com" -H "X-Forwarded-Proto: https" -H "X-Forwarded-Host: $host" -H "Sec-Fetch-Site: same-origin" "${j[@]}" --data '{"endpoint":"https://web.push.apple.com/evil","keys":{"p256dh":"P","auth":"A"}}' "$base/push/subscribe")
[ "$code" = 403 ] || fail "a subscription from another login: $code"
[ "$(curl -sS "${serve[@]}" "${j[@]}" --data '{"endpoint":"https://web.push.apple.com/abc"}' "$base/push/unsubscribe")" = '{"ok":true,"said":"removed"}' ] || fail "push/unsubscribe"
[ "$(grep -c '^push subscribe' "$STUB_RECORD.needs")" = 1 ] || fail "chitin subscribed: $(cat "$STUB_RECORD.needs")"
grep -q '"notify":"0123456789ab".*"result":"ok"' "$work/state/acts.jsonl" || fail "the notify left no log line: $(cat "$work/state/acts.jsonl")"
grep -q "web.push.apple.com/abc" "$work/state/acts.jsonl" && fail "an endpoint's path was logged"
grep -qF "Route the plan" "$work/state/acts.jsonl" "$work/serve.log" && fail "an intent's text was logged"
echo "act-e2e: ok (303 ok, 303 already, a say sent once and a non-ASCII one refused, 409 changed; key and log 0600; no token or line logged; needs.json live and never stored, no unsigned decline; one notify per new intent, a device subscribed and unsubscribed through chitin)"
