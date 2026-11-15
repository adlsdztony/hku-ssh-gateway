use anyhow::{ensure, Context, Result};
use std::fs;
use std::path::{Path, PathBuf};
use std::process::{Command, ExitStatus};

use crate::config::{Backend, Config};

const ASSETS: &[(&str, &str)] = &[
    ("Dockerfile", include_str!("../Dockerfile")),
    ("compose.yaml", include_str!("../compose.yaml")),
    (".dockerignore", include_str!("../.dockerignore")),
    (
        "container/common.sh",
        include_str!("../container/common.sh"),
    ),
    (
        "container/entrypoint.sh",
        include_str!("../container/entrypoint.sh"),
    ),
    (
        "container/console.sh",
        include_str!("../container/console.sh"),
    ),
    (
        "container/connection-state.sh",
        include_str!("../container/connection-state.sh"),
    ),
    (
        "container/healthcheck.sh",
        include_str!("../container/healthcheck.sh"),
    ),
    (
        "container/keepalive.sh",
        include_str!("../container/keepalive.sh"),
    ),
    (
        "container/status.sh",
        include_str!("../container/status.sh"),
    ),
];

pub struct Runtime {
    pub dir: PathBuf,
}

pub fn private_write(path: &Path, data: &[u8]) -> Result<()> {
    use std::io::Write;
    let mut options = fs::OpenOptions::new();
    options.write(true).create(true).truncate(true);
    #[cfg(unix)]
    {
        use std::os::unix::fs::OpenOptionsExt;
        options.mode(0o600);
    }
    let mut file = options
        .open(path)
        .with_context(|| format!("Cannot write {}", path.display()))?;
    file.write_all(data)?;
    Ok(())
}

pub fn success(command: &mut Command) -> Result<()> {
    let name = command.get_program().to_string_lossy().into_owned();
    let status = command
        .status()
        .with_context(|| format!("Cannot run {name}"))?;
    ensure!(status.success(), "{name} failed ({status})");
    Ok(())
}

pub fn capture(command: &mut Command) -> Result<String> {
    let output = command.output().context("Cannot run external command")?;
    ensure!(
        output.status.success(),
        "Command failed: {}",
        String::from_utf8_lossy(&output.stderr).trim()
    );
    Ok(String::from_utf8(output.stdout)?.trim().to_string())
}

impl Runtime {
    pub fn new(dir: PathBuf) -> Result<Self> {
        let dir = if dir.is_absolute() {
            dir
        } else {
            std::env::current_dir()?.join(dir)
        };
        Ok(Self { dir })
    }

    pub fn config_path(&self) -> PathBuf {
        self.dir.join("config.toml")
    }

    pub fn load(&self) -> Result<Config> {
        let text = fs::read_to_string(self.config_path())
            .context("Not initialized. Run hku setup (or hku init) first")?;
        let config: Config = toml::from_str(&text).context("Invalid config.toml")?;
        config.validate()?;
        Ok(config)
    }

    pub fn save(&self, config: &Config) -> Result<()> {
        config.validate()?;
        fs::create_dir_all(&self.dir)?;
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            fs::set_permissions(&self.dir, fs::Permissions::from_mode(0o700))?;
        }
        private_write(
            &self.config_path(),
            toml::to_string_pretty(config)?.as_bytes(),
        )
    }

    pub fn sync(&self) -> Result<Config> {
        let config = self.load()?;
        if config.backend != Backend::Docker {
            return Ok(config);
        }
        for (name, text) in ASSETS {
            let path = self.dir.join(name);
            fs::create_dir_all(path.parent().context("Invalid asset path")?)?;
            // Normalize Windows checkout line endings for Linux container scripts.
            private_write(&path, text.replace("\r\n", "\n").as_bytes())?;
        }
        private_write(&self.dir.join(".env"), config.dotenv()?.as_bytes())?;
        Ok(config)
    }

    pub fn compose(&self) -> Command {
        let mut command = Command::new("docker");
        command
            .arg("compose")
            .arg("--project-directory")
            .arg(&self.dir)
            .arg("--env-file")
            .arg(self.dir.join(".env"))
            .arg("-f")
            .arg(self.dir.join("compose.yaml"));
        // Values generated from config.toml must not be silently overridden by
        // unrelated environment variables or a surrounding Compose checkout.
        for name in [
            "VPN_USER",
            "VPN_SERVER",
            "VPN_USERAGENT",
            "SSH_BIND_PORT",
            "SSH_ALLOWED_TARGETS",
            "COMPOSE_PROJECT_NAME",
            "KEEPALIVE_ENABLED",
            "KEEPALIVE_HOST",
            "KEEPALIVE_PORT",
            "KEEPALIVE_INTERVAL",
            "KEEPALIVE_TIMEOUT",
            "COMPOSE_FILE",
            "COMPOSE_PROFILES",
        ] {
            command.env_remove(name);
        }
        command
    }

    pub fn run(&self, args: &[&str]) -> Result<ExitStatus> {
        self.compose()
            .args(args)
            .status()
            .context("Cannot run Docker Compose")
    }

    pub fn container(&self) -> Result<String> {
        let id = capture(self.compose().args(["ps", "-q", "vpn"]))?;
        ensure!(
            id.len() >= 12 && id.bytes().all(|c| c.is_ascii_hexdigit()),
            "Gateway is stopped. Run hku up"
        );
        Ok(id)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn embedded_runtime_needs_no_source_checkout() {
        let temp = tempfile::tempdir().unwrap();
        let runtime = Runtime::new(temp.path().into()).unwrap();
        let config = Config {
            backend: Backend::Docker,
            vpn_user: "test@example.invalid".into(),
            ..Config::default()
        };
        runtime.save(&config).unwrap();
        runtime.sync().unwrap();
        for (name, _) in ASSETS {
            let bytes = fs::read(runtime.dir.join(name)).unwrap();
            assert!(!bytes.contains(&b'\r'));
        }
        assert!(fs::read_to_string(runtime.dir.join(".env"))
            .unwrap()
            .contains("SSH_ALLOWED_TARGETS=any"));
    }
}
