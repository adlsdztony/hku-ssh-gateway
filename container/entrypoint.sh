#!/bin/bash
set -euo pipefail
source /usr/local/lib/hku/common.sh
validate_config
for firewall in iptables ip6tables; do
    "$firewall" -N HKU_JUMP_OUTPUT
    "$firewall" -A HKU_JUMP_OUTPUT -o lo -j ACCEPT
    "$firewall" -A HKU_JUMP_OUTPUT -o tun0 -j ACCEPT
    "$firewall" -A HKU_JUMP_OUTPUT -j REJECT
    "$firewall" -I OUTPUT 1 -m owner --uid-owner 1100 -j HKU_JUMP_OUTPUT
done
/usr/local/lib/hku/keepalive.sh &
probe_pid=$!
printf '%s\n' "$probe_pid" > /run/hku-keepalive.pid
rm -f /run/hku-openconnect.pid /run/hku-vpn.exit
/usr/local/lib/hku/console.sh </dev/tty &
console_pid=$!
printf '%s\n' "$console_pid" > /run/hku-console.pid
cleanup() {
    trap - TERM INT
    pkill -INT -x openconnect 2>/dev/null || true
    kill -TERM "$console_pid" "$probe_pid" 2>/dev/null || true
    wait "$console_pid" "$probe_pid" 2>/dev/null || true
}
trap 'cleanup; exit 0' TERM INT
set +e
wait -n "$probe_pid" "$console_pid"
result=$?
cleanup
if (( result == 0 )); then result=1; fi
exit "$result"
