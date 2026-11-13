#!/bin/bash
set -euo pipefail
source /usr/local/lib/hku/common.sh
validate_config
printf '\nHKU OpenConnect console\nAccount: %s\n' "$VPN_USER"
printf 'After connecting: Ctrl-B, then D detaches and keeps the VPN running.\n'
printf 'Ctrl-C disconnects the VPN. Do not share passwords, OTPs or cookies.\n\n'
read -r -p 'Press Enter to start VPN login: ' answer || exit 0
# Stay in the foreground. --background inside docker exec -it can lose its
# terminal while the pre-init script is running (OpenConnect 8.20).
exec openconnect --protocol=anyconnect --interface=tun0 \
    --useragent="$VPN_USERAGENT" --user="$VPN_USER" \
    --script=/usr/share/vpnc-scripts/vpnc-script "$VPN_SERVER"
