mod config;
mod connection;
#[cfg(target_os = "linux")]
mod native;
mod runtime;
#[cfg(windows)]
mod windows_job;

use anyhow::{bail, ensure, Context, Result};
use clap::{Args, Parser, Subcommand};
use config::{Backend, Config};
use runtime::{private_write, success, Runtime};
use std::ffi::OsString;
use std::fs;
use std::io::{self, IsTerminal, Write};
use std::path::PathBuf;
use std::process::{Command, ExitCode, Stdio};

#[derive(Parser)]
#[command(
    name = "hku",
    version,
    about = "Isolated HKU VPN gateway for SSH and TCP forwarding"
)]
struct Cli {
    /// Deployment state directory (defaults to XDG_DATA_HOME/hku)
    #[arg(long, global = true, env = "HKU_STATE_DIR")]
    state_dir: Option<PathBuf>,
    #[command(subcommand)]
    command: Action,
}

#[derive(Subcommand)]
enum Action {
    /// Initialize and start the gateway; run connect afterwards for VPN MFA
    Setup(Setup),
    /// Prepare config and embedded runtime without starting Docker
    Init(Setup),
    /// Check host dependencies without changing the system
    Doctor {
        #[arg(long)]
        backend: Option<Backend>,
    },
    /// Build/start using config.toml; container recreation requires new MFA
    Up,
    /// Log in interactively and return automatically when the VPN is ready
    #[command(alias = "login")]
    Connect,
    /// Show VPN and keepalive state (exit 2 if the VPN is offline)
    Status,
    /// Disconnect the VPN gracefully
    #[command(alias = "logout")]
    Disconnect,
    /// Stop and remove the deployment containers
    Down,
    /// Show recent service logs
    Logs,
    /// Print config.toml, or just its path
    Config {
        #[arg(long)]
        path: bool,
    },
    /// Generate client SSH config; no public key registration is required
    ClientConfig(Client),
    /// SSH through a local VPN, or use -s HOST as a client of a remote gateway
    Ssh(Ssh),
    /// Carry a raw TCP stream over the VPN (for SSH ProxyCommand)
    Proxy(Proxy),
    #[cfg(target_os = "linux")]
    #[command(hide = true, subcommand)]
    NativeInternal(native::Internal),
}

#[derive(Args, Clone, Default)]
struct Remote {
    /// Existing SSH host alias or user@hostname of a remote gateway
    #[arg(short = 's', long)]
    server: Option<String>,
    /// Gateway executable on the remote host (default: $HOME/.local/bin/hku)
    #[arg(long)]
    gateway_bin: Option<String>,
    /// Optional state directory on the remote gateway
    #[arg(long)]
    remote_state_dir: Option<String>,
}

#[derive(Args)]
struct Ssh {
    #[command(flatten)]
    remote: Remote,
    /// SSH arguments, destination and optional remote command; use -- if needed
    #[arg(trailing_var_arg = true, allow_hyphen_values = true)]
    command: Vec<OsString>,
}

#[derive(Args)]
struct Proxy {
    #[command(flatten)]
    remote: Remote,
    host: String,
    #[arg(value_parser = clap::value_parser!(u16).range(1..))]
    port: u16,
}

#[derive(Args)]
struct Setup {
    /// Linux gateway backend (new deployments default to native)
    #[arg(long)]
    backend: Option<Backend>,
    #[arg(long)]
    user: Option<String>,
    #[arg(long)]
    project: Option<String>,
    #[arg(long)]
    vpn_server: Option<String>,
    /// Optional destination restriction; default is any VPN-reachable TCP host
    #[arg(long)]
    allow: Option<String>,
    #[arg(long)]
    keepalive_host: Option<String>,
    #[arg(long)]
    keepalive_interval: Option<u32>,
    #[arg(long)]
    no_keepalive: bool,
}

#[derive(Args)]
struct Client {
    /// Local deployment: invoke this CLI directly as the TCP proxy
    #[arg(long, conflicts_with = "server", required_unless_present = "server")]
    direct: bool,
    /// Remote deployment: reuse ordinary SSH authentication to user@host
    #[arg(long, required_unless_present = "direct")]
    server: Option<String>,
    #[arg(long, default_value_t = 22)]
    server_port: u16,
    /// Optional existing host SSH identity path ON THE CLIENT
    #[arg(long)]
    identity: Option<String>,
    /// Executable path ON THE GATEWAY HOST (default: this executable's path)
    #[arg(long)]
    gateway_bin: Option<String>,
    /// Optional target alias; does not create an access allowlist
    #[arg(long, requires = "target_user")]
    target: Option<String>,
    #[arg(long, requires = "target")]
    target_user: Option<String>,
    #[arg(long)]
    output: Option<PathBuf>,
}

fn home() -> Result<PathBuf> {
    std::env::var_os("HOME")
        .or_else(|| std::env::var_os("USERPROFILE"))
        .map(PathBuf::from)
        .context("Cannot find home directory; use --state-dir")
}
fn default_state() -> Result<PathBuf> {
    let base = match std::env::var_os("XDG_DATA_HOME") {
        Some(p) => PathBuf::from(p),
        None => home()?.join(".local/share"),
    };
    Ok(base.join("hku"))
}
fn expand_home(path: PathBuf) -> Result<PathBuf> {
    if let Ok(rest) = path.strip_prefix("~") {
        return Ok(home()?.join(rest));
    }
    Ok(path)
}
fn prompt(label: &str) -> Result<String> {
    ensure!(
        io::stdin().is_terminal(),
        "Provide --user EMAIL in noninteractive mode"
    );
    print!("{label}: ");
    io::stdout().flush()?;
    let mut text = String::new();
    io::stdin().read_line(&mut text)?;
    Ok(text.trim().into())
}
fn doctor(backend: Option<Backend>) -> Result<bool> {
    if !cfg!(target_os = "linux") {
        let found = Command::new("ssh")
            .arg("-V")
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .status()
            .is_ok_and(|s| s.success());
        println!(
            "{} OpenSSH client (this platform supports remote client mode only)",
            if found { "OK" } else { "MISSING" }
        );
        return Ok(found);
    }
    let backend = backend.unwrap_or_else(|| {
        Runtime::new(default_state().unwrap_or_default())
            .and_then(|r| r.load())
            .map(|c| c.backend)
            .unwrap_or(Backend::Native)
    });
    #[cfg(target_os = "linux")]
    if backend == Backend::Native {
        return native::doctor();
    }
    let _ = backend;
    let mut ok = cfg!(target_os = "linux");
    println!(
        "{} Linux host ({})",
        if ok { "OK" } else { "MISSING" },
        std::env::consts::OS
    );
    for (program, args) in [
        ("docker", vec!["info"]),
        ("docker", vec!["compose", "version"]),
        ("stty", vec!["--version"]),
    ] {
        let found = Command::new(program)
            .args(&args)
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .status()
            .is_ok_and(|s| s.success());
        println!(
            "{} {} {}",
            if found { "OK" } else { "MISSING" },
            program,
            args.join(" ")
        );
        ok &= found;
    }
    #[cfg(unix)]
    let tun = {
        use std::os::unix::fs::FileTypeExt;
        fs::metadata("/dev/net/tun").is_ok_and(|m| m.file_type().is_char_device())
    };
    #[cfg(not(unix))]
    let tun = false;
    println!("{} /dev/net/tun", if tun { "OK" } else { "MISSING" });
    ok &= tun;
    if !ok {
        println!("Install/enable the missing prerequisites. hku does not install Docker or modify system packages. See README.");
    }
    Ok(ok)
}
fn initialize(runtime: &Runtime, options: Setup) -> Result<()> {
    ensure!(
        cfg!(target_os = "linux"),
        "Gateway deployment is Linux-only. Use hku ssh -s SERVER DESTINATION in client mode"
    );
    let mut config = if runtime.config_path().exists() {
        runtime.load()?
    } else {
        Config::default()
    };
    if let Some(backend) = options.backend {
        ensure!(
            !runtime.config_path().exists() || config.backend == backend,
            "Use a separate --state-dir to switch backends without changing an existing deployment"
        );
        config.backend = backend;
    }
    if let Some(v) = options.user {
        config.vpn_user = v;
    }
    if config.vpn_user.is_empty() {
        config.vpn_user = prompt("HKU email (uid@connect.hku.hk or uid@hku.hk)")?;
    }
    if let Some(v) = options.project {
        config.project = v;
    }
    if let Some(v) = options.vpn_server {
        config.vpn_server = v;
    }
    if let Some(v) = options.allow {
        config.allowed_targets = v.split_whitespace().map(String::from).collect();
    }
    if let Some(v) = options.keepalive_host {
        config.keepalive.host = v;
    }
    if let Some(v) = options.keepalive_interval {
        config.keepalive.interval = v;
    }
    if options.no_keepalive {
        config.keepalive.enabled = false;
    }
    runtime.save(&config)?;
    runtime.sync()?;
    println!("Initialized {}", runtime.dir.display());
    println!("Edit settings: {}", runtime.config_path().display());
    Ok(())
}
fn execute(mut command: Command) -> Result<i32> {
    #[cfg(unix)]
    {
        use std::os::unix::process::CommandExt;
        Err(command.exec()).context("Cannot start command")
    }
    #[cfg(windows)]
    {
        Ok(windows_job::run(&mut command)?.code().unwrap_or(1))
    }
    #[cfg(not(any(unix, windows)))]
    {
        Ok(command.status()?.code().unwrap_or(1))
    }
}
// OpenSSH substitutes %h/%p before a shell parses ProxyCommand. Restrict the
// generated host pattern and also revalidate destination arguments in `proxy`.
fn proxy_command(runtime: &Runtime, host: String, port: u16) -> Result<i32> {
    ensure!(
        config::host(&host) || host.parse::<std::net::IpAddr>().is_ok(),
        "Invalid proxy hostname"
    );
    let config = runtime.load()?;
    #[cfg(target_os = "linux")]
    if config.backend == Backend::Native {
        return native::proxy(runtime, &host, port);
    }
    ensure!(
        config.allowed_targets == ["any"]
            || config.allowed_targets.contains(&format!("{host}:{port}")),
        "Destination is excluded by allowed_targets"
    );
    let id = runtime.container()?;
    let mut command = Command::new("docker");
    command.args([
        "exec",
        "-i",
        &id,
        "runuser",
        "-u",
        "jump",
        "--",
        "nc",
        "-N",
        "--",
        &host,
        &port.to_string(),
    ]);
    execute(command)
}

fn remote_proxy(options: &Proxy) -> Result<i32> {
    let server = options
        .remote
        .server
        .as_deref()
        .context("Missing remote server")?;
    let valid = if let Some((user, host)) = server.split_once('@') {
        config::ssh_user(user) && config::host(host)
    } else {
        config::host(server)
    };
    ensure!(valid, "Use an existing SSH host alias or user@hostname");
    ensure!(
        config::host(&options.host) || options.host.parse::<std::net::IpAddr>().is_ok(),
        "Invalid destination hostname"
    );
    let binary = match &options.remote.gateway_bin {
        Some(path) => shell_quote(path)?,
        None => String::from("\"$HOME/.local/bin/hku\""),
    };
    let state = match &options.remote.remote_state_dir {
        Some(path) => format!(" --state-dir {}", shell_quote(path)?),
        None => String::new(),
    };
    let remote_command = format!(
        "exec {binary}{state} proxy {} {}",
        shell_quote(&options.host)?,
        options.port
    );
    let mut command = Command::new("ssh");
    command.args(["-T", server, &remote_command]);
    execute(command)
}

fn local_quote(value: &str) -> Result<String> {
    ensure!(
        !value.contains(['\n', '\r', '\0', '%']),
        "Unsupported character in proxy argument"
    );
    #[cfg(windows)]
    {
        // Windows OpenSSH creates the proxy process directly, using Windows
        // command-line quoting rather than POSIX single-quote rules.
        let mut result = String::from("\"");
        let mut slashes = 0;
        for c in value.chars() {
            if c == '\\' {
                slashes += 1;
                continue;
            }
            if c == '"' {
                result.push_str(&"\\".repeat(slashes * 2 + 1));
            } else {
                result.push_str(&"\\".repeat(slashes));
            }
            slashes = 0;
            result.push(c);
        }
        result.push_str(&"\\".repeat(slashes * 2));
        result.push('"');
        Ok(result)
    }
    #[cfg(not(windows))]
    Ok(format!("'{}'", value.replace('\'', "'\\''")))
}

fn launch_ssh(mut options: Ssh, state_dir: Option<PathBuf>) -> Result<i32> {
    let mut proxy = local_quote(&std::env::current_exe()?.to_string_lossy())?;
    if options.remote.server.is_none() {
        ensure!(
            cfg!(target_os = "linux"),
            "Local VPN deployment requires Linux. Use hku ssh -s SERVER DESTINATION as a client"
        );
        let state = Runtime::new(match state_dir {
            Some(p) => expand_home(p)?,
            None => default_state()?,
        })?;
        state.load()?;
        proxy += &format!(
            " --state-dir {}",
            local_quote(&state.dir.to_string_lossy())?
        );
    }
    proxy += " proxy";
    for (name, value) in [
        ("--server", &options.remote.server),
        ("--gateway-bin", &options.remote.gateway_bin),
        ("--remote-state-dir", &options.remote.remote_state_dir),
    ] {
        if let Some(value) = value {
            proxy += &format!(" {name} {}", local_quote(value)?);
        }
    }
    proxy += " %h %p";
    if options.command.first().is_some_and(|v| v == "ssh") {
        options.command.remove(0);
    }
    if options.command.is_empty() {
        options
            .command
            .push(prompt("SSH destination (user@host)")?.into());
    }
    let mut command = Command::new("ssh");
    command
        .arg("-o")
        .arg(format!("ProxyCommand={proxy}"))
        .args(options.command);
    execute(command)
}

fn shell_quote(value: &str) -> Result<String> {
    // OpenSSH expands % tokens. Do not allow them in literal configuration paths.
    ensure!(
        !value.contains(['\n', '\r', '\0', '%', '"', '$', '`', '\\']),
        "Unsupported character in executable/state path"
    );
    Ok(format!("'{}'", value.replace('\'', "'\\''")))
}
fn generate_client(runtime: &Runtime, options: Client) -> Result<()> {
    let server = if let Some(v) = &options.server {
        let (user, host) = v.split_once('@').context("Use --server user@hostname")?;
        ensure!(
            config::ssh_user(user) && config::host(host) && options.server_port > 0,
            "Invalid SSH host or port"
        );
        Some((user, host))
    } else {
        None
    };
    if let Some(v) = &options.identity {
        config::client_identity(v)?;
    }
    if let Some(v) = &options.target {
        ensure!(config::host(v), "Invalid target hostname");
    }
    if let Some(v) = &options.target_user {
        ensure!(config::ssh_user(v), "Invalid target user");
    }
    let executable = options
        .gateway_bin
        .unwrap_or(std::env::current_exe()?.to_string_lossy().into_owned());
    let remote = format!(
        "{} --state-dir {} proxy %h %p",
        shell_quote(&executable)?,
        shell_quote(&runtime.dir.to_string_lossy())?
    );
    let mut text =
        String::from("# Generated by hku. Host and destination authentication use normal SSH.\n");
    if let Some((user, host)) = server {
        text += &format!("Host hku-gateway-host\n    HostName {host}\n    User {user}\n    Port {}\n    ForwardAgent no\n", options.server_port);
        if let Some(identity) = &options.identity {
            text += &format!("    IdentityFile \"{identity}\"\n");
        }
    }
    let proxy = if let Some((user, host)) = server {
        let identity = if let Some(path) = &options.identity {
            ensure!(
                !path.contains(['$', '`', '%']),
                "Unsupported character in identity path"
            );
            format!(" -i \"{path}\"")
        } else {
            String::new()
        };
        format!(
            "ssh -T -p {}{identity} {user}@{host} \"{remote}\"",
            options.server_port
        )
    } else {
        remote
    };
    // The generic alias takes its destination from -o HostName=DEST.
    if let (Some(target), Some(user)) = (options.target, options.target_user) {
        text += &format!("\nHost hku-target\n    HostName {target}\n    User {user}\n    ProxyCommand {proxy}\n    ForwardAgent no\n    ServerAliveInterval 30\n    ServerAliveCountMax 3\n");
    }
    text += &format!("\nHost hku-vpn\n    ProxyCommand {proxy}\n    ForwardAgent no\n");
    let output = options.output.unwrap_or(runtime.dir.join("client"));
    fs::create_dir_all(&output)?;
    private_write(&output.join("hku-ssh.config"), text.as_bytes())?;
    println!("Created {}", output.join("hku-ssh.config").display());
    println!("Use ssh -F hku-ssh.config hku-target, or -o HostName=DEST USER@hku-vpn.");
    Ok(())
}
fn run(cli: Cli) -> Result<i32> {
    #[cfg(target_os = "linux")]
    if let Action::NativeInternal(command) = cli.command {
        return native::internal(command);
    }
    // Remote client commands do not read deployment config or require Docker.
    if let Action::Ssh(options) = cli.command {
        return launch_ssh(options, cli.state_dir);
    }
    if let Action::Proxy(ref options) = cli.command {
        if options.remote.server.is_some() {
            return remote_proxy(options);
        }
    }
    if let Action::Doctor { backend } = cli.command {
        let selected = backend.or_else(|| {
            cli.state_dir
                .clone()
                .and_then(|p| Runtime::new(p).ok())
                .and_then(|r| r.load().ok())
                .map(|c| c.backend)
        });
        return Ok(if doctor(selected)? { 0 } else { 1 });
    }
    ensure!(
        cfg!(target_os = "linux"),
        "Gateway management is Linux-only. Use hku ssh -s SERVER DESTINATION in client mode"
    );
    let runtime = Runtime::new(match cli.state_dir {
        Some(p) => expand_home(p)?,
        None => default_state()?,
    })?;
    match cli.command {
        Action::Init(options) => {
            initialize(&runtime, options)?;
            return Ok(0);
        }
        Action::Setup(options) => {
            ensure!(
                cfg!(target_os = "linux"),
                "Gateway deployment is Linux-only; use remote SSH client mode"
            );
            let backend = options
                .backend
                .unwrap_or_else(|| runtime.load().map(|c| c.backend).unwrap_or(Backend::Native));
            ensure!(doctor(Some(backend))?, "Missing host prerequisites");
            initialize(&runtime, options)?;
            #[cfg(target_os = "linux")]
            if runtime.load()?.backend == Backend::Native {
                native::up(&runtime)?;
                println!("Gateway started. Run hku connect for password and MFA authentication.");
                return Ok(0);
            }
            success(runtime.compose().args([
                "up",
                "-d",
                "--build",
                "--wait",
                "--wait-timeout",
                "90",
            ]))?;
            println!("Gateway started. Run hku connect for password and MFA authentication.");
            return Ok(0);
        }
        Action::Config { path } => {
            if path {
                println!("{}", runtime.config_path().display());
            } else {
                print!(
                    "{}",
                    fs::read_to_string(runtime.config_path()).context("Run hku init first")?
                );
            }
            return Ok(0);
        }
        Action::Doctor { .. } => unreachable!(),
        _ => (),
    }
    ensure!(
        cfg!(target_os = "linux"),
        "Run gateway management on Linux; other systems can use normal SSH clients"
    );
    // Proxy mode must not rewrite runtime assets or print anything to stdout.
    if let Action::Proxy(options) = cli.command {
        return proxy_command(&runtime, options.host, options.port);
    }
    let config = runtime.sync()?;
    #[cfg(target_os = "linux")]
    if config.backend == Backend::Native {
        return match cli.command {
            Action::Up => {
                ensure!(
                    doctor(Some(Backend::Native))?,
                    "Missing native prerequisites"
                );
                native::up(&runtime)
            }
            Action::Connect => native::connect(&runtime),
            Action::Status => native::status(&runtime),
            Action::Disconnect => native::disconnect(&runtime),
            Action::Down => native::down(&runtime),
            Action::Logs => native::logs(&runtime),
            Action::ClientConfig(options) => {
                generate_client(&runtime, options)?;
                Ok(0)
            }
            _ => bail!("Unsupported native operation"),
        };
    }
    let _ = config;
    let status = match cli.command {
        Action::Up => { ensure!(doctor(Some(Backend::Docker))?, "Missing host prerequisites"); runtime.run(&["up", "-d", "--build", "--wait", "--wait-timeout", "90"])? }
        Action::Connect => return connection::connect(&runtime),
        Action::Status => { success(runtime.compose().arg("ps"))?; runtime.run(&["exec", "-T", "vpn", "/usr/local/lib/hku/status.sh"])? }
        Action::Disconnect => runtime.run(&["exec", "-T", "vpn", "sh", "-c", "if pgrep -x openconnect >/dev/null; then pkill -INT -x openconnect; else echo 'VPN is already offline.'; fi"] )?,
        Action::Down => runtime.run(&["down"] )?,
        Action::Logs => runtime.run(&["logs", "--tail", "80", "vpn"] )?,
        Action::ClientConfig(options) => { generate_client(&runtime, options)?; return Ok(0); }
        _ => bail!("Unsupported command"),
    };
    Ok(status.code().unwrap_or(1))
}
fn main() -> ExitCode {
    match run(Cli::parse()) {
        Ok(code) => ExitCode::from(code.clamp(0, 255) as u8),
        Err(error) => {
            eprintln!("Error: {error:#}");
            ExitCode::FAILURE
        }
    }
}
