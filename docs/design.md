# Design

The Rust CLI manages typed TOML configuration, a persistent MFA console and a raw stdio TCP proxy. New Linux deployments use the native network-namespace backend; Docker Compose remains an explicit alternative. Configuration created before the backend option keeps using Docker. Windows and macOS provide remote clients only.

## Common SSH transport

`hku ssh` starts the client's OpenSSH process. It uses `hku proxy` locally, or invokes that proxy through an ordinary authenticated SSH command channel on a remote Linux gateway. Both hops use local SSH configuration, keys and host-key verification. There is no key registration or public proxy listener. Tailscale is optional transport to the gateway.

## Native Linux backend

The installer requests sudo to copy validated configuration and the executable into root-controlled directories and install a systemd service. The service creates a dedicated netns, veth, scoped NAT/forwarding rules and namespace-specific resolver files. The host default route and DNS are preserved; host IPv4 forwarding is enabled if needed and left enabled on shutdown.

Linux CI/release builds embed a relocatable OpenConnect/musl runtime and vpnc-script. The installer verifies and extracts it into an instance-private digest directory before starting the service. The loader uses an explicit library path and TLS uses a root-owned host CA file or the bundled fallback. Source builds without a payload use host packages. Neither mode replaces system libraries. Corresponding sources and license notices accompany the runtime; see [packaging details](bundled-runtime.md).

Daily control uses a Unix socket restricted to its owner, with peer-credential checks. The daemon holds OpenConnect's PTY, relaying authentication bytes without parsing prompts or extracting credentials. On readiness, the client restores its terminal and returns automatically. The service retains the PTY and VPN process. Proxy and keepalive workers enter the namespace and drop privileges before resolving destinations and opening sockets. Namespace firewall rules allow their UID only tun0 and loopback, with no fallback when the VPN disappears.

This provides network isolation, not filesystem/process sandboxing of privileged OpenConnect. Full ownership, DNS and lifecycle details are in [the native backend reference](native.md).

## Optional Docker backend

There is no SSH daemon, key registration, password database or published proxy port inside the container. Local users access it through their existing Docker permissions. Remote users execute `hku proxy` through the host's ordinary authenticated SSH command channel. The destination still uses normal end-to-end SSH authentication and host-key verification.

The proxy executes netcat as a restricted container UID. IPv4/IPv6 OUTPUT rules permit that UID's sockets only through tun0 or loopback. No general established-connection exception permits fallback after tunnel loss. Optional destination restrictions are convenience CLI policy, not a security boundary against a Docker administrator.

OpenConnect runs in the container's persistent Docker terminal with stdin kept open. The CLI attaches for interactive login, checks only process/interface/route readiness, and stops its own attach client automatically after connection. It restores the user's terminal settings before returning. There is no tmux dependency, authentication-output parsing, cookie extraction or credential handoff. OpenConnect handles credentials directly. The AnyConnect user agent selects HKU XML POST MFA; normal TLS verification remains enabled.

The supervised TCP keepalive waits for a configured VPN interface, resolves its independent target, validates the route, and connects as the restricted user. It records result times and state transitions. It addresses application idle accounting, not hard session lifetime or compulsory reauthentication. Restarting the container restores the background service but still requires interactive VPN login.

## Packaging

Release binaries embed only runtime templates, never instance data. Linux amd64/arm64 runners build musl binaries; Windows and macOS Intel/Apple Silicon runners build clients. Tagged publication depends on Rust, installer, native and Docker integration checks. The Bash installer downloads over GitHub HTTPS, checks SHA256SUMS and atomically replaces the CLI binary, without restarting deployments. This branch is experimental; the current stable release still provides the Docker implementation.
