# hku

[English](README.md) | [简体中文](README.zh-CN.md)

A Rust CLI for accessing HKU internal services through an isolated Linux VPN gateway. Native network namespaces are the default for new deployments; Docker remains optional. Any VPN-reachable TCP destination is allowed by default.

> The native backend is experimental on this branch and is not yet released. Release installers below still provide the stable Docker version. Build this branch with `cargo build --release` to try native mode.

## Install

### Linux / macOS

```sh
curl -fsSL https://github.com/adlsdztony/hku-ssh-gateway/releases/latest/download/install.sh | bash
```

Installs to `~/.local/bin`. Add it to PATH if prompted.
macOS supports remote-client mode only, with OpenSSH and no Docker requirement.

### Windows

```powershell
irm https://github.com/adlsdztony/hku-ssh-gateway/releases/latest/download/install.ps1 | iex
```

Installs to `%LOCALAPPDATA%\Programs\hku` and updates your user PATH. Windows supports remote-client mode with OpenSSH; Docker is not required.

Both installers verify SHA-256 and need no administrator privileges. Rerun to update, or download binaries from [Releases](https://github.com/adlsdztony/hku-ssh-gateway/releases).

## Setup

Run this **on the Linux gateway**. Native mode needs systemd, OpenConnect, vpnc-scripts, iproute2, iptables, TUN and sudo. On Ubuntu/Debian:

```sh
sudo apt-get install openconnect vpnc-scripts iproute2 iptables
```

The gateway can be local Linux or a server accessible through ordinary SSH. For a remote gateway, install `hku` on both sides; Windows and macOS are clients only.

```sh
hku doctor
hku setup --user YOUR_UID@connect.hku.hk
hku connect
```

Native setup requests sudo to install a root-owned systemd service and configure an isolated netns, veth and NAT rules. It enables host IPv4 forwarding if necessary and leaves that shared setting enabled on shutdown. Host default routing and DNS stay unchanged. Daily connect/proxy/status commands run without sudo through an owner-only Unix socket.

Follow the prompts for your HKU password and Microsoft MFA code. **The command returns automatically when the VPN is ready**. The background service keeps running; no manual detach or tmux is needed. Authentication remains inside OpenConnect.

### Optional Docker backend

Existing configurations without a `backend` field keep using Docker. New Docker deployments require Docker Engine and Compose:

```sh
hku --state-dir ~/.local/share/hku-docker setup --backend docker --user YOUR_UID@connect.hku.hk
```

Use separate state directories to compare backends; switching does not modify an existing deployment.

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

After a host restart or runtime recreation, run `hku connect` to authenticate again. Native `up`/`down` request sudo; `down` removes only that instance's namespace and firewall rules and preserves configuration.

## Configuration and keepalive

Find `config.toml` with `hku config --path`. Edit it, then run `hku up` to apply changes; restarting the runtime requires VPN login again. See [example configuration](config.example.toml).

By default, a TCP probe runs every **5 minutes** to reduce idle disconnections. Its target is configurable and does not restrict which servers you can access. Set `keepalive.enabled = false` to disable it. Probes cannot override hard session expiration or required MFA.

## More

- **SCP / VS Code:** optionally generate standard SSH configuration with `hku client-config --help`.
- **Tailscale:** optional; use the gateway's tailnet address like any other SSH address. Public/LAN SSH and local deployment work without it.
- **Details:** [native backend and privilege model](docs/native.md), [design](docs/design.md), [validation](docs/validation.md), or `hku --help`.
