"""Test real OpenConnect HTTPS redirects with slow DNS, inside the CI netns.

Only namespace-local resolver contents and loopback sockets are changed.
No real VPN credentials are supplied; the original resolver is restored.
"""
import http.server
import os
from pathlib import Path
import socket
import ssl
import struct
import subprocess
import sys
import tempfile
import threading
import time

runtime = Path(sys.argv[1])
resolver = Path('/etc/resolv.conf')
original = resolver.read_text()
options = '\n'.join(line for line in original.splitlines() if line.startswith('options '))
dns = socket.socket(socket.AF_INET, socket.SOCK_DGRAM)
dns.bind(('127.0.0.1', 53))
reached = threading.Event()

def answer(packet, address):
    end = 12
    while packet[end]:
        end += packet[end] + 1
    end += 1
    qtype, qclass = struct.unpack('!HH', packet[end:end + 4])
    end += 4
    response = b''
    if qtype == 1 and qclass == 1:
        response = b'\xc0\x0c' + struct.pack('!HHIH', 1, 1, 0, 4) + socket.inet_aton('127.0.0.1')
    time.sleep(1.5)
    header = packet[:2] + struct.pack('!HHHHH', 0x8180, 1, bool(response), 0, 0)
    try:
        dns.sendto(header + packet[12:end] + response, address)
    except OSError:
        pass

def serve_dns():
    while True:
        try:
            packet, address = dns.recvfrom(4096)
        except OSError:
            return
        threading.Thread(target=answer, args=(packet, address), daemon=True).start()

class Handler(http.server.BaseHTTPRequestHandler):
    def do_POST(self):
        if self.headers.get('Host', '').startswith('slow.fixture.invalid:'):
            reached.set()
            self.send_response(401)
        else:
            self.send_response(302)
            self.send_header('Location', 'https://slow.fixture.invalid:%d/' % self.server.server_port)
        self.send_header('Content-Length', '0')
        self.send_header('Connection', 'close')
        self.end_headers()

    do_GET = do_POST

    def log_message(self, *args):
        pass

server = None
try:
    resolver.write_text('nameserver 127.0.0.1\n' + options + '\n')
    with tempfile.TemporaryDirectory() as temporary:
        ca = Path(temporary) / 'cert.pem'
        key = Path(temporary) / 'key.pem'
        subprocess.run(['openssl', 'req', '-x509', '-newkey', 'rsa:2048', '-nodes',
                        '-days', '1', '-subj', '/CN=slow.fixture.invalid',
                        '-addext', 'subjectAltName=IP:127.0.0.1,DNS:slow.fixture.invalid',
                        '-keyout', str(key), '-out', str(ca)],
                       stdout=subprocess.DEVNULL, stderr=subprocess.DEVNULL, check=True)
        context = ssl.SSLContext(ssl.PROTOCOL_TLS_SERVER)
        context.load_cert_chain(ca, key)
        server = http.server.ThreadingHTTPServer(('127.0.0.1', 0), Handler)
        server.socket = context.wrap_socket(server.socket, server_side=True)
        threading.Thread(target=server.serve_forever, daemon=True).start()
        threading.Thread(target=serve_dns, daemon=True).start()
        command = [str(runtime / ('lib/ld-musl-' + os.uname().machine + '.so.1')),
                   '--library-path', str(runtime / 'lib'), str(runtime / 'bin/openconnect'),
                   '--no-system-trust', '--cafile=' + str(ca), '--no-proxy',
                   '--non-inter', '--user=fixture',
                   'https://127.0.0.1:%d/' % server.server_port]
        result = subprocess.run(command, stdin=subprocess.DEVNULL, capture_output=True, timeout=20)
        if not reached.is_set():
            sys.stderr.write(result.stdout.decode(errors='replace') + result.stderr.decode(errors='replace'))
            raise AssertionError('Redirect did not reach the delayed-DNS HTTPS endpoint')
        assert result.returncode != 0, 'Fixture must not authenticate a VPN session'
        print('PASS: bundled OpenConnect HTTPS redirect with delayed A/AAAA DNS replies.')
finally:
    resolver.write_text(original)
    dns.close()
    if server is not None:
        server.shutdown()
        server.server_close()
