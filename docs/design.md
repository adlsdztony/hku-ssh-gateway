# Design

The native Rust CLI manages typed TOML configuration, writes embedded runtime assets, invokes Docker Compose without shell interpolation, attaches a persistent MFA console, and provides a raw stdio TCP proxy.

There is no SSH daemon, key registration, password database or published proxy port inside the container. Local users access it through their existing Docker permissions. Remote users execute `hku proxy` through the host's ordinary authenticated SSH command channel. The destination still uses normal end-to-end SSH authentication and host-key verification.

The proxy executes netcat as a restricted container UID. IPv4/IPv6 OUTPUT rules permit that UID's sockets only through tun0 or loopback. No general established-connection exception permits fallback after tunnel loss. Optional destination restrictions are convenience CLI policy, not a security boundary against a Docker administrator.

OpenConnect runs in the foreground inside a host tmux session and interactive Docker exec. This avoids terminal closure during OpenConnect 8.20 background initialization. Credentials are entered directly into OpenConnect and are not intercepted by the CLI. The AnyConnect user agent selects HKU XML POST MFA; normal TLS verification remains enabled.

The supervised TCP keepalive waits for a configured VPN interface, resolves its independent target, validates the route, and connects as the restricted user. It records result times and state transitions. It addresses application idle accounting, not hard session lifetime or compulsory reauthentication. Restarting the container restores the background service but still requires interactive VPN login.

Release binaries embed only runtime templates, never instance data. Native amd64/arm64 runners build musl binaries; tagged publication depends on Rust, installer and Docker integration checks. The Bash installer downloads over GitHub HTTPS, checks SHA256SUMS and atomically replaces the CLI binary, without restarting deployments.
