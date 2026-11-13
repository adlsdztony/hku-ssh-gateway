# hku

[English（默认）](README.md) | [简体中文](README.zh-CN.md)

用一个 Rust CLI 管理 Docker 中的 HKU VPN，并通过 SSH 或 TCP 流访问内网服务。**无需登记公钥，不创建额外 SSH 账号，不开放代理监听端口。**

本地 Linux 直接执行 `hku proxy`；远端通过宿主机原有 SSH 执行它。宿主机和目标服务器均使用原有的密码、密钥或 agent 认证方式。Tailscale 只是访问宿主机的可选扩展，不是依赖。默认可访问任意 VPN 可达 TCP 目标。

## 直接使用 hku ssh

网关已连接 VPN 后：

```sh
hku ssh user@internal-server.hku.hk
hku ssh -s my-gateway user@internal-server.hku.hk
hku ssh --server user@server.example.com user@internal-server.hku.hk hostname
hku ssh -s my-gateway -- -p 2222 user@internal-server.hku.hk
```

第一条使用本地 Linux 网关；`-s/--server` 仅作为客户端，复用宿主机 SSH 别名或 user@host。不传目标时会提示输入。目标也可以是本机 SSH 别名，如 `hku ssh -s my-gateway school-alias`。两跳均读取本机 SSH config、私钥和 agent，不复制或登记私钥；工具仅提供 ProxyCommand 来替换原有连接通道。

Windows 可以在 PowerShell 一键安装，无需管理员权限：

```powershell
irm https://github.com/adlsdztony/hku-ssh-gateway/releases/latest/download/install.ps1 | iex
```

脚本校验 SHA-256，安装到 `%LOCALAPPDATA%\Programs\hku`，并更新用户 PATH 和当前 PowerShell PATH。可先下载检查；参数支持 `-Version v0.1.1 -InstallDir C:/Tools/hku -NoPathUpdate`。不会修改系统 PATH 或执行策略。也可以从 Releases 手动下载 x64 ZIP。有 OpenSSH 即可使用 `hku ssh -s my-gateway ...`，**不需要 Docker**；本版不支持 Windows 本地 Docker 部署。

远端默认调用 `$HOME/.local/bin/hku`，可用 `--gateway-bin /绝对路径/hku` 和 `--remote-state-dir /网关状态目录` 改写。客户端无需本地部署配置、Docker、tmux 或 TUN。其他 SSH 参数可通过 `--` 传给本机 OpenSSH，例如 `hku ssh -s my-gateway -- -i ~/.ssh/school_key user@internal-host`。

## 安装与登录

在运行网关的 Linux 机器上：

```sh
curl -fsSL https://github.com/adlsdztony/hku-ssh-gateway/releases/latest/download/install.sh | bash
hku doctor
hku setup --user YOUR_UID@connect.hku.hk
hku connect
```

安装脚本自动识别 amd64/arm64、下载静态二进制、检查 SHA-256，安装到 `~/.local/bin/hku`；无需 Rust 编译器、源码或 sudo，不修改 shell 配置。按提示将该目录加入 PATH。可以先下载检查脚本，也可用 `--version v0.1.0 --bin-dir DIR` 固定版本和目录。

部署仍需 Linux、Docker Engine / Compose、TUN 和 tmux，`doctor` 会检查。构建需要访问 Ubuntu 镜像与软件仓库。`init` 只生成文件，`setup` 还会构建并启动。远端宿主机账号需要有普通 SSH 命令执行和 Docker 权限；Docker 权限相当于主机管理权限。

控制台按 Enter 后输入 Portal PIN/密码，在 `Response:` 输入 Microsoft MFA OTP。连接后 **Ctrl-B，再按 D** 脱离，关闭 SSH 终端不影响 VPN。再次 `connect` 可回到控制台，**Ctrl-C 会断开 VPN**。密码与验证码不写入配置。

## 使用 SSH

本地部署：

```sh
hku client-config --direct --target gpu3gate1.cs.hku.hk --target-user YOUR_UID --output ./client
ssh -F ./client/hku-ssh.config hku-target
```

远端部署，在网关上生成配置：

```sh
hku client-config --server linux_user@server.example.com \
  --target gpu3gate1.cs.hku.hk --target-user YOUR_UID --output ./client
```

把生成的 `hku-ssh.config` 复制回自己电脑，运行 `ssh -F ./hku-ssh.config hku-target`。Windows/macOS/Linux 客户端只需普通 OpenSSH，不需要安装 CLI。生成配置包含网关程序和状态目录的绝对路径，不依赖远端 shell PATH。宿主机和最终服务器仍正常验证主机密钥与登录凭据。

`--server-port` 配置宿主机端口，`--identity` 可指定客户端已有的宿主机 SSH 身份，但不是必选项，也不会登记公钥。`--gateway-bin` 可改网关程序路径。

`--target` 只是快捷别名，访问其他服务器不需要重新部署：

```sh
ssh -F ./hku-ssh.config -o HostName=other-server.hku.hk user@hku-vpn
scp -F ./hku-ssh.config -o HostName=other-server.hku.hk ./file.txt user@hku-vpn:/path/
```

可以 Include 进现有 `~/.ssh/config`，用于 VS Code。SSH `-L`/`-D` 可在登录最终服务器后按正常语义使用。`hku proxy HOST PORT` 还可为支持 stdio 代理的程序提供原始 TCP 流；不分配 TTY、不向 stdout 输出提示，不提供 UDP、ICMP 或透明全局路由。

## 配置与保活

默认状态目录为 `${XDG_DATA_HOME:-~/.local/share}/hku`，可用 `--state-dir DIR` 或 `HKU_STATE_DIR` 改写。多实例需同时使用不同目录和 project，例如 `hku --state-dir ./second setup --user YOUR_UID@hku.hk --project hku-second`。

`hku config --path` 查看配置路径。编辑 `config.toml` 后运行 `hku up`；容器重建后需重新认证。配置示例见 [config.example.toml](config.example.toml)。

默认每 300 秒经 VPN 探测一次 `gpu3gate1.cs.hku.hk:22`，不提交 SSH 账号、密码或命令。地址可换成任意有权访问、通常在线的校内 TCP 服务，**它不是访问白名单**。`keepalive.enabled = false` 可关闭，`status` 显示最近成功和失败次数。

这用于减少 1800 秒空闲超时导致的断线，不能取消硬性会话上限、强制 MFA、账号失效或网络故障。原会话还下发了 7 天上限，以实际策略为准。`allowed_targets` 默认 `["any"]`；仅在自己需要时设置目标限制，这不是对已经控制宿主机 Docker 用户的安全边界。

常用命令：`hku status`、`hku logs`、`hku disconnect`、`hku connect`、`hku down`、`hku up`。容器重启后保活服务自动恢复，VPN 仍需手动 MFA。更新 CLI 不会自动重启当前 VPN；运行 `up` 才应用新运行文件。

## 可选 Tailscale 与隔离

自行在两端安装 Tailscale 并确认普通 SSH 可用后，用 `--server linux_user@100.x.y.z` 生成配置即可。项目不配置 exit node、子网路由或 Tailscale SSH，本地和普通公网/局域网 SSH 不依赖它。

VPN 只影响容器网络，不改宿主机默认路由。没有 privileged、host networking、Docker socket 挂载、容器 SSH 服务或对外代理端口。转发和探测只能经 `tun0` 或 loopback；Docker 本地 DNS 可能代理解析，不保证 DNS 完全无旁路。配置和发行包不包含密码、OTP、会话 Cookie 或私钥。

完整排障、设计和测试范围见[英文说明](README.md)、[设计说明](docs/design.md)及[验证范围](docs/validation.md)。代码注释、CLI 输出、安装脚本和发布说明均为英文。
