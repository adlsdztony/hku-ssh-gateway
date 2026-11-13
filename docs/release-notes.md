## Changes

- `hku connect` returns automatically once the VPN process, address and route are ready. No manual terminal detach is required after password/MFA entry.
- Remove the tmux dependency. OpenConnect runs in Docker's persistent terminal; the CLI closes only its own attach client and restores terminal settings.
- Keep authentication entirely inside OpenConnect: no credential-output parsing, cookie extraction or credential handoff.
- Add terminal lifecycle coverage for automatic return while stdin stays open, surviving VPN processes, hidden input, terminal restoration, failed login and retry.
- Keep English and Chinese quick-start documentation synchronized.

## Upgrade

Reinstall the CLI on the Linux gateway:

```sh
curl -fsSL https://github.com/adlsdztony/hku-ssh-gateway/releases/latest/download/install.sh | bash
hku up
hku connect
```

Upgrading an older runtime recreates the container and disconnects its VPN session, so authenticate again when prompted. After login, the command returns automatically and the VPN keeps running. Existing configuration and normal local SSH keys/config remain in use.

Windows client update:

```powershell
irm https://github.com/adlsdztony/hku-ssh-gateway/releases/latest/download/install.ps1 | iex
```

Assets include Linux amd64/arm64 binaries, the Windows x64 client, both installers and SHA256SUMS. The Linux gateway requires Docker Engine, Compose, TUN and standard Linux tools. Windows remote-client mode needs OpenSSH, not Docker.
