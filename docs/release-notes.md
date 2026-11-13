`hku` packages an isolated HKU VPN SSH/TCP gateway into a Rust CLI.

- Single-binary Linux installation for amd64 and arm64; no Rust toolchain required.
- Windows x64 client: `hku ssh -s SERVER user@internal-host`, without Docker.
- Local `hku ssh user@host` and normal local SSH config/key/agent support.
- Embedded Docker runtime, typed TOML configuration and interactive MFA in a persistent console.
- No public-key registration or extra authentication layer; reuse normal host SSH authentication.
- Local deployment or ordinary SSH to a remote host; Tailscale is optional.
- Any VPN-reachable TCP destination by default, with optional restrictions.
- Configurable TCP keepalive, service supervision and VPN-only forwarding rules.
- SHA-256 verified Bash installer; supports pinned versions and custom install directories.
- SHA-256 verified Windows PowerShell installer with per-user PATH setup and an opt-out.

Install:

```sh
curl -fsSL https://github.com/adlsdztony/hku-ssh-gateway/releases/latest/download/install.sh | bash
```

Then run `hku doctor`, `hku setup --user YOUR_UID@connect.hku.hk`, and `hku connect`.

Docker Engine, Docker Compose, TUN and tmux are deployment prerequisites. Passwords and MFA codes are entered interactively and never stored. Automatic probes do not override session expiration or forced reauthentication policies.

Windows client installation:

```powershell
irm https://github.com/adlsdztony/hku-ssh-gateway/releases/latest/download/install.ps1 | iex
```

Windows client mode needs OpenSSH, not Docker.
