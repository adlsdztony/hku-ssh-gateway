#!/usr/bin/env bash
# Exercise the CLI and embedded runtime without school credentials or SSH keys.
set -euo pipefail
ROOT=$(cd -- "$(dirname -- "${BASH_SOURCE[0]}")/.." && pwd)
BIN=${1:-$ROOT/target/debug/hku}
[[ -x $BIN ]] || { echo 'Build hku first, or pass its absolute path.' >&2; exit 1; }
TMP=$(mktemp -d)
PROJECT="hku-smoke-$$"
NETWORK="$PROJECT-probe"
SERVER="$PROJECT-peer"
hku() { "$BIN" --state-dir "$TMP/state" "$@"; }
compose() { docker compose --project-directory "$TMP/state" --env-file "$TMP/state/.env" -f "$TMP/state/compose.yaml" "$@"; }
proxy() { timeout 8 "$BIN" --state-dir "$TMP/state" proxy "$@"; }
cleanup() {
    docker rm -f "$SERVER" >/dev/null 2>&1 || true
    compose down --volumes >/dev/null 2>&1 || true
    docker network rm "$NETWORK" >/dev/null 2>&1 || true
    rm -rf -- "$TMP"
}
trap cleanup EXIT
for script in "$ROOT"/container/*.sh "$ROOT"/tests/*.sh "$ROOT/install.sh"; do bash -n "$script"; done
hku setup --user test@example.invalid --project "$PROJECT"
id=$(compose ps -q vpn)
image=$(docker inspect --format '{{.Image}}' "$id")
test -z "$(docker port "$id")"
ordinary_gateway=$(docker exec "$id" ip -4 route | awk '/^default / {print $3; exit}')
docker exec "$id" /usr/local/lib/hku/keepalive.sh --once | /bin/grep -q waiting-for-vpn
docker exec -e KEEPALIVE_ENABLED=false "$id" /usr/local/lib/hku/keepalive.sh --once | /bin/grep -q disabled
docker network create "$NETWORK" >/dev/null
docker run -d --name "$SERVER" --network "$NETWORK" --entrypoint /bin/bash "$image" \
    -c 'while true; do printf "HKU_OK\n" | nc -l -p 8080 -q 0; done' >/dev/null
docker network connect "$NETWORK" "$id"
peer=$(docker inspect --format '{{range .NetworkSettings.Networks}}{{.IPAddress}}{{end}}' "$SERVER")
docker exec "$id" timeout 5 nc -4 -z -w 3 "$peer" 8080
if proxy "$peer" 8080 </dev/null; then
    echo 'FAIL: proxy leaked traffic through the ordinary network' >&2; exit 1
fi
interface=$(docker exec "$id" ip -4 route get "$peer" | awk '{for(i=1;i<=NF;i++) if($i=="dev") {print $(i+1); exit}}')
[[ $interface != eth0 && -n $interface ]]
docker exec "$id" ip link set "$interface" down
docker exec "$id" ip link set "$interface" name tun0
docker exec "$id" ip link set tun0 up
test "$(proxy "$peer" 8080 </dev/null)" = HKU_OK
docker exec -e KEEPALIVE_HOST="$peer" -e KEEPALIVE_PORT=8080 "$id" /usr/local/lib/hku/keepalive.sh --once | /bin/grep -q ' ok '
docker exec -e KEEPALIVE_HOST="$ordinary_gateway" "$id" /usr/local/lib/hku/keepalive.sh --once | /bin/grep -q blocked-non-vpn-route
hku client-config --server test@192.0.2.1 --target other.hku.hk --target-user test --output "$TMP/client"
ssh -G -F "$TMP/client/hku-ssh.config" hku-target | /bin/grep '^proxycommand ' | /bin/grep -q 'ssh -T'
hku client-config --direct --output "$TMP/local-client"
ssh -G -F "$TMP/local-client/hku-ssh.config" -o HostName=other.hku.hk hku-vpn | /bin/grep '^proxycommand ' | /bin/grep -q 'proxy %h %p'
docker exec "$id" ip link set tun0 down
if proxy "$peer" 8080 </dev/null; then
    echo 'FAIL: proxy connected after VPN interface shutdown' >&2; exit 1
fi
compose restart vpn
compose up -d --wait --wait-timeout 60
hku disconnect
if hku status; then
    echo 'FAIL: status reported VPN online without authentication' >&2; exit 1
else
    test "$?" -eq 2
fi
printf '\nPASS: keyless setup, no exposed ports, VPN-only streams, probes, client configs and restart.\n'
