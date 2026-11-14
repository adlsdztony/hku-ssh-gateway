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
tls_server=
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
    if [[ -n $tls_server ]]; then kill "$tls_server" 2>/dev/null || true; fi
    sudo ip netns pids "$PEER" 2>/dev/null | xargs -r sudo kill -TERM 2>/dev/null || true
    sudo ip netns delete "$PEER" >/dev/null 2>&1 || true
    if [[ $id =~ ^[0-9a-f]{12}$ ]]; then sudo rm -rf -- "/var/lib/hku/native/$id"; fi
    if [[ $fixture_installed == true ]]; then sudo rm -f /usr/local/sbin/openconnect; fi
    rm -rf -- "$TMP"
}
trap cleanup EXIT
if [[ -z ${HKU_TEST_RUNTIME:-} ]]; then
    test ! -e /usr/local/sbin/openconnect
    cc -static -O2 -Wall -Wextra -Werror "$ROOT/tests/mock-openconnect.c" -o "$TMP/openconnect"
    sudo install -m 755 "$TMP/openconnect" /usr/local/sbin/openconnect
    fixture_installed=true
else
    ! command -v openconnect
    test ! -e /usr/share/vpnc-scripts/vpnc-script
    test ! -e /etc/vpnc/vpnc-script
    test -f "$HKU_TEST_MOCK"
fi
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
before_ns_route=$(sudo ip -n "$namespace" -4 route show default)
if [[ -n ${HKU_TEST_RUNTIME:-} ]]; then
    runtime="/var/lib/hku/native/$id/runtime/$(sha256sum "$HKU_TEST_RUNTIME" | cut -d ' ' -f 1)"
    loader="$runtime/lib/ld-musl-$(uname -m).so.1"
    sudo "$loader" --library-path "$runtime/lib" "$runtime/bin/openconnect" --version
    openssl req -x509 -newkey rsa:2048 -nodes -days 1 -subj '/CN=hku-fixture.invalid' \
        -addext "subjectAltName=IP:$host_ip" -keyout "$TMP/key.pem" -out "$TMP/cert.pem" >/dev/null 2>&1
    cat > "$TMP/tls.py" <<'PY'
import socket, ssl, sys
context = ssl.SSLContext(ssl.PROTOCOL_TLS_SERVER)
context.load_cert_chain(sys.argv[2], sys.argv[3])
server = socket.socket(); server.bind((sys.argv[1], 0)); server.listen()
open(sys.argv[4], 'w').write(str(server.getsockname()[1]))
while True:
    c, _ = server.accept()
    try:
        with context.wrap_socket(c, server_side=True) as tls:
            tls.recv(4096)
            tls.sendall(b'HTTP/1.1 401 Unauthorized\r\nContent-Length: 0\r\nConnection: close\r\n\r\n')
    except (ssl.SSLError, ConnectionError, OSError): c.close()
PY
    python3 "$TMP/tls.py" "$host_ip" "$TMP/cert.pem" "$TMP/key.pem" "$TMP/tls-port" &
    tls_server=$!
    for attempt in {1..50}; do test ! -f "$TMP/tls-port" || break; sleep 0.1; done
    tls_port=$(cat "$TMP/tls-port")
    if timeout 15 sudo ip netns exec "$namespace" "$loader" --library-path "$runtime/lib" \
        "$runtime/bin/openconnect" --no-system-trust --cafile="$runtime/ca-certificates.crt" \
        --non-inter --user fixture "https://$host_ip:$tls_port" </dev/null > "$TMP/tls-reject.log" 2>&1; then
        echo 'FAIL: bundled TLS accepted an untrusted certificate'; exit 1
    fi
    grep -qiE 'certificate.*(verify|verification|trust|signer)' "$TMP/tls-reject.log" || { cat "$TMP/tls-reject.log"; exit 1; }
    timeout 15 sudo ip netns exec "$namespace" "$loader" --library-path "$runtime/lib" \
        "$runtime/bin/openconnect" --no-system-trust --cafile="$TMP/cert.pem" \
        --non-inter --user fixture "https://$host_ip:$tls_port" </dev/null > "$TMP/tls-trust.log" 2>&1 || true
    grep -q 'Connected to HTTPS' "$TMP/tls-trust.log" || { cat "$TMP/tls-trust.log"; exit 1; }
    echo 'PASS: embedded OpenConnect and TLS trust/rejection without host VPN packages.'
    main_pid=$(systemctl show "hku-native-$id.service" --property MainPID --value)
    sudo cp "$runtime/ca-certificates.crt" "$TMP/original-ca.crt"
    sudo python3 -c 'import sys; f=open(sys.argv[1],"r+b"); f.write(b"!"); f.close()' "$runtime/ca-certificates.crt"
    if hku up > "$TMP/tamper.log" 2>&1; then
        echo 'FAIL: installer accepted a corrupted runtime cache'; exit 1
    fi
    grep -q 'Runtime checksum mismatch' "$TMP/tamper.log" || { cat "$TMP/tamper.log"; exit 1; }
    test "$main_pid" = "$(systemctl show "hku-native-$id.service" --property MainPID --value)"
    sudo cp "$TMP/original-ca.crt" "$runtime/ca-certificates.crt"
    echo 'PASS: corrupted runtime cache rejected before stopping the existing service.'
    # Replace only the test VM's private executable with a matching musl fixture.
    # Production bundles never contain this fixture or a runtime override option.
    sudo install -m 755 "$HKU_TEST_MOCK" "$runtime/bin/openconnect.fixture"
    sudo mv "$runtime/bin/openconnect.fixture" "$runtime/bin/openconnect"
fi
cat > "$TMP/listen.py" <<'PY'
import socket, sys, threading
s = socket.socket(); s.setsockopt(socket.SOL_SOCKET, socket.SO_REUSEADDR, 1)
s.bind((sys.argv[1], int(sys.argv[2]))); s.listen()
def serve(c):
    with c:
        try:
            c.sendall(b"HKU_NATIVE_OK\n")
            while data := c.recv(65536): c.sendall(data)
        except (ConnectionError, OSError): pass
while True:
    c, _ = s.accept()
    threading.Thread(target=serve, args=(c,), daemon=True).start()
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
login() {
    rm -f "$TMP/state/returned"
    { sleep 1; printf 'fixture-password\n'; sleep 1; printf 'fixture-otp\n';
      for attempt in {1..80}; do
          if [[ -f $TMP/state/returned ]]; then exit 0; fi
          sleep 0.1
      done
      exit 1;
    } | timeout 20 script -q -e -c "$TMP/login" /dev/null
}
login > "$TMP/login.log" 2>&1 || { cat "$TMP/login.log"; exit 1; }
grep -q 'VPN connected. It will keep running' "$TMP/login.log"
if grep -qE 'fixture-password|fixture-otp' "$TMP/login.log"; then echo 'FAIL: input was echoed'; exit 1; fi
test "$(timeout 8 "$BIN" --state-dir "$TMP/state" proxy 10.250.254.2 22 </dev/null)" = HKU_NATIVE_OK
# Do not close stdin to trigger flushing: SSH needs binary packets immediately.
python3 - "$BIN" "$TMP/state" <<'PY'
import os, select, subprocess, sys, time
p = subprocess.Popen([sys.argv[1], '--state-dir', sys.argv[2], 'proxy', '10.250.254.2', '22'],
                     stdin=subprocess.PIPE, stdout=subprocess.PIPE)
def receive(size):
    data = b''
    deadline = time.monotonic() + 10
    while len(data) < size:
        ready, _, _ = select.select([p.stdout], [], [], max(0, deadline - time.monotonic()))
        if not ready: raise AssertionError('Live binary stream stalled without newline/EOF')
        chunk = os.read(p.stdout.fileno(), size - len(data))
        if not chunk: raise AssertionError('Binary stream ended early')
        data += chunk
    return data
try:
    assert receive(len(b'HKU_NATIVE_OK\n')) == b'HKU_NATIVE_OK\n'
    for payload in [b'\x00\xff', b'\x01\xfe\x00' * 300, b'\x02\xfd\x00' * 4000]:
        p.stdin.write(payload); p.stdin.flush()
        assert receive(len(payload)) == payload
    print('PASS: live bidirectional binary forwarding without newline or EOF.')
finally:
    p.terminate(); p.wait(timeout=5)
PY
sleep 2
hku status | grep -q 'Keepalive: ok'

# The mock invokes the real routing hook with a complete OpenConnect environment.
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
for attempt in {1..80}; do
    if (hku status 2>/dev/null || test "$?" -eq 2) | grep -q 'VPN: offline'; then break; fi
    sleep 0.1
done
test "$before_ns_route" = "$(sudo ip -n "$namespace" -4 route show default)"
if timeout 8 "$BIN" --state-dir "$TMP/state" proxy "$host_ip" 18080 </dev/null; then
    echo 'FAIL: disconnected stream fell back to eth0' >&2; exit 1
fi
login > "$TMP/retry.log" 2>&1 || { cat "$TMP/retry.log"; exit 1; }
hku status | grep -q 'VPN: connected'
hku down
hku down
if sudo journalctl -u "hku-native-$id.service" --no-pager | grep -qE 'Text file busy|Bind /etc/netns/.*/hku-owner'; then
    echo 'FAIL: native installation or namespace bind warning' >&2; exit 1
fi
test ! -e "/run/netns/$namespace"
test ! -e "$socket"
if sudo iptables -S | grep -q "HKU_$id"; then echo 'FAIL: forwarding rules remain'; exit 1; fi
if sudo iptables -t nat -S | grep -q "HKU_$id"; then echo 'FAIL: NAT rule remains'; exit 1; fi
test "$before_dns" = "$(sha256sum /etc/resolv.conf)"
test "$before_route" = "$(ip -4 route show default)"
printf 'PASS: native service, privilege boundary, namespace streams, auto login, keepalive, DNS isolation and scoped cleanup.\n'
