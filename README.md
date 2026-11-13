# hku

[English](README.md) | [简体中文](README.zh-CN.md)

A Rust CLI for accessing HKU internal services through a Docker-isolated VPN, locally or through an SSH gateway. Host routing stays unchanged; any VPN-reachable TCP destination is allowed by default.

## Install

### Linux

```sh
curl -fsSL https://github.com/adlsdztony/hku-ssh-gateway/releases/latest/download/install.sh | bash
```

Installs to `~/.local/bin`. Add it to PATH if prompted.

### Windows

```powershell
irm https://github.com/adlsdztony/hku-ssh-gateway/releases/latest/download/install.ps1 | iex
```

Installs to `%LOCALAPPDATA%\Programs\hku` and updates your user PATH. Windows supports remote-client mode with OpenSSH; Docker is not required.

Both installers verify SHA-256 and need no administrator privileges. Rerun to update, or download binaries from [Releases](https://github.com/adlsdztony/hku-ssh-gateway/releases).

## Setup

Run this **on the Linux gateway**, which needs Docker Engine with Compose, TUN and standard Linux tools. The gateway can be your local Linux machine or a server accessible through ordinary SSH. For a remote gateway, install `hku` on both the gateway and your client; only the gateway needs Docker.

```sh
hku doctor
hku setup --user YOUR_UID@connect.hku.hk
hku connect
```

Press Enter to start, then enter your HKU password and Microsoft MFA code when prompted. **The command returns automatically when the VPN is ready**, and the VPN keeps running in Docker. No manual detach or tmux is needed. Passwords and MFA codes are not stored.

## Quick SSH Client (recommended)

**Use `hku ssh` for everyday SSH access.** After connecting the gateway VPN:

```sh
# Local Linux gateway
hku ssh user@internal-host

# Remote gateway: my-gateway is an existing SSH alias or user@host
hku ssh -s my-gateway user@internal-host

# Existing destination alias from your local SSH config
hku ssh -s my-gateway school-alias
```

Both SSH hops use your **local SSH config, private keys and agent**. No key registration or copying is needed. Remote-client mode needs only `hku` and OpenSSH on the client.

Pass SSH options or a remote command as usual:

```sh
hku ssh -s my-gateway -- -i ~/.ssh/school_key user@internal-host hostname
```

For custom gateway executable/state paths, see `hku ssh --help`.

## Manage

```sh
hku status
hku logs
hku disconnect
hku connect
hku down
hku up
```

After a host restart or container recreation, run `hku connect` to authenticate again.

## Configuration and keepalive

Find `config.toml` with `hku config --path`. Edit it, then run `hku up` to apply changes; container recreation requires VPN login again. See [example configuration](config.example.toml).

By default, a TCP probe runs every **5 minutes** to reduce idle disconnections. Its target is configurable and does not restrict which servers you can access. Set `keepalive.enabled = false` to disable it. Probes cannot override hard session expiration or required MFA.

## More

- **SCP / VS Code:** optionally generate standard SSH configuration with `hku client-config --help`.
- **Tailscale:** optional; use the gateway's tailnet address like any other SSH address. Public/LAN SSH and local deployment work without it.
- **Details:** [design](docs/design.md), [validation](docs/validation.md), [release notes](docs/release-notes.md), or `hku --help`.
