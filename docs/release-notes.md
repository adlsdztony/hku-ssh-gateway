## Changes

- New Linux gateways use native network namespaces and systemd, with an isolated veth/NAT path and VPN-only forwarding. Docker remains optional; existing Docker configurations keep their backend.
- Linux executables embed OpenConnect 9.21, the musl loader, shared libraries, vpnc-script and fallback CA certificates. No separate VPN packages or Docker are required for native deployment.
- Preserve interactive password/Microsoft MFA login, automatic terminal return and configurable TCP keepalive. Credentials remain inside OpenConnect.
- Fix binary-stream forwarding, slow DNS redirects and Cisco TLS 1.3 STRAP authentication. TLS certificate and destination SSH host-key verification remain enabled.
- Add macOS Intel/Apple Silicon clients alongside Windows x64 and Linux amd64/arm64. Clients reuse local SSH configuration, keys and agent.
- Preserve native instance identity when relocating state, and allow administrator staging of compatible updates without interrupting an active VPN session.

## Install or update

Linux/macOS:

```sh
curl -fsSL https://github.com/adlsdztony/hku-ssh-gateway/releases/latest/download/install.sh | bash
```

Windows:

```powershell
irm https://github.com/adlsdztony/hku-ssh-gateway/releases/latest/download/install.ps1 | iex
```

For a new Linux native gateway, run `hku doctor`, `hku setup --user YOUR_UID@connect.hku.hk`, then `hku connect`. Host requirements are systemd, sudo, TUN, iproute2, iptables/ip6tables and standard shell utilities.

Existing Docker deployments remain Docker-backed. To migrate, use a separate native state directory and verify the connection before retiring Docker. Ordinary `hku up` restarts a deployment and requires MFA again.

## Validation and assets

CI covers both Linux architectures, Windows and both macOS architectures, installer checksums, native/Docker lifecycle, persistent binary forwarding, TLS trust, delayed DNS redirects, Cisco TLS 1.2/1.3 channel bindings and scoped cleanup. A live native deployment authenticated to HKU and passed campus SSH key exchange and keepalive checks.

Assets include platform executables, Bash/PowerShell installers and SHA256SUMS. Linux corresponding-source archives include upstream sources, dependency recipes/patches, license notices and build inputs.
