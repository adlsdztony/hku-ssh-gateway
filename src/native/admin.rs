use super::*;
use crate::runtime::{capture, private_write, success};
use std::fs;
use std::os::unix::fs::{MetadataExt, PermissionsExt};
use std::path::Path;
use std::process::Command;

pub(super) fn secure_directory(path: &Path, mode: u32) -> Result<()> {
    if !path.exists() {
        fs::create_dir(path)?;
        fs::set_permissions(path, fs::Permissions::from_mode(mode))?;
    }
    let metadata = fs::symlink_metadata(path)?;
    ensure!(
        metadata.is_dir()
            && !metadata.file_type().is_symlink()
            && metadata.uid() == 0
            && metadata.mode() & 0o022 == 0,
        "Native administrator directory is not root-controlled: {}",
        path.display()
    );
    Ok(())
}

pub fn prepare_runtime() -> Result<()> {
    secure_directory(Path::new(RUN), 0o755)
}

pub fn trusted_program(candidates: &[&str]) -> Result<PathBuf> {
    let mut reasons = Vec::new();
    for path in candidates {
        if let Ok(canonical) = Path::new(path).canonicalize() {
            let metadata = fs::metadata(&canonical)?;
            let parents_safe = canonical.ancestors().skip(1).all(|parent| {
                fs::metadata(parent).is_ok_and(|m| m.uid() == 0 && m.mode() & 0o022 == 0)
            });
            if metadata.is_file()
                && metadata.uid() == 0
                && metadata.mode() & 0o022 == 0
                && parents_safe
            {
                return Ok(canonical);
            }
            reasons.push(format!(
                "{} uid={} mode={:o} parents_safe={parents_safe}",
                canonical.display(),
                metadata.uid(),
                metadata.mode()
            ));
        }
    }
    anyhow::bail!(
        "Install a root-owned program at one of: {} ({})",
        candidates.join(", "),
        reasons.join("; ")
    )
}
pub fn vpnc_script() -> Result<PathBuf> {
    trusted_program(&[
        "/usr/share/vpnc-scripts/vpnc-script",
        "/etc/vpnc/vpnc-script",
    ])
}
pub fn openconnect() -> Result<PathBuf> {
    trusted_program(&[
        "/usr/local/sbin/openconnect",
        "/usr/sbin/openconnect",
        "/usr/bin/openconnect",
    ])
}

pub fn load(id: &str) -> Result<Deployment> {
    ensure!(
        id.len() == 12 && id.bytes().all(|b| b.is_ascii_hexdigit()),
        "Invalid instance ID"
    );
    let path = PathBuf::from(ROOT).join(id).join("deployment.json");
    let meta = fs::symlink_metadata(&path)?;
    ensure!(
        meta.uid() == 0 && meta.mode() & 0o022 == 0 && !meta.file_type().is_symlink(),
        "Unsafe native configuration ownership"
    );
    let d: Deployment = serde_json::from_slice(&fs::read(path)?)?;
    d.validate()?;
    ensure!(d.id == id, "Native deployment identity mismatch");
    Ok(d)
}

pub fn install() -> Result<i32> {
    let mut data = Vec::new();
    std::io::stdin()
        .take(MAX_FRAME as u64 + 1)
        .read_to_end(&mut data)?;
    ensure!(data.len() <= MAX_FRAME, "Configuration too large");
    let d: Deployment = serde_json::from_slice(&data)?;
    d.validate()?;
    if let Ok(uid) = std::env::var("SUDO_UID") {
        ensure!(
            uid.parse::<u32>()? == d.owner,
            "Installer owner differs from sudo caller"
        );
    }
    for (path, mode) in [("/var/lib/hku", 0o755), (ROOT, 0o755), (RUN, 0o755)] {
        secure_directory(Path::new(path), mode)?;
    }
    secure_directory(&d.directory(), 0o700)?;
    bundle::install(&d)?;
    if d.directory().join("deployment.json").exists() {
        ensure!(
            load(&d.id)?.owner == d.owner,
            "Another user owns this native deployment"
        );
        let _ = Command::new("systemctl").args(["stop", &d.unit()]).status();
    }
    let temporary = d
        .directory()
        .join(format!("hku-install-{}", std::process::id()));
    let mut output = fs::OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(&temporary)?;
    std::io::copy(&mut fs::File::open(std::env::current_exe()?)?, &mut output)?;
    fs::set_permissions(&temporary, fs::Permissions::from_mode(0o755))?;
    output.sync_all()?;
    // Linux refuses exec while any writable descriptor for this inode is open.
    drop(output);
    fs::rename(&temporary, d.binary())?;
    private_write(
        &d.directory().join("deployment.json"),
        &serde_json::to_vec_pretty(&d)?,
    )?;
    let unit = format!("[Unit]\nDescription=HKU native VPN gateway {}\nAfter=network-online.target\nWants=network-online.target\n\n[Service]\nType=exec\nExecStart={} native-internal daemon --id {}\nRestart=on-failure\nRestartSec=3\nKillMode=mixed\nTimeoutStopSec=20\nUMask=0077\n\n[Install]\nWantedBy=multi-user.target\n", d.id, d.binary().display(), d.id);
    let unit_path = PathBuf::from("/etc/systemd/system").join(d.unit());
    private_write(&unit_path, unit.as_bytes())?;
    fs::set_permissions(&unit_path, fs::Permissions::from_mode(0o644))?;
    success(Command::new("systemctl").arg("daemon-reload"))?;
    success(Command::new("systemctl").args(["enable", "--now", &d.unit()]))?;
    println!("Native service installed: {}", d.unit());
    Ok(0)
}

pub fn remove(id: &str) -> Result<i32> {
    let d = load(id)?;
    if let Ok(uid) = std::env::var("SUDO_UID") {
        ensure!(
            uid.parse::<u32>()? == d.owner,
            "Deployment belongs to another user"
        );
    }
    let unit_path = PathBuf::from("/etc/systemd/system").join(d.unit());
    if unit_path.exists() {
        success(Command::new("systemctl").args(["disable", "--now", &d.unit()]))?;
        fs::remove_file(unit_path)?;
    }
    network::Network::new(&d)?.destroy()?;
    success(Command::new("systemctl").arg("daemon-reload"))?;
    println!(
        "Native gateway stopped; user configuration and root-owned installation are preserved."
    );
    Ok(0)
}

pub fn root_processes(namespace: &str) -> Result<Vec<i32>> {
    let text = capture(Command::new("ip").args(["netns", "pids", namespace]))?;
    text.split_whitespace()
        .map(|p| p.parse().map_err(Into::into))
        .collect()
}
