#!/usr/bin/env bash
# act-e2e.sh <fold binary> <site dir>: an act end to end with no JavaScript (S2: allie-s2-fold-act-write §6). It starts `fold serve`
# acting for one login on a loopback port, with a stub chitin that records its argv. Then it uses curl to GET the item's page with
# tailscale serve's headers, read the ack form's token, and POST it: 303 to the fold with did=ack&as=ok, and the stub saw exactly
# one ack. The same POST again: 303 as=already. After the act map changes under the page: 409, and nothing more is sent. At the end
# the key and the act log are 0600.
set -euo pipefail
fold=$1 src=$2
work=$(mktemp -d)
trap 'kill "$pid" 2>/dev/null || true; rm -rf "$work"' EXIT
cp -R "$src" "$work/site"
chmod -R u+w "$work/site"

# chitin's `ack` as it answers: `written <id>` the first time, a noop on the same move again
cat > "$work/chitin" <<'EOF'
#!/bin/sh
echo "$*" >> "$STUB_RECORD"
if [ "$(grep -cxF -- "$*" "$STUB_RECORD")" -gt 1 ]; then echo "refused: already acting"; exit 1; fi
echo "written 0001"
EOF
chmod +x "$work/chitin"
export STUB_RECORD=$work/record
: > "$STUB_RECORD"

login=bobby@example.com host=phobos.example.ts.net
"$fold" serve --root "$work/site" --mount /fold --port 0 \
  --act-as "$login" --act-host "$host" --act-key "$work/state/act.key" --chitin "$work/chitin" 2> "$work/serve.log" &
pid=$!
for _ in $(seq 50); do grep -q ' on 127.0.0.1:' "$work/serve.log" && break; sleep 0.1; done
port=$(grep -o ' on 127.0.0.1:[0-9]*' "$work/serve.log" | cut -d: -f2)
[ -n "$port" ] || { cat "$work/serve.log"; echo "fold serve did not start"; exit 1; }
base=http://127.0.0.1:$port
serve=(-H "Tailscale-User-Login: $login" -H "X-Forwarded-Proto: https" -H "X-Forwarded-Host: $host" -H "Sec-Fetch-Site: same-origin")

fail() { echo "act-e2e: $*"; cat "$work/serve.log"; exit 1; }
token() {
  curl -fsS "${serve[@]}" "$base/m1/" | grep -o 'action="/fold/act/m1/ack"><input type="hidden" name="token" value="[^"]*"' | cut -d'"' -f8
}
post() { curl -sS -o /dev/null -w '%{http_code} %{redirect_url}' "${serve[@]}" --data-urlencode "token=$1" --data "cap=ack" "$base/act/m1/ack"; }

tok=$(token)
[ -n "$tok" ] || fail "the page carried no token"
got=$(post "$tok")
[ "$got" = "303 $base/fold/m1/?did=ack&as=ok" ] || fail "first POST: $got"
[ "$(cat "$STUB_RECORD")" = "ack m-1 acting --to seat-a --as fold-act" ] || fail "chitin saw: $(cat "$STUB_RECORD")"
curl -fsS "${serve[@]}" "$base/m1/?did=ack&as=ok" | grep -q 'role="status">Acknowledged.</span>' || fail "the fold did not say Acknowledged."

got=$(post "$tok")
[ "$got" = "303 $base/fold/m1/?did=ack&as=already" ] || fail "second POST: $got"

# the item's act changes under the open page: its old token is refused, and nothing is sent
zeros=$(printf '0%.0s' $(seq 64))
awk -v z="$zeros" '/"fixity":/ { sub(/"fixity": "[0-9a-f]+"/, "\"fixity\": \"" z "\"") } 1' "$work/site/_acts.json" > "$work/acts.json"
mv "$work/acts.json" "$work/site/_acts.json"
got=$(post "$tok")
[ "${got%% *}" = 409 ] || fail "after the change: $got"
[ "$(wc -l < "$STUB_RECORD" | tr -d ' ')" = 2 ] || fail "chitin was called after a refusal: $(cat "$STUB_RECORD")"

mode() { stat -c %a "$1" 2>/dev/null || stat -f %Lp "$1"; }
[ "$(mode "$work/state/act.key")" = 600 ] || fail "act.key is $(mode "$work/state/act.key")"
[ "$(mode "$work/state/acts.jsonl")" = 600 ] || fail "acts.jsonl is $(mode "$work/state/acts.jsonl")"
[ "$(wc -l < "$work/state/acts.jsonl" | tr -d ' ')" = 3 ] || fail "acts.jsonl: $(cat "$work/state/acts.jsonl")"
grep -q "$tok" "$work/state/acts.jsonl" "$work/serve.log" && fail "a token was logged"
echo "act-e2e: ok (303 ok, 303 already, 409 changed; chitin saw one ack and its replay; key and log 0600)"
