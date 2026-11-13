#!/bin/bash
set -euo pipefail
source /usr/local/lib/hku/common.sh
validate_config
vpn_pid=
stop() {
    trap - TERM INT
    if [[ -n $vpn_pid ]]; then
        kill -INT "$vpn_pid" 2>/dev/null || true
        wait "$vpn_pid" 2>/dev/null || true
    fi
    exit 0
}
trap stop TERM INT
while true; do
    printf '\nHKU VPN login. Run hku connect to authenticate.\n'
    read -r -p 'Press Enter to start VPN login: ' || exit 0
    unset REPLY
    rm -f /run/hku-vpn.exit
    printf 'Account: %s\n' "$VPN_USER"
    # OpenConnect handles all credential prompts itself. The container keeps
    # its terminal alive after the attach client exits; no cookie is extracted.
    openconnect --protocol=anyconnect --interface=tun0 \
        --useragent="$VPN_USERAGENT" --user="$VPN_USER" \
        --script=/usr/share/vpnc-scripts/vpnc-script "$VPN_SERVER" </dev/tty &
    vpn_pid=$!
    printf '%s\n' "$vpn_pid" > /run/hku-openconnect.pid
    set +e
    wait "$vpn_pid"
    result=$?
    set -e
    vpn_pid=
    rm -f /run/hku-openconnect.pid
    printf '%s\n' "$result" > /run/hku-vpn.exit
    printf '\nVPN session ended (exit %s).\n' "$result"
done
