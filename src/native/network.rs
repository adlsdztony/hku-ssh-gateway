use super::*;
use crate::runtime::{capture, success};
use std::fs;
use std::net::{IpAddr, Ipv4Addr};
use std::os::unix::fs::PermissionsExt;
use std::os::unix::process::CommandExt;
use std::path::PathBuf;
use std::process::{Command, Stdio};

#[derive(Debug)]
pub struct Network {
    pub namespace: String,
    pub veth: String,
    pub chain: String,
    pub cidr: String,
    pub host: Ipv4Addr,
    pub guest: Ipv4Addr,
    owner: u32,
    id: String,
}
fn cidr(text: &str) -> Result<(u32, u32)> {
    let (ip, prefix) = text.split_once('/').context("Expected an IPv4 CIDR")?;
    let ip = u32::from(ip.parse::<Ipv4Addr>()?);
    let prefix: u32 = prefix.parse()?;
    ensure!(prefix <= 32, "Invalid IPv4 prefix");
    let mask = if prefix == 0 {
        0
    } else {
        u32::MAX << (32 - prefix)
    };
    Ok((ip & mask, mask))
}
impl Network {
    pub fn new(d: &Deployment) -> Result<Self> {
        let seed = u64::from_str_radix(&d.id, 16)?;
        let subnet = d
            .config
            .native
            .subnet
            .clone()
            .unwrap_or_else(|| format!("10.253.{}.{}/30", (seed >> 8) & 255, seed & 252));
        let (base, mask) = cidr(&subnet)?;
        ensure!(
            mask == 0xfffffffc && Ipv4Addr::from(base).is_private(),
            "native.subnet must be a private IPv4 /30 network"
        );
        ensure!(
            subnet == format!("{}/30", Ipv4Addr::from(base)),
            "native.subnet must use the network address"
        );
        Ok(Self {
            namespace: d.namespace(),
            veth: format!("hk{}", d.id),
            chain: format!("HKU_{}", d.id),
            cidr: subnet,
            host: Ipv4Addr::from(base + 1),
            guest: Ipv4Addr::from(base + 2),
            owner: d.owner,
            id: d.id.clone(),
        })
    }
    fn directory(&self) -> PathBuf {
        PathBuf::from("/etc/netns").join(&self.namespace)
    }
    fn marker(&self) -> PathBuf {
        self.directory().join("hku-owner")
    }
    fn owned(&self) -> bool {
        fs::read_to_string(self.marker())
            .is_ok_and(|text| text == format!("{}:{}", self.id, self.owner))
    }
    pub fn command(&self, program: impl AsRef<std::ffi::OsStr>) -> Command {
        let mut command = Command::new("ip");
        command
            .args(["netns", "exec", &self.namespace])
            .arg(program);
        command
    }
    pub fn create(&self, d: &Deployment) -> Result<()> {
        if PathBuf::from("/run/netns").join(&self.namespace).exists() {
            ensure!(
                self.owned(),
                "Refusing to replace an unowned network namespace"
            );
            self.destroy()?;
        }
        let routes: serde_json::Value = serde_json::from_str(&capture(
            Command::new("ip").args(["-j", "-4", "route", "show", "table", "all"]),
        )?)?;
        let (base, mask) = cidr(&self.cidr)?;
        for route in routes.as_array().context("Invalid ip route output")? {
            if let Some(dest) = route["dst"].as_str() {
                if dest == "default" {
                    continue;
                }
                let normalized = if dest.contains('/') {
                    dest.to_string()
                } else {
                    format!("{dest}/32")
                };
                if let Ok((other, other_mask)) = cidr(&normalized) {
                    ensure!((base & other_mask) != other && (other & mask) != base, "Native subnet overlaps host route {dest}; set native.subnet to a free private /30");
                }
            }
        }
        let dns = bootstrap_dns(d)?;
        fs::create_dir_all(self.directory())?;
        fs::set_permissions(self.directory(), fs::Permissions::from_mode(0o755))?;
        fs::write(self.marker(), format!("{}:{}", self.id, self.owner))?;
        fs::write(self.directory().join("resolv.conf"), &dns)?;
        fs::set_permissions(
            self.directory().join("resolv.conf"),
            fs::Permissions::from_mode(0o644),
        )?;
        // Avoid nss-resolve reaching the host resolver through a Unix socket.
        let nss = fs::read_to_string("/etc/nsswitch.conf")?;
        let mut had_hosts = false;
        let mut nss = nss
            .lines()
            .map(|line| {
                if line.trim_start().starts_with("hosts:") {
                    had_hosts = true;
                    "hosts: files dns"
                } else {
                    line
                }
            })
            .collect::<Vec<_>>()
            .join("\n");
        if !had_hosts {
            nss.push_str("\nhosts: files dns");
        }
        nss.push('\n');
        fs::write(self.directory().join("nsswitch.conf"), nss)?;
        fs::set_permissions(
            self.directory().join("nsswitch.conf"),
            fs::Permissions::from_mode(0o644),
        )?;
        fs::write(d.directory().join("bootstrap-resolv.conf"), &dns)?;
        success(Command::new("ip").args(["netns", "add", &self.namespace]))?;
        let result = self.configure();
        if result.is_err() {
            let _ = self.destroy();
        }
        result
    }
    fn configure(&self) -> Result<()> {
        success(Command::new("ip").args([
            "link",
            "add",
            &self.veth,
            "type",
            "veth",
            "peer",
            "name",
            "eth0",
            "netns",
            &self.namespace,
        ]))?;
        success(Command::new("ip").args([
            "address",
            "add",
            &format!("{}/30", self.host),
            "dev",
            &self.veth,
        ]))?;
        success(Command::new("ip").args(["link", "set", &self.veth, "up"]))?;
        for args in [
            vec!["link".into(), "set".into(), "lo".into(), "up".into()],
            vec!["link".into(), "set".into(), "eth0".into(), "up".into()],
            vec![
                "address".into(),
                "add".into(),
                format!("{}/30", self.guest),
                "dev".into(),
                "eth0".into(),
            ],
            vec![
                "route".into(),
                "add".into(),
                "default".into(),
                "via".into(),
                self.host.to_string(),
            ],
        ] {
            success(self.command("ip").args(args))?;
        }
        for firewall in ["iptables", "ip6tables"] {
            success(self.command(firewall).args(["-N", "HKU_VPN_ONLY"]))?;
            for interface in ["lo", "tun0"] {
                success(self.command(firewall).args([
                    "-A",
                    "HKU_VPN_ONLY",
                    "-o",
                    interface,
                    "-j",
                    "ACCEPT",
                ]))?;
            }
            success(
                self.command(firewall)
                    .args(["-A", "HKU_VPN_ONLY", "-j", "REJECT"]),
            )?;
            success(self.command(firewall).args([
                "-I",
                "OUTPUT",
                "1",
                "-m",
                "owner",
                "--uid-owner",
                &self.owner.to_string(),
                "-j",
                "HKU_VPN_ONLY",
            ]))?;
        }
        success(Command::new("iptables").args(["-w", "-N", &self.chain]))?;
        success(Command::new("iptables").args([
            "-w",
            "-A",
            &self.chain,
            "-i",
            &self.veth,
            "-s",
            &self.guest.to_string(),
            "-j",
            "ACCEPT",
        ]))?;
        success(Command::new("iptables").args([
            "-w",
            "-A",
            &self.chain,
            "-o",
            &self.veth,
            "-d",
            &self.guest.to_string(),
            "-m",
            "conntrack",
            "--ctstate",
            "ESTABLISHED,RELATED",
            "-j",
            "ACCEPT",
        ]))?;
        success(Command::new("iptables").args(["-w", "-I", "FORWARD", "1", "-j", &self.chain]))?;
        success(Command::new("iptables").args([
            "-w",
            "-t",
            "nat",
            "-A",
            "POSTROUTING",
            "-s",
            &self.cidr,
            "!",
            "-o",
            &self.veth,
            "-m",
            "comment",
            "--comment",
            &self.chain,
            "-j",
            "MASQUERADE",
        ]))?;
        if fs::read_to_string("/proc/sys/net/ipv4/ip_forward")?.trim() != "1" {
            fs::write("/proc/sys/net/ipv4/ip_forward", "1\n")?;
            eprintln!(
                "Enabled host IPv4 forwarding; shutdown leaves this shared host setting enabled."
            );
        }
        Ok(())
    }
    pub fn destroy(&self) -> Result<()> {
        if !self.owned() {
            return Ok(());
        }
        if let Ok(pids) = admin::root_processes(&self.namespace) {
            for pid in pids {
                let _ = nix::sys::signal::kill(
                    nix::unistd::Pid::from_raw(pid),
                    nix::sys::signal::Signal::SIGTERM,
                );
            }
            std::thread::sleep(std::time::Duration::from_millis(100));
            if let Ok(pids) = admin::root_processes(&self.namespace) {
                for pid in pids {
                    let _ = nix::sys::signal::kill(
                        nix::unistd::Pid::from_raw(pid),
                        nix::sys::signal::Signal::SIGKILL,
                    );
                }
            }
        }
        for args in [
            vec!["-w", "-D", "FORWARD", "-j", &self.chain],
            vec!["-w", "-F", &self.chain],
            vec!["-w", "-X", &self.chain],
        ] {
            let _ = Command::new("iptables")
                .args(args)
                .stdout(Stdio::null())
                .stderr(Stdio::null())
                .status();
        }
        let _ = Command::new("iptables")
            .args([
                "-w",
                "-t",
                "nat",
                "-D",
                "POSTROUTING",
                "-s",
                &self.cidr,
                "!",
                "-o",
                &self.veth,
                "-m",
                "comment",
                "--comment",
                &self.chain,
                "-j",
                "MASQUERADE",
            ])
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .status();
        let _ = Command::new("ip")
            .args(["link", "delete", &self.veth])
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .status();
        let _ = Command::new("ip")
            .args(["netns", "delete", &self.namespace])
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .status();
        for name in ["resolv.conf", "nsswitch.conf", "hku-owner"] {
            let _ = fs::remove_file(self.directory().join(name));
        }
        let _ = fs::remove_dir(self.directory());
        Ok(())
    }
    pub fn ready(&self) -> bool {
        capture(
            self.command("ip")
                .args(["-o", "-4", "address", "show", "dev", "tun0"]),
        )
        .is_ok_and(|s| s.contains(" inet "))
            && capture(self.command("ip").args(["-4", "route", "show", "default"]))
                .is_ok_and(|s| s.lines().any(|l| l.starts_with("default dev tun0 ")))
    }
}

fn bootstrap_dns(d: &Deployment) -> Result<String> {
    let mut addresses: Vec<IpAddr> = d.config.native.bootstrap_dns.clone();
    if addresses.is_empty() {
        for file in ["/run/systemd/resolve/resolv.conf", "/etc/resolv.conf"] {
            if let Ok(text) = fs::read_to_string(file) {
                for line in text.lines() {
                    let mut fields = line.split_whitespace();
                    if fields.next() == Some("nameserver") {
                        if let Some(Ok(ip)) = fields.next().map(str::parse::<IpAddr>) {
                            if ip.is_ipv4() && !ip.is_loopback() && !ip.is_unspecified() {
                                addresses.push(ip);
                            }
                        }
                    }
                }
                if !addresses.is_empty() {
                    break;
                }
            }
        }
    }
    ensure!(
        !addresses.is_empty()
            && addresses
                .iter()
                .all(|ip| ip.is_ipv4() && !ip.is_loopback() && !ip.is_unspecified()),
        "Set native.bootstrap_dns to reachable, non-loopback IPv4 DNS servers"
    );
    Ok(addresses
        .iter()
        .map(|ip| format!("nameserver {ip}\n"))
        .collect::<String>()
        + "options timeout:1 attempts:1\n")
}

pub fn hook(d: &Deployment) -> Result<i32> {
    let reason = std::env::var("reason").unwrap_or_default();
    let dns: Vec<IpAddr> = ["INTERNAL_IP4_DNS", "INTERNAL_IP6_DNS"]
        .iter()
        .filter_map(|name| std::env::var(name).ok())
        .flat_map(|s| {
            s.split_whitespace()
                .filter_map(|ip| ip.parse().ok())
                .collect::<Vec<_>>()
        })
        .collect();
    let text = if matches!(reason.as_str(), "connect" | "reconnect") && !dns.is_empty() {
        Some(
            dns.iter()
                .map(|ip| format!("nameserver {ip}\n"))
                .collect::<String>()
                + "options timeout:1 attempts:1\n",
        )
    } else if reason == "disconnect" {
        Some(fs::read_to_string(
            d.directory().join("bootstrap-resolv.conf"),
        )?)
    } else {
        None
    };
    if let Some(text) = text {
        fs::write(
            PathBuf::from("/etc/netns")
                .join(d.namespace())
                .join("resolv.conf"),
            text,
        )?;
    }
    // Suppress host resolver integration and preserve OpenConnect's parent PID:
    // vpnc-script uses PPID to pair route backups across connect/disconnect.
    Err(Command::new(admin::vpnc_script()?)
        .env_remove("INTERNAL_IP4_DNS")
        .env_remove("INTERNAL_IP6_DNS")
        .exec())
    .context("Cannot execute vpnc-script")
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn native_network_names_and_ranges_are_scoped() {
        let mut d = Deployment {
            id: "123456abcdef".into(),
            owner: 1001,
            group: 1001,
            config: Config::default(),
        };
        let network = Network::new(&d).unwrap();
        assert!(network.veth.len() <= 15);
        assert!(network.host.is_private());
        d.config.native.subnet = Some("8.8.8.0/30".into());
        assert!(Network::new(&d).is_err());
        d.config.native.subnet = Some("10.0.0.1/30".into());
        assert!(Network::new(&d).is_err());
    }
}
