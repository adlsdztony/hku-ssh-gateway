use super::*;
use nix::sys::termios::{self, SetArg, Termios};
use std::io::{self, IsTerminal};
use std::net::Shutdown;
use std::os::unix::net::UnixStream;
use std::process::{Command, Stdio};
use std::thread;
use std::time::{Duration, Instant};

fn socket(runtime: &Runtime) -> Result<UnixStream> {
    let d = deployment(runtime)?;
    UnixStream::connect(d.socket()).context("Native gateway is not running; run hku up")
}
fn request(runtime: &Runtime, operation: Request) -> Result<(UnixStream, Reply)> {
    let mut stream = socket(runtime)?;
    send_json(&mut stream, &operation)?;
    let reply: Reply = read_json(&mut stream)?;
    ensure!(reply.ok, "{}", reply.message);
    Ok((stream, reply))
}
fn admin(runtime: &Runtime, remove: bool) -> Result<()> {
    let d = deployment(runtime)?;
    let mut command = Command::new("sudo");
    command
        .arg("--")
        .arg(std::env::current_exe()?)
        .arg("native-internal");
    if remove {
        command.args(["remove", "--id", &d.id]);
    } else {
        command.arg("install").stdin(Stdio::piped());
    }
    let mut child = command
        .spawn()
        .context("Cannot request administrator access using sudo")?;
    if !remove {
        let mut input = child.stdin.take().context("Missing installer pipe")?;
        serde_json::to_writer(&mut input, &d)?;
    }
    ensure!(
        child.wait()?.success(),
        "Native administrator operation failed"
    );
    Ok(())
}
pub fn up(runtime: &Runtime) -> Result<i32> {
    admin(runtime, false)?;
    let deadline = Instant::now() + Duration::from_secs(20);
    while Instant::now() < deadline {
        if let Ok((_, reply)) = request(runtime, Request::Status) {
            println!("{}", reply.message);
            return Ok(0);
        }
        thread::sleep(Duration::from_millis(250));
    }
    anyhow::bail!(
        "Native service did not become ready; inspect sudo journalctl -u {}",
        deployment(runtime)?.unit()
    )
}
pub fn down(runtime: &Runtime) -> Result<i32> {
    admin(runtime, true)?;
    Ok(0)
}
pub fn status(runtime: &Runtime) -> Result<i32> {
    let (_, reply) = request(runtime, Request::Status)?;
    println!("{}", reply.message);
    Ok(if reply.ready { 0 } else { 2 })
}
pub fn logs(runtime: &Runtime) -> Result<i32> {
    println!("{}", request(runtime, Request::Logs)?.1.message);
    Ok(0)
}
pub fn disconnect(runtime: &Runtime) -> Result<i32> {
    println!("{}", request(runtime, Request::Disconnect)?.1.message);
    Ok(0)
}
pub fn proxy(runtime: &Runtime, host: &str, port: u16) -> Result<i32> {
    let (mut stream, _) = request(
        runtime,
        Request::Proxy {
            host: host.into(),
            port,
        },
    )?;
    let mut input = stream.try_clone()?;
    thread::spawn(move || {
        let _ = io::copy(&mut io::stdin(), &mut input);
        let _ = input.shutdown(Shutdown::Write);
    });
    io::copy(&mut stream, &mut raw_stdout()?)?;
    Ok(0)
}

struct Terminal(Termios);
impl Drop for Terminal {
    fn drop(&mut self) {
        let _ = termios::tcsetattr(io::stdin(), SetArg::TCSANOW, &self.0);
    }
}
pub fn connect(runtime: &Runtime) -> Result<i32> {
    ensure!(
        io::stdin().is_terminal() && io::stdout().is_terminal(),
        "Use an interactive terminal for VPN login"
    );
    let (mut stream, reply) = request(runtime, Request::Connect)?;
    if reply.ready {
        println!("VPN is already connected.");
        return Ok(0);
    }
    let saved = termios::tcgetattr(io::stdin())?;
    let guard = Terminal(saved.clone());
    let mut raw = saved;
    termios::cfmakeraw(&mut raw);
    termios::tcsetattr(io::stdin(), SetArg::TCSANOW, &raw)?;
    let mut writer = stream.try_clone()?;
    thread::spawn(move || {
        let mut bytes = [0u8; 4096];
        loop {
            match io::stdin().read(&mut bytes) {
                Ok(0) | Err(_) => break,
                Ok(size) => {
                    if send_frame(&mut writer, b'I', &bytes[..size]).is_err() {
                        break;
                    }
                }
            }
        }
    });
    let result = loop {
        match read_frame(&mut stream) {
            Ok((b'O', data)) => {
                io::stdout().write_all(&data)?;
                io::stdout().flush()?;
            }
            Ok((b'R', _)) => break Ok(0),
            Ok((b'E', data)) => {
                break Err(anyhow::anyhow!(String::from_utf8_lossy(&data).into_owned()))
            }
            Ok(_) => break Err(anyhow::anyhow!("Invalid terminal response")),
            Err(e) => break Err(e.context("Native VPN terminal disconnected")),
        }
    };
    let _ = stream.shutdown(Shutdown::Both);
    drop(guard);
    if result.is_ok() {
        println!("\nVPN connected. It will keep running after this terminal closes.");
    }
    result
}
