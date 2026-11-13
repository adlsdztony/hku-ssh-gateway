# Validation scope

The original container implementation was tested with real HKU password/MFA, CSTP/DTLS, persistent tmux detachment, internal DNS and a complete destination SSH handshake from a Windows client. The destination host key matched the existing known_hosts record. Destination shell login was not verified because the test key was not authorized there.

The live session advertised a 1800-second idle timeout and 604800-second maximum duration. Repeated 300-second TCP probes succeeded without restarting the authenticated VPN. This verifies actual probe traffic, not every possible future gateway policy or seven days of operation.

The CLI pipeline checks configuration roundtrips and validation, embedded runtime generation, installer success/checksum failure, setup without any SSH key, absence of published ports, rejection of ordinary-network proxy streams, successful simulated-VPN streams, probe success/route rejection, local/remote SSH config parsing and container restart recovery.

The simulated VPN is an extra temporary Docker interface, not an HKU login. Tests clean up their containers, networks and temporary state. Native amd64/arm64 runners compile and test static binaries. CI never receives school passwords, OTPs, cookies or private keys. Legitimate users must authenticate and verify real campus reachability on their deployment.
