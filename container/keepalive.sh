#!/bin/bash
set -euo pipefail
source /usr/local/lib/hku/common.sh
validate_config
mkdir -p /run/hku-keepalive
last_success=never
failures=0
previous_state=initial
pause_pid=
stop() {
    trap - TERM INT
    if [[ -n $pause_pid ]]; then kill "$pause_pid" 2>/dev/null || true; fi
    exit 0
}
trap stop TERM INT
write_status() {
    local state=$1 now
    now=$(date -u +%FT%TZ)
    {
        printf 'state=%s\nlast_check=%s\nlast_success=%s\nconsecutive_failures=%s\n' "$state" "$now" "$last_success" "$failures"
        printf 'target=%s:%s\ninterval_seconds=%s\n' "$KEEPALIVE_HOST" "$KEEPALIVE_PORT" "$KEEPALIVE_INTERVAL"
    } > /run/hku-keepalive/status.tmp
    mv /run/hku-keepalive/status.tmp /run/hku-keepalive/status
    if [[ $state != "$previous_state" ]]; then
        printf '[keepalive] %s %s (last_success=%s)\n' "$now" "$state" "$last_success"
        previous_state=$state
    fi
}
probe() {
    local address route
    # Do not resolve a host or send a probe before the VPN is ready.
    ip -4 address show dev tun0 2>/dev/null | /bin/grep -q 'inet ' || return 2
    address=$(runuser -u jump -- timeout "$KEEPALIVE_TIMEOUT" getent ahostsv4 "$KEEPALIVE_HOST" | awk 'NR==1 {print $1}') || return 1
    [[ -n $address ]] || return 1
    route=$(ip -4 route get "$address") || return 1
    [[ " $route " == *' dev tun0 '* ]] || return 3
    # A TCP handshake creates actual tunneled data, without an SSH login.
    # The jump UID is also subject to the VPN-only OUTPUT firewall.
    runuser -u jump -- timeout "$((KEEPALIVE_TIMEOUT + 1))" nc -4 -z -w "$KEEPALIVE_TIMEOUT" "$address" "$KEEPALIVE_PORT" >/dev/null 2>&1
}
while true; do
    if [[ $KEEPALIVE_ENABLED == false ]]; then
        write_status disabled
    elif probe; then
        last_success=$(date -u +%FT%TZ)
        failures=0
        write_status ok
    else
        result=$?
        if (( result == 2 )); then
            write_status waiting-for-vpn
        else
            failures=$((failures + 1))
            if (( result == 3 )); then write_status blocked-non-vpn-route; else write_status probe-failed; fi
        fi
    fi
    if [[ ${1:-} == --once ]]; then break; fi
    # Check VPN readiness frequently, without sending more frequent probes.
    delay=$KEEPALIVE_INTERVAL
    if [[ $previous_state == waiting-for-vpn ]]; then delay=5; fi
    sleep "$delay" &
    pause_pid=$!
    wait "$pause_pid" || true
    pause_pid=
done
