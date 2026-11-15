"""Verify Cisco STRAP channel bindings using local, synthetic identities only.

The server checks ECDSA over RFC5929 tls-unique (TLS 1.2) or RFC9266
tls-exporter (TLS 1.3). Generated fixture cookies/keys never leave loopback;
this is not a production cookie handoff or an HKU authentication transcript.
"""
import base64
from pathlib import Path
import socket
import subprocess
import sys
import tempfile
import threading

from OpenSSL import SSL
from cryptography.hazmat.primitives import hashes, serialization
from cryptography.hazmat.primitives.asymmetric import ec

command = sys.argv[1:]
assert command, 'Pass the OpenConnect executable and any loader arguments'

def check(temporary, version, label):
    key = ec.generate_private_key(ec.SECP256R1())
    public = key.public_key().public_bytes(serialization.Encoding.DER,
                                          serialization.PublicFormat.SubjectPublicKeyInfo)
    private = key.private_bytes(serialization.Encoding.DER,
                                serialization.PrivateFormat.TraditionalOpenSSL,
                                serialization.NoEncryption())
    context = SSL.Context(SSL.TLS_SERVER_METHOD)
    context.set_min_proto_version(version)
    context.set_max_proto_version(version)
    context.use_certificate_file(str(temporary / 'cert.pem'))
    context.use_privatekey_file(str(temporary / 'key.pem'))
    listener = socket.socket()
    listener.bind(('127.0.0.1', 0))
    listener.listen()
    outcome = []

    def serve():
        connection = None
        try:
            raw, _ = listener.accept()
            connection = SSL.Connection(context, raw)
            connection.set_accept_state()
            connection.do_handshake()
            data = b''
            while b'\r\n\r\n' not in data:
                data += connection.recv(4096)
                if len(data) > 65536:
                    raise AssertionError('Oversized fixture request')
            lines = data.split(b'\r\n')
            assert lines[0].startswith(b'CONNECT /CSCOSSLC/tunnel ')
            headers = dict(line.split(b':', 1) for line in lines[1:] if b':' in line)
            announced = base64.b64decode(headers[b'X-AnyConnect-STRAP-Pubkey'].strip(), validate=True)
            signature = base64.b64decode(headers[b'X-AnyConnect-STRAP-Verify'].strip(), validate=True)
            assert announced == public, 'Fixture STRAP identity changed unexpectedly'
            if version == SSL.TLS1_3_VERSION:
                binding = connection.export_keying_material(b'EXPORTER-Channel-Binding', 32, b'')
            else:
                binding = connection.get_peer_finished()
            key.public_key().verify(signature, binding + public, ec.ECDSA(hashes.SHA256()))
            outcome.append(True)
            # No address is issued, so the client exits without configuring a VPN.
            connection.sendall(b'HTTP/1.1 200 OK\r\nX-CSTP-Version: 1\r\n\r\n')
        except Exception as error:
            outcome.append(type(error).__name__)
            if connection is not None:
                try:
                    connection.sendall(b'HTTP/1.1 401 Unauthorized\r\nContent-Length: 0\r\n\r\n')
                except Exception:
                    pass
        finally:
            if connection is not None:
                connection.close()

    worker = threading.Thread(target=serve, daemon=True)
    worker.start()
    fixture_cookie = 'openconnect_strapkey=%s; webvpn=local-fixture-session\n' % base64.b64encode(private).decode()
    arguments = command + ['--protocol=anyconnect', '--useragent=AnyConnect', '--no-proxy',
                           '--no-dtls', '--non-inter', '--no-system-trust',
                           '--cafile=' + str(temporary / 'cert.pem'), '--cookie-on-stdin',
                           'https://127.0.0.1:%d' % listener.getsockname()[1]]
    try:
        result = subprocess.run(arguments, input=fixture_cookie.encode(), capture_output=True, timeout=15)
        worker.join(timeout=3)
        if outcome != [True]:
            sys.stderr.write(result.stderr.decode(errors='replace'))
            raise AssertionError('%s STRAP channel binding rejected: %s' % (label, outcome))
        assert result.returncode != 0, 'Fixture must not establish a configured VPN tunnel'
        print('PASS: %s Cisco STRAP signature matches the standardized channel binding.' % label)
    finally:
        listener.close()

with tempfile.TemporaryDirectory() as directory:
    temporary = Path(directory)
    subprocess.run(['openssl', 'req', '-x509', '-newkey', 'rsa:2048', '-nodes', '-days', '1',
                    '-subj', '/CN=strap.fixture.invalid', '-addext', 'subjectAltName=IP:127.0.0.1',
                    '-keyout', str(temporary / 'key.pem'), '-out', str(temporary / 'cert.pem')],
                   stdout=subprocess.DEVNULL, stderr=subprocess.DEVNULL, check=True)
    check(temporary, SSL.TLS1_2_VERSION, 'TLS 1.2')
    check(temporary, SSL.TLS1_3_VERSION, 'TLS 1.3')
