# Native Linux backend (experimental)

Build `feat/native-linux-backend` with `cargo build --release`. This backend is not included in the current stable release. Windows and macOS provide remote SSH/proxy clients only; their configuration and private keys stay on the client.

## Setup and ownership

Run the CLI as the ordinary account that will own the gateway. `setup`, `up` and `down` invoke sudo for administrative work; do not run the whole CLI as root. Prerequisites are a systemd system manager, OpenConnect, vpnc-scripts, iproute2, iptables/ip6tables and TUN.

The privileged installer copies the executable and validated configuration into a root-controlled instance directory under `/var/lib/hku/native`. It installs a systemd unit named `hku-native-INSTANCE.service`. The service does not execute scripts from user-writable state directories. Dependencies and the routing script must be root-owned, with non-writable parent directories.

Runtime control uses `/run/hku-native/INSTANCE.sock`, mode 0600 and owned by the initiating user. The service also checks Unix peer credentials. Status, login and TCP proxy operations do not invoke sudo. Proxy workers enter the namespace and drop supplementary groups, GID and UID before opening destination sockets.

## Network isolation

Each instance has its own named network namespace, veth pair and private /30 transit network. The namespace owns the VPN interface and default route. The host receives only an instance-specific veth route and tagged forwarding/NAT rules, not a replacement default route. Conflicting transit ranges are rejected; configure `native.subnet` to select another range.

Setup enables host IPv4 forwarding if it is disabled. This shared host setting is deliberately left enabled on shutdown, avoiding interference with other routing applications. `down` removes only the owned namespace, veth, service unit and tagged firewall rules; installed metadata and user configuration remain for later use. Stop a deployment before moving its state directory, since its identity includes the canonical path and owner UID.

Forwarded sockets use an unprivileged UID whose namespace OUTPUT rules allow only tun0 and loopback, including IPv6. When tun0 goes away, existing streams cannot escape over the veth. This is network isolation, not container filesystem/process isolation; the OpenConnect process still has administrative host capabilities.

## DNS isolation

Initial DNS comes from configured `native.bootstrap_dns` or the host's non-loopback upstream IPv4 DNS. A local host stub such as 127.0.0.53 cannot be used from another namespace. If no upstream can be discovered, configure an explicit reachable DNS server.

Namespace-specific `resolv.conf` and `nsswitch.conf` are bind-mounted by `ip netns exec`. The hosts NSS database uses `files dns`, preventing nss-resolve from bypassing namespace DNS through the host resolver's Unix socket. VPN DNS is applied only to the namespace file. The routing hook writes validated DNS addresses there, then replaces itself with the system vpnc-script after removing INTERNAL_IP4_DNS/INTERNAL_IP6_DNS so it cannot update the host's DNS manager. Using exec preserves the OpenConnect parent PID used to pair the script's route backups across connection and disconnection.

## Authentication and lifecycle

The root service holds OpenConnect's pseudo-terminal. `hku connect` relays terminal bytes over the owner-only socket, without parsing prompts or extracting cookies. Credentials are handled by OpenConnect. Authentication output is not retained in logs. The client automatically restores its terminal and returns when the process, VPN address and route are ready; the daemon keeps the PTY alive.

Keepalive uses a separate unprivileged worker inside the namespace. A host/service restart requires interactive MFA again. `hku logs` returns a bounded buffer of post-connection output; systemd lifecycle errors are available through `sudo journalctl -u hku-native-INSTANCE.service`.

## Docker compatibility

New configurations default to native; old TOML without a backend field defaults to Docker. Explicitly use `--backend docker` for a new Docker deployment. Backends cannot be switched in place: use a separate state directory and stop the old deployment yourself when ready. The original Docker runtime and its integration tests are retained.

## Validation

The native integration test runs only on a dedicated CI VM. It checks a real systemd service and Linux namespaces with a terminal fixture, privilege boundaries, fail-closed streams, automatic login return, keepalive, scoped DNS updates, unchanged host default route/resolv.conf and cleanup of owned rules. It never authenticates to HKU and does not alter a user's live gateway. Live native HKU MFA remains a separate user acceptance check.
