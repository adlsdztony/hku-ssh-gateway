# hku

[English](README.md) | [简体中文](README.zh-CN.md)

通过隔离的 Linux VPN 网关访问校内服务的 Rust CLI。新部署默认使用原生网络命名空间，Docker 保留为可选后端。默认允许任意 VPN 可达的 TCP 目标。

> 本分支的原生后端仍处于实验阶段，尚未发布。下面的 Release 安装脚本仍提供稳定的 Docker 版本；体验本分支请参见[源码构建与内置运行包](docs/bundled-runtime.md)。

## 安装

### Linux / macOS

```sh
curl -fsSL https://github.com/adlsdztony/hku-ssh-gateway/releases/latest/download/install.sh | bash
```

安装到 `~/.local/bin`，按提示加入 PATH。
macOS 只支持远端客户端模式，需要 OpenSSH，不需要 Docker。

### Windows

```powershell
irm https://github.com/adlsdztony/hku-ssh-gateway/releases/latest/download/install.ps1 | iex
```

安装到 `%LOCALAPPDATA%\Programs\hku` 并更新用户 PATH。Windows 通过 OpenSSH 使用远端客户端模式，不需要 Docker。

两个安装脚本均校验 SHA-256，无需管理员权限。重新运行即可更新，也可从 [Releases](https://github.com/adlsdztony/hku-ssh-gateway/releases) 下载二进制。

## 配置网关

在 **Linux 网关上**执行。Linux CI/Release 二进制内置 OpenConnect、运行库和 vpnc-script，无需单独安装 VPN 软件包。宿主机需要 systemd、iproute2、iptables、TUN、sudo 和标准 shell 工具。如果 `hku doctor` 在 Ubuntu/Debian 上提示缺少网络工具：

```sh
sudo apt-get install iproute2 iptables
```

网关可以是本地 Linux，也可以是通过普通 SSH 访问的服务器。使用远端网关时，两端都安装 `hku`；Windows 和 macOS 仅作为客户端。

```sh
hku doctor
hku setup --user YOUR_UID@connect.hku.hk
hku connect
```

原生 setup 会请求 sudo，安装 root 管理的 systemd 服务，并创建独立 netns、veth 和 NAT 规则。需要时会启用宿主机 IPv4 转发，关闭时保留这个共享设置；宿主机默认路由和 DNS 保持不变。日常连接、代理和状态命令通过仅所属用户可访问的 Unix socket 工作，无需 sudo。

内置文件经过校验和检查，解压到私有的版本目录。更新 `hku` 即可更新这些依赖，不会替换宿主机运行库。Windows/macOS 客户端不包含 VPN 运行包。

按提示输入 HKU 密码和 Microsoft MFA 验证码。**VPN 就绪后命令自动返回**，后台服务继续运行，无需手动脱离或使用 tmux。认证仍由 OpenConnect 完成。

### 可选 Docker 后端

未包含 `backend` 字段的旧配置继续使用 Docker。新的 Docker 部署需要 Docker Engine 和 Compose：

```sh
hku --state-dir ~/.local/share/hku-docker setup --backend docker --user YOUR_UID@connect.hku.hk
```

比较两种后端时使用不同状态目录，不会修改已有部署。

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

宿主机重启或运行环境重建后，运行 `hku connect` 重新认证。原生 `up`/`down` 会请求 sudo；`down` 仅清理该实例的命名空间和防火墙规则，保留配置。

## 配置与保活

用 `hku config --path` 找到 `config.toml`，编辑后运行 `hku up` 应用；运行环境重启后需重新登录 VPN。参见[配置示例](config.example.toml)。

默认每 **5 分钟**进行一次 TCP 探测，减少空闲断线。探测地址可修改，不限制你能访问的服务器；设 `keepalive.enabled = false` 可关闭。保活不能取消硬性会话到期或 MFA 要求。

## 更多

- **SCP / VS Code：**可选用 `hku client-config --help` 生成标准 SSH 配置。
- **Tailscale：**可选，网关的 tailnet 地址按普通 SSH 地址使用。本地部署和公网/局域网 SSH 均不依赖它。
- **详细说明：**[原生后端及权限模型](docs/native.md)、[设计](docs/design.md)、[验证](docs/validation.md)，或 `hku --help`。
