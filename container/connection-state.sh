#!/bin/bash
set -eu
# Only network/process metadata is inspected. Authentication output is not read.
if [[ -s /run/hku-openconnect.pid ]] &&
    kill -0 "$(cat /run/hku-openconnect.pid)" 2>/dev/null &&
    ip -o -4 address show dev tun0 2>/dev/null | grep -q ' inet ' &&
    ip -4 route show default | grep -q '^default dev tun0\( \|$\)'; then
    exit 0
fi
if [[ -f /run/hku-vpn.exit ]]; then exit 2; fi
exit 1
