#!/bin/bash
# Shared validation. Never source user configuration as shell code.
fail() { printf 'Error: %s\n' "$*" >&2; exit 1; }
valid_host() { [[ $1 =~ ^[A-Za-z0-9][A-Za-z0-9._-]*$ ]]; }
valid_port() { [[ $1 =~ ^[0-9]{1,5}$ ]] && (( 10#$1 >= 1 && 10#$1 <= 65535 )); }
valid_seconds() { [[ $1 =~ ^[0-9]{1,6}$ ]] && (( 10#$1 >= 1 )); }
validate_config() {
    : "${VPN_SERVER:=vpn2fa.hku.hk}" "${VPN_USERAGENT:=AnyConnect}"
    : "${SSH_ALLOWED_TARGETS:=any}" "${KEEPALIVE_ENABLED:=true}"
    : "${KEEPALIVE_HOST:=gpu3gate1.cs.hku.hk}" "${KEEPALIVE_PORT:=22}"
    : "${KEEPALIVE_INTERVAL:=300}" "${KEEPALIVE_TIMEOUT:=5}"
    local server_host=${VPN_SERVER%:*} server_port=${VPN_SERVER##*:} item
    valid_host "$server_host" || fail 'VPN_SERVER must be a hostname, optionally followed by :port.'
    if [[ $VPN_SERVER == *:* ]]; then valid_port "$server_port" || fail 'Invalid VPN port.'; fi
    [[ ${VPN_USER:-} =~ ^[A-Za-z0-9._+@-]+$ ]] || fail 'Set VPN_USER to your HKU email address.'
    [[ $VPN_USERAGENT =~ ^[A-Za-z0-9._/-]+$ ]] || fail 'Invalid VPN_USERAGENT.'
    if [[ $SSH_ALLOWED_TARGETS != any ]]; then
        [[ -n $SSH_ALLOWED_TARGETS && $SSH_ALLOWED_TARGETS != *$'\n'* && $SSH_ALLOWED_TARGETS != *$'\r'* ]] || fail 'Invalid SSH_ALLOWED_TARGETS.'
        for item in $SSH_ALLOWED_TARGETS; do
            [[ $item == *:* ]] && valid_host "${item%:*}" && valid_port "${item##*:}" || fail "Invalid allowed destination: $item"
        done
    fi
    [[ $KEEPALIVE_ENABLED == true || $KEEPALIVE_ENABLED == false ]] || fail 'KEEPALIVE_ENABLED must be true or false.'
    valid_host "$KEEPALIVE_HOST" || fail 'Invalid KEEPALIVE_HOST.'
    valid_port "$KEEPALIVE_PORT" || fail 'Invalid KEEPALIVE_PORT.'
    valid_seconds "$KEEPALIVE_INTERVAL" || fail 'KEEPALIVE_INTERVAL must be 1..999999 seconds.'
    valid_seconds "$KEEPALIVE_TIMEOUT" || fail 'KEEPALIVE_TIMEOUT must be 1..999999 seconds.'
    # Normalize leading zeroes before passing values to other programs.
    KEEPALIVE_PORT=$((10#$KEEPALIVE_PORT))
    KEEPALIVE_INTERVAL=$((10#$KEEPALIVE_INTERVAL))
    KEEPALIVE_TIMEOUT=$((10#$KEEPALIVE_TIMEOUT))
}
