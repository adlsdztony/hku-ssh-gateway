#!/usr/bin/env bash
# Verify automatic console return without reading real credentials or VPN data.
set -euo pipefail
ROOT=$(cd -- "$(dirname -- "${BASH_SOURCE[0]}")/.." && pwd)
BIN=${1:-$ROOT/target/debug/hku}
TMP=$(mktemp -d)
PROJECT="hku-connect-test-$$"
export HKU_TEST_BIN="$BIN" HKU_TEST_STATE="$TMP/state"
compose() { docker compose --project-directory "$TMP/state" --env-file "$TMP/state/.env" -f "$TMP/state/compose.yaml" "$@"; }
cleanup() {
    compose down --volumes >/dev/null 2>&1 || true
    rm -rf -- "$TMP"
}
trap cleanup EXIT
"$BIN" --state-dir "$TMP/state" setup --backend docker --user test@example.invalid --project "$PROJECT" --no-keepalive
id=$(compose ps -q vpn)
cc -static -O2 -Wall -Wextra -Werror "$ROOT/tests/mock-openconnect.c" -o "$TMP/openconnect"
docker cp "$TMP/openconnect" "$id:/usr/sbin/openconnect"
docker exec "$id" chmod 755 /usr/sbin/openconnect
cat > "$TMP/run-connect" <<'CONNECT'
#!/bin/bash
before=$(stty -g)
"$HKU_TEST_BIN" --state-dir "$HKU_TEST_STATE" connect
result=$?
after=$(stty -g)
if [[ $before != "$after" ]]; then echo 'Terminal settings were not restored.' >&2; exit 90; fi
touch "$HKU_TEST_STATE/command-returned"
exit "$result"
CONNECT
chmod 755 "$TMP/run-connect"
login() {
    rm -f "$TMP/state/command-returned"
    # Input is supplied only after each terminal has time to display its prompt.
    # Keep stdin open until the CLI returns, so EOF cannot fake auto-detachment.
    { sleep 1; printf '\n'; sleep 1; printf 'fixture-password\n'; sleep 1; printf 'fixture-otp\n';
      for attempt in {1..50}; do
          if [[ -f $TMP/state/command-returned ]]; then exit 0; fi
          sleep 0.1
      done
      echo 'FAIL: command required an extra input/EOF to return.' >&2; exit 1;
    } |
        timeout 20 script -q -e -c "$TMP/run-connect" /dev/null
}
login > "$TMP/success.log" 2>&1 || { cat "$TMP/success.log" >&2; exit 1; }
grep -q 'VPN connected. It will keep running' "$TMP/success.log"
if grep -qE 'fixture-password|fixture-otp' "$TMP/success.log"; then
    echo 'FAIL: terminal input was echoed' >&2; exit 1
fi
sleep 1
docker exec "$id" /usr/local/lib/hku/connection-state.sh
timeout 8 script -q -e -c "$TMP/run-connect" /dev/null </dev/null > "$TMP/already.log" 2>&1
grep -q 'VPN is already connected' "$TMP/already.log"
"$BIN" --state-dir "$TMP/state" disconnect
for attempt in {1..20}; do
    if ! docker exec "$id" /usr/local/lib/hku/connection-state.sh; then break; fi
    sleep 0.1
done
docker exec "$id" touch /run/hku-test-auth-fail
if login > "$TMP/failure.log" 2>&1; then
    echo 'FAIL: failed authentication was reported as connected' >&2; exit 1
fi
grep -q 'VPN login ended before the tunnel was ready' "$TMP/failure.log" || { cat "$TMP/failure.log" >&2; exit 1; }
docker exec "$id" rm /run/hku-test-auth-fail
login > "$TMP/retry.log" 2>&1 || { cat "$TMP/retry.log" >&2; exit 1; }
docker exec "$id" /usr/local/lib/hku/connection-state.sh
printf 'PASS: automatic return, preserved VPN, terminal restoration, hidden input, failure and retry.\n'
