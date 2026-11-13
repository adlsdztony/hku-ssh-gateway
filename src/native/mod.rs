mod admin;
mod client;
mod network;
mod server;

use anyhow::{ensure, Context, Result};
use serde::{Deserialize, Serialize};
use std::io::{Read, Write};
use std::path::PathBuf;

use crate::config::{Backend, Config};
use crate::runtime::Runtime;

pub use client::{connect, disconnect, down, logs, proxy, status, up};

const ROOT: &str = "/var/lib/hku/native";
const RUN: &str = "/run/hku-native";
const MAX_FRAME: usize = 65536;

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Deployment {
    id: String,
    owner: u32,
    group: u32,
    config: Config,
}

impl Deployment {
    fn validate(&self) -> Result<()> {
        ensure!(
            self.id.len() == 12 && self.id.bytes().all(|b| b.is_ascii_hexdigit()),
            "Invalid native instance ID"
        );
        ensure!(self.owner > 0, "Native deployments require a non-root owner; invoke hku normally and let it request sudo");
        ensure!(
            self.group > 0,
            "Native deployments require a non-root primary group"
        );
        ensure!(
            self.config.backend == Backend::Native,
            "Not a native configuration"
        );
        self.config.validate()?;
        network::Network::new(self)?;
        Ok(())
    }
    fn directory(&self) -> PathBuf {
        PathBuf::from(ROOT).join(&self.id)
    }
    fn binary(&self) -> PathBuf {
        self.directory().join("hku")
    }
    fn namespace(&self) -> String {
        format!("hku-{}", self.id)
    }
    fn unit(&self) -> String {
        format!("hku-native-{}.service", self.id)
    }
    fn socket(&self) -> PathBuf {
        PathBuf::from(RUN).join(format!("{}.sock", self.id))
    }
}

fn identity(runtime: &Runtime) -> Result<String> {
    let path = runtime
        .dir
        .canonicalize()
        .context("Initialize the deployment first")?;
    let mut hash = 0xcbf29ce484222325u64;
    for b in path.as_os_str().as_encoded_bytes() {
        hash ^= u64::from(*b);
        hash = hash.wrapping_mul(0x100000001b3);
    }
    for b in nix::unistd::getuid().as_raw().to_le_bytes() {
        hash ^= u64::from(b);
        hash = hash.wrapping_mul(0x100000001b3);
    }
    Ok(format!("{:012x}", hash & 0xffffffffffff))
}

fn deployment(runtime: &Runtime) -> Result<Deployment> {
    let owner = nix::unistd::getuid().as_raw();
    let result = Deployment {
        id: identity(runtime)?,
        owner,
        group: nix::unistd::getgid().as_raw(),
        config: runtime.load()?,
    };
    result.validate()?;
    Ok(result)
}

#[derive(Debug, Serialize, Deserialize)]
#[serde(tag = "op", rename_all = "kebab-case")]
enum Request {
    Status,
    Connect,
    Disconnect,
    Logs,
    Proxy { host: String, port: u16 },
}

#[derive(Debug, Serialize, Deserialize)]
struct Reply {
    ok: bool,
    message: String,
    #[serde(default)]
    ready: bool,
}

fn send_json(stream: &mut impl Write, value: &impl Serialize) -> Result<()> {
    serde_json::to_writer(&mut *stream, value)?;
    stream.write_all(b"\n")?;
    stream.flush()?;
    Ok(())
}
fn read_json<T: serde::de::DeserializeOwned>(stream: &mut impl Read) -> Result<T> {
    let mut data = Vec::new();
    loop {
        let mut byte = [0];
        stream.read_exact(&mut byte)?;
        if byte[0] == b'\n' {
            break;
        }
        ensure!(data.len() < MAX_FRAME, "Control message too large");
        data.push(byte[0]);
    }
    Ok(serde_json::from_slice(&data)?)
}
fn send_frame(stream: &mut impl Write, kind: u8, payload: &[u8]) -> Result<()> {
    ensure!(payload.len() <= MAX_FRAME, "Terminal frame too large");
    stream.write_all(&[kind])?;
    stream.write_all(&(payload.len() as u32).to_be_bytes())?;
    stream.write_all(payload)?;
    stream.flush()?;
    Ok(())
}
fn read_frame(stream: &mut impl Read) -> Result<(u8, Vec<u8>)> {
    let mut header = [0u8; 5];
    stream.read_exact(&mut header)?;
    let size = u32::from_be_bytes(header[1..5].try_into()?) as usize;
    ensure!(size <= MAX_FRAME, "Terminal frame too large");
    let mut bytes = vec![0; size];
    stream.read_exact(&mut bytes)?;
    Ok((header[0], bytes))
}

#[derive(clap::Subcommand)]
pub enum Internal {
    Install,
    Remove {
        #[arg(long)]
        id: String,
    },
    Daemon {
        #[arg(long)]
        id: String,
    },
    Hook {
        #[arg(long)]
        id: String,
    },
    Stream {
        #[arg(long)]
        owner: u32,
        #[arg(long)]
        group: u32,
        #[arg(long)]
        host: String,
        #[arg(long)]
        port: u16,
    },
    Probe {
        #[arg(long)]
        owner: u32,
        #[arg(long)]
        group: u32,
        #[arg(long)]
        host: String,
        #[arg(long)]
        port: u16,
        #[arg(long)]
        timeout: u32,
    },
}

pub fn internal(command: Internal) -> Result<i32> {
    ensure!(
        nix::unistd::geteuid().is_root(),
        "This internal operation requires root"
    );
    match command {
        Internal::Install => admin::install(),
        Internal::Remove { id } => admin::remove(&id),
        Internal::Daemon { id } => server::run(admin::load(&id)?),
        Internal::Hook { id } => network::hook(&admin::load(&id)?),
        Internal::Stream {
            owner,
            group,
            host,
            port,
        } => server::stream_child(owner, group, &host, port),
        Internal::Probe {
            owner,
            group,
            host,
            port,
            timeout,
        } => server::probe_child(owner, group, &host, port, timeout),
    }
}

pub fn doctor() -> Result<bool> {
    use std::process::{Command, Stdio};
    let mut ok = true;
    for (program, args) in [
        ("ip", vec!["-Version"]),
        ("iptables", vec!["--version"]),
        ("ip6tables", vec!["--version"]),
        ("openconnect", vec!["--version"]),
        ("systemctl", vec!["--version"]),
        ("sudo", vec!["--version"]),
    ] {
        let found = Command::new(program)
            .args(args)
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .status()
            .is_ok_and(|s| s.success());
        println!("{} {program}", if found { "OK" } else { "MISSING" });
        ok &= found;
    }
    let script_result = admin::vpnc_script();
    let script = script_result.is_ok();
    println!("{} vpnc-script", if script { "OK" } else { "MISSING" });
    if let Err(error) = script_result {
        println!("{error:#}");
    }
    ok &= script;
    if let Err(error) = admin::openconnect() {
        println!("UNSAFE openconnect: {error:#}");
        ok = false;
    }
    use std::os::unix::fs::FileTypeExt;
    let tun = std::fs::metadata("/dev/net/tun").is_ok_and(|m| m.file_type().is_char_device());
    println!("{} /dev/net/tun", if tun { "OK" } else { "MISSING" });
    ok &= tun;
    let systemd = std::path::Path::new("/run/systemd/system").is_dir();
    println!(
        "{} systemd system manager",
        if systemd { "OK" } else { "MISSING" }
    );
    Ok(ok && systemd)
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn control_protocol_rejects_oversized_terminal_frames() {
        let bytes = [b'I', 0xff, 0xff, 0xff, 0xff];
        assert!(read_frame(&mut bytes.as_slice()).is_err());
        let mut output = Vec::new();
        send_frame(&mut output, b'O', b"terminal bytes").unwrap();
        let (kind, payload) = read_frame(&mut output.as_slice()).unwrap();
        assert_eq!(kind, b'O');
        assert_eq!(payload, b"terminal bytes");
    }
}
