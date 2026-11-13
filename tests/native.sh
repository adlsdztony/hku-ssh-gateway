#!/usr/bin/env bash
# Privileged integration test for an isolated CI VM, never a user's live gateway.
set -euo pipefail
ROOT=$(cd -- "$(dirname -- "${BASH_SOURCE[0]}")/.." && pwd)
BIN=${1:-$ROOT/target/debug/hku}
TMP=$(mktemp -d)
PEER="hku-native-test-$$"
id=
namespace=
host_server=
peer_server=
fixture_installed=false
export PATH="/usr/local/sbin:$PATH"
export HKU_TEST_BIN="$BIN" HKU_TEST_STATE="$TMP/state"
hku() { "$BIN" --state-dir "$TMP/state" "$@"; }
cleanup() {
    if [[ $? != 0 && $id =~ ^[0-9a-f]{12}$ ]]; then
        sudo journalctl -u "hku-native-$id.service" --no-pager -n 100 || true
    fi
    hku down >/dev/null 2>&1 || true
    if [[ -n $host_server ]]; then kill "$host_server" 2>/dev/null || true; fi
    if [[ -n $peer_server ]]; then kill "$peer_server" 2>/dev/null || true; fi
    sudo ip netns pids "$PEER" 2>/dev/null | xargs -r sudo kill -TERM 2>/dev/null || true
    sudo ip netns delete "$PEER" >/dev/null 2>&1 || true
    if [[ $id =~ ^[0-9a-f]{12}$ ]]; then sudo rm -rf -- "/var/lib/hku/native/$id"; fi
    if [[ $fixture_installed == true ]]; then sudo rm -f /usr/local/sbin/openconnect; fi
    rm -rf -- "$TMP"
}
trap cleanup EXIT
test ! -e /usr/local/sbin/openconnect
cc -static -O2 -Wall -Wextra -Werror "$ROOT/tests/mock-openconnect.c" -o "$TMP/openconnect"
sudo install -m 755 "$TMP/openconnect" /usr/local/sbin/openconnect
fixture_installed=true
before_route=$(ip -4 route show default)
before_dns=$(sha256sum /etc/resolv.conf)
hku setup --user test@example.invalid --vpn-server fixture.invalid --project native-test \
    --keepalive-host 10.250.254.2 --keepalive-interval 1 || {
    sudo journalctl --no-pager -n 150 -u 'hku-native-*' || true
    exit 1
}
state=$(hku status || test "$?" -eq 2)
namespace=$(printf '%s\n' "$state" | awk '/^Namespace:/ {print $2}')
id=${namespace#hku-}
[[ $id =~ ^[0-9a-f]{12}$ ]]
socket="/run/hku-native/$id.sock"
# A reboot clears /run, including the socket parent created by the installer.
sudo systemctl stop "hku-native-$id.service"
sudo rmdir /run/hku-native
sudo systemctl start "hku-native-$id.service"
for attempt in {1..80}; do
    if hku status >/dev/null 2>&1 || [[ $? == 2 ]]; then break; fi
    sleep 0.1
done
test "$(stat -c %a "$socket")" = 600
test "$(stat -c %u "$socket")" = "$(id -u)"
test "$(stat -c %u "/var/lib/hku/native")" = 0
host_ip=$(ip -o -4 address show "hk$id" | awk '{split($4,a,"/"); print a[1]}')
cat > "$TMP/listen.py" <<'PY'
import socket, sys
s = socket.socket(); s.setsockopt(socket.SOL_SOCKET, socket.SO_REUSEADDR, 1)
s.bind((sys.argv[1], int(sys.argv[2]))); s.listen()
while True:
    c, _ = s.accept()
    with c: c.sendall(b"HKU_NATIVE_OK\n")
PY
python3 "$TMP/listen.py" "$host_ip" 18080 &
host_server=$!
sleep 0.2
sudo ip netns exec "$namespace" python3 -c 'import socket,sys; socket.create_connection((sys.argv[1],18080)).close()' "$host_ip"
if timeout 8 "$BIN" --state-dir "$TMP/state" proxy "$host_ip" 18080 </dev/null; then
    echo 'FAIL: unprivileged native stream escaped through eth0' >&2; exit 1
fi

sudo ip netns add "$PEER"
sudo ip link add "ht$$" type veth peer name "hp$$"
sudo ip link set "ht$$" netns "$namespace"
sudo ip -n "$namespace" link set "ht$$" name tun0
sudo ip link set "hp$$" netns "$PEER"
sudo ip -n "$PEER" address add 10.250.254.2/30 dev "hp$$"
sudo ip -n "$PEER" link set "hp$$" up
sudo ip -n "$PEER" link set lo up
sudo ip netns exec "$PEER" python3 "$TMP/listen.py" 10.250.254.2 22 &
peer_server=$!
cat > "$TMP/login" <<'LOGIN'
#!/bin/bash
before=$(stty -g)
"$HKU_TEST_BIN" --state-dir "$HKU_TEST_STATE" connect
result=$?
test "$before" = "$(stty -g)" || exit 90
touch "$HKU_TEST_STATE/returned"
exit "$result"
LOGIN
chmod 755 "$TMP/login"
{ sleep 1; printf 'fixture-password\n'; sleep 1; printf 'fixture-otp\n';
  for attempt in {1..80}; do
      if [[ -f $TMP/state/returned ]]; then exit 0; fi
      sleep 0.1
  done
  exit 1;
} | timeout 20 script -q -e -c "$TMP/login" /dev/null > "$TMP/login.log" 2>&1 || { cat "$TMP/login.log"; exit 1; }
grep -q 'VPN connected. It will keep running' "$TMP/login.log"
if grep -qE 'fixture-password|fixture-otp' "$TMP/login.log"; then echo 'FAIL: input was echoed'; exit 1; fi
test "$(timeout 8 "$BIN" --state-dir "$TMP/state" proxy 10.250.254.2 22 </dev/null)" = HKU_NATIVE_OK
sleep 2
hku status | grep -q 'Keepalive: ok'

# Exercise the real routing script with DNS handled only by the native hook.
sudo ip netns exec "$namespace" env reason=connect TUNDEV=tun0 VPNGATEWAY="$host_ip" \
    INTERNAL_IP4_ADDRESS=10.250.254.1 INTERNAL_IP4_NETMASK=255.255.255.252 INTERNAL_IP4_MTU=1300 \
    INTERNAL_IP4_DNS=10.250.254.2 \
    "/var/lib/hku/native/$id/hku" native-internal hook --id "$id"
sudo grep -q 'nameserver 10.250.254.2' "/etc/netns/$namespace/resolv.conf"
cat > "$TMP/dns.py" <<'PY'
import socket, struct
s = socket.socket(socket.AF_INET, socket.SOCK_DGRAM)
s.bind(('10.250.254.2', 53))
while True:
    data, address = s.recvfrom(4096)
    end = 12
    labels = []
    while data[end]:
        length = data[end]; end += 1
        labels.append(data[end:end + length]); end += length
    end += 1
    qtype, qclass = struct.unpack('!HH', data[end:end + 4]); end += 4
    answer = b''
    if b'.'.join(labels) == b'fixture.hku.invalid' and qtype == 1 and qclass == 1:
        answer = b'\xc0\x0c' + struct.pack('!HHIH', 1, 1, 0, 4) + socket.inet_aton('10.250.254.2')
    header = data[:2] + struct.pack('!HHHHH', 0x8180, 1, bool(answer), 0, 0)
    s.sendto(header + data[12:end] + answer, address)
PY
sudo ip netns exec "$PEER" python3 "$TMP/dns.py" &
sleep 0.2
test "$(timeout 8 "$BIN" --state-dir "$TMP/state" proxy fixture.hku.invalid 22 </dev/null)" = HKU_NATIVE_OK
test "$before_dns" = "$(sha256sum /etc/resolv.conf)"
test "$before_route" = "$(ip -4 route show default)"

# A separate ordinary user must not access the owner's root service.
if sudo -u nobody python3 -c 'import socket,sys; s=socket.socket(socket.AF_UNIX); s.connect(sys.argv[1])' "$socket" 2>/dev/null; then
    echo 'FAIL: another user reached the native control socket' >&2; exit 1
fi
hku disconnect
sleep 1
if timeout 8 "$BIN" --state-dir "$TMP/state" proxy "$host_ip" 18080 </dev/null; then
    echo 'FAIL: disconnected stream fell back to eth0' >&2; exit 1
fi
hku down
hku down
test ! -e "/run/netns/$namespace"
test ! -e "$socket"
if sudo iptables -S | grep -q "HKU_$id"; then echo 'FAIL: forwarding rules remain'; exit 1; fi
if sudo iptables -t nat -S | grep -q "HKU_$id"; then echo 'FAIL: NAT rule remains'; exit 1; fi
test "$before_dns" = "$(sha256sum /etc/resolv.conf)"
test "$before_route" = "$(ip -4 route show default)"
printf 'PASS: native service, privilege boundary, namespace streams, auto login, keepalive, DNS isolation and scoped cleanup.\n'
