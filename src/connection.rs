use anyhow::{ensure, Context, Result};
use std::io::{self, IsTerminal};
use std::process::{Child, Command, Stdio};
use std::thread;
use std::time::{Duration, Instant};

use crate::runtime::{capture, success, Runtime};

// Authentication goes directly through Docker's terminal. This module only
// checks network readiness and closes its own attach client afterwards.
struct Terminal(String);
impl Drop for Terminal {
    fn drop(&mut self) {
        let _ = Command::new("stty")
            .arg(&self.0)
            .stdin(Stdio::inherit())
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .status();
    }
}

struct Attachment(Child);
impl Drop for Attachment {
    fn drop(&mut self) {
        if matches!(self.0.try_wait(), Ok(Some(_))) {
            return;
        }
        // --sig-proxy=false prevents this signal reaching the container.
        let _ = Command::new("kill")
            .args(["-TERM", &self.0.id().to_string()])
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .status();
        let deadline = Instant::now() + Duration::from_secs(2);
        while Instant::now() < deadline {
            if matches!(self.0.try_wait(), Ok(Some(_))) {
                return;
            }
            thread::sleep(Duration::from_millis(50));
        }
        let _ = self.0.kill();
        let _ = self.0.wait();
    }
}

fn state(id: &str) -> Result<i32> {
    Ok(Command::new("docker")
        .args(["exec", id, "/usr/local/lib/hku/connection-state.sh"])
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .status()
        .context("Cannot check VPN readiness")?
        .code()
        .unwrap_or(125))
}

pub fn connect(runtime: &Runtime) -> Result<i32> {
    ensure!(
        io::stdin().is_terminal() && io::stdout().is_terminal(),
        "Use an interactive terminal (remote: ssh -t SERVER /path/to/hku connect)"
    );
    let id = runtime.container()?;
    let terminal = capture(Command::new("docker").args([
        "inspect",
        "--format",
        "{{.Config.Tty}} {{.Config.OpenStdin}}",
        &id,
    ]))?;
    ensure!(
        terminal == "true true",
        "This deployment needs an updated container. Run hku up to recreate it, then reconnect"
    );
    if state(&id)? == 0 {
        println!("VPN is already connected.");
        return Ok(0);
    }
    success(Command::new("docker").args(["exec", &id, "rm", "-f", "/run/hku-vpn.exit"]))?;
    let saved = capture(Command::new("stty").arg("-g").stdin(Stdio::inherit()))?;
    let terminal_guard = Terminal(saved);
    println!(
        "Press Enter to start or continue VPN login. Enter your password and MFA when prompted."
    );
    println!("This command returns automatically when the VPN is ready. Ctrl-C cancels login.");
    let mut attachment = Attachment(
        Command::new("docker")
            .args(["attach", "--sig-proxy=false", &id])
            .spawn()
            .context("Cannot attach to the VPN terminal")?,
    );
    loop {
        if let Some(status) = attachment.0.try_wait()? {
            ensure!(status.success(), "VPN console closed ({status})");
            ensure!(
                state(&id)? == 0,
                "Console detached before VPN was ready. Run hku connect to continue"
            );
            break;
        }
        match state(&id)? {
            0 => break,
            1 => (),
            2 => anyhow::bail!(
                "VPN login ended before the tunnel was ready. Run hku connect to retry"
            ),
            code => anyhow::bail!(
                "Cannot check the container's VPN state (exit {code}). Run hku status"
            ),
        }
        thread::sleep(Duration::from_millis(500));
    }
    drop(attachment);
    drop(terminal_guard);
    println!("\nVPN connected. It will keep running after this terminal closes.");
    Ok(0)
}
