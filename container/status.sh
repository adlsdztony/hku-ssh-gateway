#!/bin/bash
set -euo pipefail
printf '\n--- VPN process ---\n'
ps -C openconnect -o pid,etimes,comm || true
printf '\n--- Network ---\n'
ip -brief address
ip -4 route
printf '\n--- Keepalive (UTC) ---\n'
cat /run/hku-keepalive/status 2>/dev/null || true
printf '\n--- Destination policy ---\n%s\n' "$SSH_ALLOWED_TARGETS"
if ip -4 address show dev tun0 2>/dev/null | /bin/grep -q 'inet '; then
    printf '\nVPN interface is configured. Target authentication is separate.\n'
else
    printf '\nVPN is offline. Run: bash gateway connect\n'
    exit 2
fi
