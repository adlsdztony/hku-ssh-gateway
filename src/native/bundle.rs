use super::*;
use flate2::read::GzDecoder;
use sha2::{Digest, Sha256};
use std::collections::{BTreeMap, BTreeSet};
use std::ffi::OsString;
use std::fs;
use std::os::unix::fs::{MetadataExt, OpenOptionsExt, PermissionsExt};
use std::path::{Component, Path};

const PAYLOAD: &[u8] = include_bytes!(concat!(env!("OUT_DIR"), "/openconnect-runtime.tar.gz"));
const MAX_EXPANDED: u64 = 128 * 1024 * 1024;

#[derive(Deserialize)]
struct Asset {
    sha256: String,
    size: u64,
    mode: u32,
}

#[derive(Deserialize)]
struct Manifest {
    schema: u32,
    arch: String,
    openconnect_version: String,
    loader: String,
    files: BTreeMap<String, Asset>,
}

pub fn available() -> bool {
    !PAYLOAD.is_empty()
}

fn safe_path(path: &Path) -> bool {
    !path.as_os_str().is_empty()
        && path
            .components()
            .all(|component| matches!(component, Component::Normal(_)))
}

fn manifest() -> Result<(Manifest, Vec<u8>)> {
    ensure!(
        available(),
        "This source build has no embedded OpenConnect runtime"
    );
    let mut archive = tar::Archive::new(GzDecoder::new(PAYLOAD));
    let mut entries = archive.entries()?;
    let mut first = entries.next().context("Empty runtime archive")??;
    ensure!(
        first.header().entry_type().is_file()
            && first.path()?.as_ref() == Path::new("manifest.json")
            && first.size() <= 2 * 1024 * 1024,
        "Runtime manifest must be the first regular file"
    );
    let mut bytes = Vec::new();
    first.read_to_end(&mut bytes)?;
    let manifest: Manifest = serde_json::from_slice(&bytes)?;
    ensure!(
        manifest.schema == 1 && manifest.arch == std::env::consts::ARCH,
        "Bundled runtime architecture/schema mismatch"
    );
    let mut total = 0u64;
    for (path, asset) in &manifest.files {
        ensure!(
            safe_path(Path::new(path)) && path != "manifest.json",
            "Unsafe runtime asset path"
        );
        ensure!(
            asset.sha256.len() == 64
                && asset.sha256.bytes().all(|b| b.is_ascii_hexdigit())
                && matches!(asset.mode, 0o644 | 0o755),
            "Invalid runtime asset metadata"
        );
        total = total
            .checked_add(asset.size)
            .context("Runtime size overflow")?;
        ensure!(total <= MAX_EXPANDED, "Runtime is too large");
    }
    for path in [
        "bin/openconnect",
        "vpnc-script",
        "ca-certificates.crt",
        manifest.loader.as_str(),
    ] {
        ensure!(
            manifest.files.get(path).is_some_and(|asset| asset.size > 0),
            "Required runtime asset missing: {path}"
        );
    }
    ensure!(
        safe_path(Path::new(&manifest.loader)) && manifest.loader.starts_with("lib/ld-musl-"),
        "Invalid bundled loader path"
    );
    Ok((manifest, bytes))
}

fn directory(d: &Deployment) -> PathBuf {
    d.directory()
        .join("runtime")
        .join(format!("{:x}", Sha256::digest(PAYLOAD)))
}

fn validate_installed(root: &Path, manifest: &Manifest) -> Result<()> {
    admin::secure_directory(root, 0o700)?;
    for (name, asset) in &manifest.files {
        let path = root.join(name);
        let metadata = fs::symlink_metadata(&path)?;
        ensure!(
            metadata.is_file()
                && !metadata.file_type().is_symlink()
                && metadata.uid() == 0
                && metadata.mode() & 0o777 == asset.mode
                && metadata.len() == asset.size,
            "Unsafe or damaged runtime asset: {name}"
        );
        ensure!(
            format!("{:x}", Sha256::digest(fs::read(&path)?)) == asset.sha256,
            "Runtime checksum mismatch: {name}"
        );
    }
    Ok(())
}

pub fn install(d: &Deployment) -> Result<()> {
    if !available() {
        admin::openconnect()?;
        admin::vpnc_script()?;
        return Ok(());
    }
    let (manifest, metadata) = manifest()?;
    let base = d.directory().join("runtime");
    admin::secure_directory(&base, 0o700)?;
    let destination = directory(d);
    if destination.exists() {
        return validate_installed(&destination, &manifest);
    }
    let staging = tempfile::Builder::new()
        .prefix(".unpack-")
        .tempdir_in(&base)?;
    let mut archive = tar::Archive::new(GzDecoder::new(PAYLOAD));
    let mut seen = BTreeSet::new();
    for (index, entry) in archive.entries()?.enumerate() {
        let mut entry = entry?;
        if index == 0 {
            continue;
        }
        let path = entry.path()?.into_owned();
        ensure!(
            entry.header().entry_type().is_file() && safe_path(&path),
            "Runtime archive contains a link or unsafe path"
        );
        let name = path.to_str().context("Non-UTF8 runtime path")?;
        let asset = manifest
            .files
            .get(name)
            .context("Unexpected runtime asset")?;
        ensure!(
            seen.insert(name.to_string()) && entry.size() == asset.size,
            "Duplicate or damaged runtime asset"
        );
        let destination = staging.path().join(&path);
        fs::create_dir_all(destination.parent().unwrap())?;
        let mut output = fs::OpenOptions::new()
            .write(true)
            .create_new(true)
            .mode(0o600)
            .open(&destination)?;
        let mut hash = Sha256::new();
        let mut buffer = [0u8; 16384];
        let mut count = 0u64;
        loop {
            let size = entry.read(&mut buffer)?;
            if size == 0 {
                break;
            }
            count += size as u64;
            ensure!(count <= asset.size, "Runtime asset exceeds declared size");
            hash.update(&buffer[..size]);
            output.write_all(&buffer[..size])?;
        }
        ensure!(
            count == asset.size && format!("{:x}", hash.finalize()) == asset.sha256,
            "Runtime checksum mismatch: {name}"
        );
        output.sync_all()?;
        drop(output);
        fs::set_permissions(destination, fs::Permissions::from_mode(asset.mode))?;
    }
    ensure!(
        seen.len() == manifest.files.len(),
        "Incomplete runtime archive"
    );
    crate::runtime::private_write(&staging.path().join("manifest.json"), &metadata)?;
    validate_installed(staging.path(), &manifest)?;
    fs::rename(staging.path(), &destination)?;
    println!(
        "Installed embedded {} ({}).",
        manifest.openconnect_version, manifest.arch
    );
    Ok(())
}

pub fn openconnect(d: &Deployment) -> Result<(PathBuf, Vec<OsString>)> {
    if !available() {
        return Ok((admin::openconnect()?, Vec::new()));
    }
    let (manifest, _) = manifest()?;
    let root = directory(d);
    let program = root.join(manifest.loader);
    admin::trusted_program(&[program.to_str().context("Invalid runtime path")?])?;
    let cafile = admin::trusted_program(&[
        "/etc/ssl/certs/ca-certificates.crt",
        "/etc/pki/tls/certs/ca-bundle.crt",
        "/etc/ssl/cert.pem",
    ])
    .unwrap_or_else(|_| root.join("ca-certificates.crt"));
    Ok((
        program,
        vec![
            "--library-path".into(),
            root.join("lib").into_os_string(),
            root.join("bin/openconnect").into_os_string(),
            "--no-system-trust".into(),
            format!("--cafile={}", cafile.display()).into(),
        ],
    ))
}

pub fn vpnc_script(d: &Deployment) -> Result<PathBuf> {
    if !available() {
        return admin::vpnc_script();
    }
    let path = directory(d).join("vpnc-script");
    admin::trusted_program(&[path.to_str().context("Invalid runtime path")?])
}

pub fn doctor() -> Result<()> {
    let (manifest, _) = manifest()?;
    println!(
        "BUNDLED {} ({}, {} assets, {} compressed bytes)",
        manifest.openconnect_version,
        manifest.arch,
        manifest.files.len(),
        PAYLOAD.len()
    );
    println!("BUNDLED vpnc-script, musl loader, shared libraries and fallback CA certificates");
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn unsafe_runtime_paths_are_rejected() {
        for path in ["/etc/shadow", "../escape", "lib/../../escape", ""] {
            assert!(!safe_path(Path::new(path)));
        }
        assert!(safe_path(Path::new("lib/libgnutls.so.30")));
    }
}
