# Validation scope

## Native Linux branch

Privileged integration runs on a dedicated Ubuntu CI VM using real systemd, netns, veth and firewall rules. A mock OpenConnect reads hidden password/MFA fixture input from its terminal and establishes a simulated tunnel. Checks cover owner-only control, cross-user denial, automatic return with restored terminal settings, a persistent VPN process, successful TCP forwarding, blocked ordinary-network fallback, keepalive, DNS resolution through the tunnel, unchanged host default routing and resolver, and repeatable scoped cleanup. No school credentials or live VPN sessions are used.

Linux amd64/arm64, Windows x64 and macOS Intel/Apple Silicon builds are covered. Windows/macOS reject gateway management and preserve remote SSH/proxy client operation without Docker. Old TOML remains Docker-backed. Native HKU password/MFA and campus reachability still require user acceptance testing; compilation and simulated integration do not prove those live results.

## Existing Docker implementation

The original container implementation was tested with real HKU password/MFA, CSTP/DTLS, persistent tmux detachment, internal DNS and a complete destination SSH handshake from a Windows client. The destination host key matched the existing known_hosts record. Destination shell login was not verified because the test key was not authorized there.

The live session advertised a 1800-second idle timeout and 604800-second maximum duration. Repeated 300-second TCP probes succeeded without restarting the authenticated VPN. This verifies actual probe traffic, not every possible future gateway policy or seven days of operation.

The CLI pipeline checks configuration roundtrips and validation, embedded runtime generation, installer success/checksum failure, setup without any SSH key, absence of published ports, rejection of ordinary-network proxy streams, successful simulated-VPN streams, probe success/route rejection, local/remote SSH config parsing and container restart recovery.

Automatic login-return tests use a compiled terminal fixture in a separate container. It reads hidden test inputs directly from its terminal and configures a simulated VPN interface/route. The CLI must return while its input remains open, preserve the running VPN process, restore terminal settings, avoid echoing input, report authentication failure, and allow a successful retry. These checks passed on an independent Ubuntu Docker host and in CI. A user also reported that the preview worked with their existing HKU deployment before the v0.1.2 release.

The simulated VPN is an extra temporary Docker interface, not an HKU login. Tests clean up their containers, networks and temporary state. Native amd64/arm64 runners compile and test static binaries. CI never receives school passwords, OTPs, cookies or private keys. Legitimate users must authenticate and verify real campus reachability on their deployment.
