use std::process::Command;

#[test]
fn remote_client_options_do_not_require_local_deployment() {
    let output = Command::new(env!("CARGO_BIN_EXE_hku"))
        .args(["ssh", "--help"])
        .output()
        .unwrap();
    assert!(output.status.success());
    let help = String::from_utf8(output.stdout).unwrap();
    assert!(help.contains("--server"));
    assert!(help.contains("--gateway-bin"));
    assert!(!help.contains("--public-key"));
}

#[test]
#[cfg(target_os = "linux")]
fn initializes_without_a_key_or_docker() {
    let temp = tempfile::tempdir().unwrap();
    let output = Command::new(env!("CARGO_BIN_EXE_hku"))
        .arg("--state-dir")
        .arg(temp.path())
        .args([
            "init",
            "--backend",
            "docker",
            "--user",
            "test@example.invalid",
        ])
        .output()
        .unwrap();
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    assert!(temp.path().join("config.toml").is_file());
    assert!(!temp.path().join("authorized_keys").exists());
    let compose = std::fs::read_to_string(temp.path().join("compose.yaml")).unwrap();
    assert!(!compose
        .lines()
        .any(|line| line.trim().starts_with("ports:")));
}

#[test]
#[cfg(not(target_os = "linux"))]
fn non_linux_builds_are_client_only() {
    let temp = tempfile::tempdir().unwrap();
    let output = Command::new(env!("CARGO_BIN_EXE_hku"))
        .arg("--state-dir")
        .arg(temp.path())
        .args(["init", "--user", "test@example.invalid"])
        .output()
        .unwrap();
    assert!(!output.status.success());
    assert!(String::from_utf8_lossy(&output.stderr).contains("Linux-only"));
    assert!(!temp.path().join("config.toml").exists());
}
