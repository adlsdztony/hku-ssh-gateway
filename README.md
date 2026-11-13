# hku

[English](README.md) | [简体中文](README.zh-CN.md)

A small Rust CLI for accessing HKU VPN services through an isolated Docker container. No public-key registration, no extra SSH account, and no published proxy ports.

```text
Local Linux: SSH client → hku proxy → Docker / OpenConnect → HKU network
Remote:     SSH client → ordinary host SSH → hku proxy → Docker / VPN → HKU network
```

Local deployments need no host SSH daemon. Remote deployments reuse the host's existing SSH authentication: password, key or agent, as normally configured. Tailscale is an optional transport to the host, not a dependency. Any VPN-reachable TCP destination is allowed by default.

## Install

Run on the Linux machine that will host the VPN:

```sh
curl -fsSL https://github.com/adlsdztony/hku-ssh-gateway/releases/latest/download/install.sh | bash
```

The Bash installer selects amd64/arm64, downloads a static binary, checks SHA-256 and installs `~/.local/bin/hku`. No Rust compiler, source checkout, sudo or shell-profile modification is required. Add `~/.local/bin` to PATH if prompted. You can download and inspect the script first.

Pin a version or choose an install directory:

```sh
curl -fsSL https://github.com/adlsdztony/hku-ssh-gateway/releases/download/v0.1.0/install.sh \
  | bash -s -- --version v0.1.0 --bin-dir "$HOME/.local/bin"
```

Reinstall to update the CLI. Remove the installed binary to uninstall it; deployment state is separate. Checksums verify archive integrity; release authenticity relies on GitHub HTTPS and this repository's publishing account.

### Windows client

Download the Windows x64 ZIP from [Releases](https://github.com/adlsdztony/hku-ssh-gateway/releases), verify it against SHA256SUMS, and put `hku.exe` on PATH. Windows needs OpenSSH but **does not need Docker** when using a remote gateway. Windows local Docker deployment is not supported in this release.

## Quick SSH client

After authenticating the gateway VPN:

```sh
hku ssh user@internal-server.hku.hk
hku ssh -s my-gateway user@internal-server.hku.hk
hku ssh --server user@server.example.com user@internal-server.hku.hk hostname
hku ssh -s my-gateway -- -p 2222 user@internal-server.hku.hk
```

The first command uses a local Linux deployment. `-s/--server` uses an existing SSH host alias or user@host and works as a client on Windows, macOS and Linux. With no destination, the tool prompts for one. An optional leading `ssh` word is accepted. SSH options and destination commands are passed to the local OpenSSH client.

Both host and destination authentication use the client's existing SSH config, private keys or agent. Nothing registers or copies private keys. For example, `hku ssh -s my-gateway school-alias` uses both aliases from the local `~/.ssh/config`; explicit identities also work with `hku ssh -s my-gateway -- -i ~/.ssh/school_key user@internal-host`. Only ProxyCommand is supplied by the tool, replacing any previous route through ProxyCommand/ProxyJump.

The remote executable defaults to `$HOME/.local/bin/hku`; use `--gateway-bin /absolute/path/to/hku` for another installation and `--remote-state-dir /gateway/state` if needed. Remote client mode reads no local deployment state and requires only `hku` and `ssh`, not Docker/tmux/TUN. The remote VPN must already be connected.

## Set up

The host requires **Linux, Docker Engine / Compose, TUN and tmux**. Check prerequisites:

```sh
hku doctor
hku setup --user YOUR_UID@connect.hku.hk
hku connect
```

`setup` writes configuration, materializes the runtime embedded in the CLI, builds the Docker image and starts the service. `hku init` only prepares files. Builds need access to Ubuntu images and package repositories. Repeated setup keeps existing settings unless an option is supplied explicitly. Docker setup follows the [official instructions](https://docs.docker.com/engine/install/ubuntu/); on Ubuntu with Docker already installed, `sudo apt-get install tmux` supplies the remaining host package.

In the console, press Enter, enter your Portal PIN/password, then enter your Microsoft OTP at `Response:`. Once connected, press **Ctrl-B, then D** to detach. The VPN survives your terminal/SSH disconnection. Run `hku connect` to reattach; **Ctrl-C disconnects the VPN**. Authentication inputs are never saved or captured by the CLI.

Remote administration can use `ssh -t user@server /absolute/path/to/hku connect`. The host account must have Docker access and permission to execute commands. Docker access grants host administration capabilities. There is no additional container SSH login or key allowlist.

State lives under `${XDG_DATA_HOME:-~/.local/share}/hku`. Use `--state-dir DIR` or `HKU_STATE_DIR` to override it. Each independent deployment needs its own state directory **and** Compose project name:

```sh
hku --state-dir ./second setup --user YOUR_UID@hku.hk --project hku-second
```

## Use SSH

### Local deployment

```sh
hku client-config --direct --target gpu3gate1.cs.hku.hk --target-user YOUR_UID --output ./client
ssh -F ./client/hku-ssh.config hku-target
```

### Remote deployment

On the gateway host, generate configuration using its public/LAN address or DNS name:

```sh
hku client-config --server linux_user@server.example.com \
  --target gpu3gate1.cs.hku.hk --target-user YOUR_UID --output ./client
```

Copy `client/hku-ssh.config` to your computer, then run `ssh -F ./hku-ssh.config hku-target`. Windows, macOS and Linux clients can use normal OpenSSH; the CLI is only required on the gateway host. Host and final-server authentication use normal SSH password/key/agent settings, with normal host-key checks. No private keys are copied into the container.

Use `--server-port PORT` for a nonstandard host SSH port, `--identity '~/.ssh/your_key'` for an optional existing client identity, and `--gateway-bin /path/to/hku` to override the executable path on the gateway. The generated ProxyCommand includes the absolute gateway executable and state path, so it does not depend on a remote shell's PATH.

`--target` only creates an alias. Omit it if desired. The generic `hku-vpn` alias accepts any destination via `HostName`:

```sh
ssh -F ./hku-ssh.config -o HostName=other-server.hku.hk user@hku-vpn
scp -F ./hku-ssh.config -o HostName=other-server.hku.hk ./file.txt user@hku-vpn:/path/
```

Add more Host aliases with the same ProxyCommand, or include the generated config in your existing `~/.ssh/config` for VS Code Remote SSH. Avoid alias conflicts between deployments. Destination shell commands, SCP and VS Code run on the final server. SSH `-L`/`-D` remain available after logging into that server, with their usual forwarding semantics.

The raw gateway stream is also available as `hku proxy HOST PORT`, for applications that support a stdio TCP proxy. It opens a socket as a restricted user inside the VPN container, without allocating a TTY or printing banners to stdout. It does not expose an unauthenticated listening port. UDP, ICMP and transparent system routing are outside its scope.

## Keepalive and configuration

`hku config --path` locates `config.toml`; `hku config` prints it. Edit the file and run `hku up` to apply changes. Container recreation requires new VPN authentication. See [the example configuration](config.example.toml).

```toml
vpn_user = "YOUR_UID@connect.hku.hk"
vpn_server = "vpn2fa.hku.hk"
vpn_useragent = "AnyConnect"
allowed_targets = ["any"]
project = "hku-ssh-gateway"

[keepalive]
enabled = true
host = "gpu3gate1.cs.hku.hk"
port = 22
interval = 300
timeout = 5
```

Every five minutes the probe establishes a TCP connection through the VPN, without submitting credentials or commands. Choose any authorized, normally available internal TCP service. The target is **not** an access allowlist. Set `enabled = false` to disable probes. `hku status` reports the most recent success and failure count; service logs record state changes.

The observed HKU session advertised an idle timeout of 1800 seconds. Protocol Keepalive/DPD maintains transport connectivity; this separate probe generates actual tunneled traffic to reduce idle disconnections. It cannot override hard session lifetime, forced MFA, account expiration or network failures. The observed session also advertised a seven-day maximum; policy can vary. See [Cisco's explanation](https://www.cisco.com/c/en/us/support/docs/security/anyconnect-secure-mobility-client/116312-qanda-anyconnect-00.html).

Optional restrictions use `allowed_targets = ["host1.hku.hk:22", "10.0.0.1:443"]`, matched against the requested destination. They are CLI policy, not a security boundary against a host user who already controls Docker. Default `any` also permits public TCP destinations reachable through the VPN. Probe and VPN configuration currently accept DNS/IPv4 hosts; raw proxy destinations also accept IPv6 literals when the VPN supports them.

## Manage

```sh
hku --help
hku doctor
hku connect                 # Alias: login
hku status                  # Exit 2 if the VPN interface is not configured
hku logs
hku disconnect              # Alias: logout
hku down
hku up
```

Container/host restarts recover the probe service, but VPN MFA still needs `connect`. Docker health reports daemon liveness, not authenticated VPN readiness. Updating the CLI does not restart an active deployment; `up` applies the new embedded runtime.

## Optional: Tailscale

Install [Tailscale](https://tailscale.com/kb/1017/install) on the host and client if you want private cross-network host access. Once ordinary SSH works, use `hku client-config --server linux_user@100.x.y.z`. This project does not configure Tailscale SSH, subnet routing or exit nodes. Local and public/LAN SSH deployments work without it.

## Isolation and troubleshooting

- Only the container receives NET_ADMIN and TUN. No privileged mode, host networking, Docker socket mount, published port, SSH daemon or proxy authentication database is used.
- `hku proxy` requires local Docker access, directly or through an authenticated host SSH command. The default state directory is private to its owner.
- Container UID-based IPv4/IPv6 OUTPUT rules allow forwarded/probe sockets only over `tun0` or loopback. Established sockets cannot fall back to ordinary networking. Docker's loopback DNS resolver may proxy lookups; DNS leak prevention is not guaranteed.
- Host routing stays unchanged. CLI configuration and runtime files contain no password, OTP, cached authentication cookie or private key.
- Missing MFA prompt: keep `vpn_useragent = "AnyConnect"`, which selects HKU XML POST authentication ([upstream explanation](https://lists.infradead.org/pipermail/openconnect-devel/2023-May/005315.html)).
- `pre-init: Interrupted system call`: use `hku connect`, keeping OpenConnect in a persistent foreground console rather than backgrounding it inside Docker exec.
- Destination `Permission denied`: check the final server's normal SSH credentials. Transport connectivity does not grant destination access.
- Remote ProxyCommand failure: check host SSH command execution, Docker permissions and the absolute gateway executable/state paths in the generated file.

## Development and releases

```sh
cargo build --release
cargo test --locked
cargo clippy --locked --all-targets -- -D warnings
bash tests/installer.sh
bash tests/smoke.sh "$PWD/target/release/hku"
```

Integration tests require a Linux Docker host, not HKU credentials. CI tests keyless setup, no exposed ports, VPN-only streams, keepalive, generated configuration and restart recovery. Native amd64/arm64 runners build static musl binaries. A `vX.Y.Z` tag matching Cargo.toml publishes archives, installer and SHA256SUMS after checks pass.

See [design](docs/design.md), [validation scope](docs/validation.md) and [release notes](docs/release-notes.md).
