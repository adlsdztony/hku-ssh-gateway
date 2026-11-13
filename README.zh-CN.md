# hku

[English](README.md) | [简体中文](README.zh-CN.md)

通过 Docker 隔离的 HKU VPN，在本地或经 SSH 网关访问校内服务的 Rust CLI。不改变宿主机路由，默认允许任意 VPN 可达的 TCP 目标。

## 安装

### Linux

```sh
curl -fsSL https://github.com/adlsdztony/hku-ssh-gateway/releases/latest/download/install.sh | bash
```

安装到 `~/.local/bin`，按提示加入 PATH。

### Windows

```powershell
irm https://github.com/adlsdztony/hku-ssh-gateway/releases/latest/download/install.ps1 | iex
```

安装到 `%LOCALAPPDATA%\Programs\hku` 并更新用户 PATH。Windows 通过 OpenSSH 使用远端客户端模式，不需要 Docker。

两个安装脚本均校验 SHA-256，无需管理员权限。重新运行即可更新，也可从 [Releases](https://github.com/adlsdztony/hku-ssh-gateway/releases) 下载二进制。

## 配置网关

在 **Linux 网关上**执行，需要 Docker Engine / Compose、TUN 和标准 Linux 工具。网关可以是本地 Linux，也可以是通过普通 SSH 访问的服务器。使用远端网关时，两端都安装 `hku`，只有网关需要 Docker。

```sh
hku doctor
hku setup --user YOUR_UID@connect.hku.hk
hku connect
```

按 Enter 开始，再按提示输入 HKU 密码和 Microsoft MFA 验证码。**VPN 就绪后命令自动返回**，VPN 继续在 Docker 后台运行，无需手动脱离或使用 tmux。密码和验证码不保存。

## 快速 SSH 连接（推荐）

**日常 SSH 访问推荐使用 `hku ssh`。** 网关连上 VPN 后：

```sh
# Local Linux gateway
hku ssh user@internal-host

# Remote gateway: my-gateway is an existing SSH alias or user@host
hku ssh -s my-gateway user@internal-host

# Existing destination alias from your local SSH config
hku ssh -s my-gateway school-alias
```

两跳均使用**本机 SSH 配置、私钥和 agent**，无需登记或复制密钥。远端客户端只需要 `hku` 和 OpenSSH。

其他 SSH 参数或远端命令照常传入：

```sh
hku ssh -s my-gateway -- -i ~/.ssh/school_key user@internal-host hostname
```

自定义网关程序或状态目录路径，见 `hku ssh --help`。

## 管理

```sh
hku status
hku logs
hku disconnect
hku connect
hku down
hku up
```

宿主机重启或容器重建后，运行 `hku connect` 重新认证。

## 配置与保活

用 `hku config --path` 找到 `config.toml`，编辑后运行 `hku up` 应用；容器重建后需重新登录 VPN。参见[配置示例](config.example.toml)。

默认每 **5 分钟**进行一次 TCP 探测，减少空闲断线。探测地址可修改，不限制你能访问的服务器；设 `keepalive.enabled = false` 可关闭。保活不能取消硬性会话到期或 MFA 要求。

## 更多

- **SCP / VS Code：**可选用 `hku client-config --help` 生成标准 SSH 配置。
- **Tailscale：**可选，网关的 tailnet 地址按普通 SSH 地址使用。本地部署和公网/局域网 SSH 均不依赖它。
- **详细说明：**[设计](docs/design.md)、[验证](docs/validation.md)、[发布说明](docs/release-notes.md)，或 `hku --help`。
