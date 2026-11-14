use super::*;
use nix::sys::signal::{killpg, Signal};
use nix::unistd::{Gid, Pid, Uid};
use std::collections::VecDeque;
use std::fs::{self, File};
use std::io;
use std::net::{Shutdown, TcpStream, ToSocketAddrs};
use std::os::unix::fs::PermissionsExt;
use std::os::unix::net::{UnixListener, UnixStream};
use std::os::unix::process::CommandExt;
use std::process::{Child, Stdio};
use std::sync::{
    atomic::{AtomicBool, AtomicUsize, Ordering},
    mpsc, Arc, Mutex,
};
use std::thread;
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};

enum Event {
    Output(Vec<u8>),
    Ready,
    Failed(String),
}
#[derive(Default)]
struct Session {
    child: Option<Child>,
    writer: Option<File>,
    ready: bool,
    generation: u64,
    attached: bool,
    events: Option<mpsc::Sender<Event>>,
    logs: VecDeque<String>,
    last_probe: String,
    failures: u32,
}
type Shared = Arc<Mutex<Session>>;

fn log(session: &mut Session, text: String) {
    session.logs.push_back(text);
    // Leave room for JSON escaping within the control protocol's frame limit.
    while session.logs.len() > 100
        || session.logs.iter().map(String::len).sum::<usize>() > MAX_FRAME / 8
    {
        session.logs.pop_front();
    }
}
fn reply(stream: &mut UnixStream, message: String, ready: bool) -> Result<()> {
    send_json(
        stream,
        &Reply {
            ok: true,
            message,
            ready,
        },
    )
}

fn start(d: &Deployment, shared: &Shared) -> Result<()> {
    let pair = nix::pty::openpty(
        Some(&nix::pty::Winsize {
            ws_row: 30,
            ws_col: 100,
            ws_xpixel: 0,
            ws_ypixel: 0,
        }),
        None,
    )?;
    for fd in [&pair.master, &pair.slave] {
        nix::fcntl::fcntl(
            fd,
            nix::fcntl::FcntlArg::F_SETFD(nix::fcntl::FdFlag::FD_CLOEXEC),
        )?;
    }
    let mut master = File::from(pair.master);
    let slave = File::from(pair.slave);
    let network = network::Network::new(d)?;
    let script = format!(
        "{} native-internal hook --id {}",
        d.binary().display(),
        d.id
    );
    let (program, arguments) = bundle::openconnect(d)?;
    let mut command = network.command(program);
    command.args(arguments);
    for variable in ["LD_PRELOAD", "LD_LIBRARY_PATH", "LD_AUDIT"] {
        command.env_remove(variable);
    }
    command.args([
        "--protocol=anyconnect",
        "--interface=tun0",
        &format!("--useragent={}", d.config.vpn_useragent),
        &format!("--user={}", d.config.vpn_user),
        &format!("--script={script}"),
        &d.config.vpn_server,
    ]);
    command
        .stdin(Stdio::from(slave.try_clone()?))
        .stdout(Stdio::from(slave.try_clone()?))
        .stderr(Stdio::from(slave));
    // The daemon owns the PTY. Closing a client socket never closes this master.
    unsafe {
        command.pre_exec(|| {
            if nix::libc::setsid() < 0 {
                return Err(io::Error::last_os_error());
            }
            if nix::libc::ioctl(0, nix::libc::TIOCSCTTY, 0) < 0 {
                return Err(io::Error::last_os_error());
            }
            Ok(())
        });
    }
    let mut session = shared.lock().unwrap();
    session.child = Some(command.spawn().context("Cannot start native OpenConnect")?);
    session.writer = Some(master.try_clone()?);
    session.ready = false;
    session.generation += 1;
    let generation = session.generation;
    drop(session);
    let shared = Arc::clone(shared);
    thread::spawn(move || {
        let mut data = [0u8; 4096];
        while let Ok(size) = master.read(&mut data) {
            if size == 0 {
                break;
            }
            let mut state = shared.lock().unwrap();
            if state.generation != generation {
                break;
            }
            // No authentication transcript is retained, even when a client drops.
            if state.ready {
                log(
                    &mut state,
                    String::from_utf8_lossy(&data[..size]).into_owned(),
                );
            }
            if let Some(sender) = &state.events {
                let _ = sender.send(Event::Output(data[..size].to_vec()));
            }
        }
    });
    Ok(())
}

fn attach(mut stream: UnixStream, d: &Deployment, shared: &Shared) -> Result<()> {
    let (sender, receiver) = mpsc::channel();
    {
        let mut state = shared.lock().unwrap();
        if state.ready {
            return reply(&mut stream, "VPN is already connected.".into(), true);
        }
        ensure!(
            !state.attached,
            "Another terminal is already attached to VPN login"
        );
        state.attached = true;
        state.events = Some(sender);
    }
    let result = (|| -> Result<()> {
        reply(&mut stream, "Native OpenConnect terminal".into(), false)?;
        let running = shared.lock().unwrap().child.is_some();
        if !running {
            start(d, shared)?;
        }
        let mut input = stream.try_clone()?;
        let shared_input = Arc::clone(shared);
        let active = Arc::new(AtomicBool::new(true));
        let input_active = Arc::clone(&active);
        let generation = shared.lock().unwrap().generation;
        thread::spawn(move || {
            while let Ok((b'I', bytes)) = read_frame(&mut input) {
                let mut state = shared_input.lock().unwrap();
                if state.generation != generation {
                    break;
                }
                if let Some(writer) = &mut state.writer {
                    if writer.write_all(&bytes).is_err() {
                        break;
                    }
                } else {
                    break;
                }
            }
            input_active.store(false, Ordering::Relaxed);
        });
        while active.load(Ordering::Relaxed) {
            match receiver.recv_timeout(Duration::from_millis(200)) {
                Ok(Event::Output(data)) => send_frame(&mut stream, b'O', &data)?,
                Ok(Event::Ready) => {
                    send_frame(&mut stream, b'R', b"")?;
                    break;
                }
                Ok(Event::Failed(error)) => {
                    send_frame(&mut stream, b'E', error.as_bytes())?;
                    break;
                }
                Err(mpsc::RecvTimeoutError::Timeout) => (),
                Err(_) => break,
            }
        }
        Ok(())
    })();
    if let Err(error) = &result {
        let _ = send_frame(&mut stream, b'E', error.to_string().as_bytes());
    }
    let _ = stream.shutdown(Shutdown::Both);
    let mut state = shared.lock().unwrap();
    state.events = None;
    state.attached = false;
    result
}

fn stop(shared: &Shared) -> Result<()> {
    if let Some(child) = &shared.lock().unwrap().child {
        killpg(Pid::from_raw(child.id() as i32), Signal::SIGINT)?;
    }
    Ok(())
}
fn handle(mut stream: UnixStream, d: Deployment, shared: Shared) -> Result<()> {
    let credentials =
        nix::sys::socket::getsockopt(&stream, nix::sys::socket::sockopt::PeerCredentials)?;
    ensure!(
        credentials.uid() == d.owner || credentials.uid() == 0,
        "Native gateway belongs to another user"
    );
    stream.set_read_timeout(Some(Duration::from_secs(5)))?;
    let operation: Request = read_json(&mut stream)?;
    stream.set_read_timeout(None)?;
    match operation {
        Request::Connect => attach(stream, &d, &shared),
        Request::Disconnect => {
            stop(&shared)?;
            reply(&mut stream, "VPN disconnect requested.".into(), false)
        }
        Request::Status => {
            let state = shared.lock().unwrap();
            reply(&mut stream, format!("Backend: native\nNamespace: {}\nVPN: {}\nKeepalive: {}\nConsecutive probe failures: {}\n", d.namespace(), if state.ready { "connected" } else if state.child.is_some() { "authenticating/reconnecting" } else { "offline" }, state.last_probe, state.failures), state.ready)
        }
        Request::Logs => {
            let state = shared.lock().unwrap();
            reply(
                &mut stream,
                state.logs.iter().cloned().collect::<String>(),
                state.ready,
            )
        }
        Request::Proxy { host, port } => bridge(stream, &d, &host, port),
    }
}

fn bridge(mut stream: UnixStream, d: &Deployment, host: &str, port: u16) -> Result<()> {
    ensure!(
        port > 0 && (crate::config::host(host) || host.parse::<std::net::IpAddr>().is_ok()),
        "Invalid proxy destination"
    );
    ensure!(
        d.config.allowed_targets == ["any"]
            || d.config.allowed_targets.contains(&format!("{host}:{port}")),
        "Destination is excluded by allowed_targets"
    );
    let mut child = network::Network::new(d)?
        .command(d.binary())
        .args([
            "native-internal",
            "stream",
            "--owner",
            &d.owner.to_string(),
            "--group",
            &d.group.to_string(),
            "--host",
            host,
            "--port",
            &port.to_string(),
        ])
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::null())
        .spawn()?;
    let result = (|| -> Result<()> {
        let mut output = child.stdout.take().context("Missing proxy output")?;
        let mut connected = [1u8];
        output
            .read_exact(&mut connected)
            .context("VPN-only connection failed")?;
        ensure!(connected == [0], "VPN-only connection failed");
        reply(&mut stream, "Connected".into(), true)?;
        let mut input = stream.try_clone()?;
        let mut stdin = child.stdin.take().context("Missing proxy input")?;
        thread::spawn(move || {
            let _ = relay(&mut input, &mut stdin);
        });
        let _ = relay(&mut output, &mut stream);
        let _ = stream.shutdown(Shutdown::Both);
        Ok(())
    })();
    if let Err(error) = &result {
        let _ = send_json(
            &mut stream,
            &Reply {
                ok: false,
                message: error.to_string(),
                ready: false,
            },
        );
    }
    let _ = child.kill();
    let _ = child.wait();
    result
}

pub fn run(d: Deployment) -> Result<i32> {
    d.validate()?;
    // /run is volatile. Recreate the root-owned socket directory after reboot.
    admin::prepare_runtime()?;
    let network = network::Network::new(&d)?;
    network.create(&d)?;
    let result = serve(&d, &network);
    let _ = network.destroy();
    let _ = fs::remove_file(d.socket());
    result
}
fn serve(d: &Deployment, network: &network::Network) -> Result<i32> {
    let _ = fs::remove_file(d.socket());
    let listener = UnixListener::bind(d.socket())?;
    fs::set_permissions(d.socket(), fs::Permissions::from_mode(0o600))?;
    nix::unistd::chown(
        &d.socket(),
        Some(Uid::from_raw(d.owner)),
        Some(Gid::from_raw(d.group)),
    )?;
    listener.set_nonblocking(true)?;
    let shared = Arc::new(Mutex::new(Session {
        last_probe: "waiting for VPN".into(),
        ..Session::default()
    }));
    let shutdown = Arc::new(AtomicBool::new(false));
    for signal in [nix::libc::SIGTERM, nix::libc::SIGINT] {
        signal_hook::flag::register(signal, Arc::clone(&shutdown))?;
    }
    let mut next_check = Instant::now();
    let clients = Arc::new(AtomicUsize::new(0));
    let mut next_probe = Instant::now();
    while !shutdown.load(Ordering::Relaxed) {
        match listener.accept() {
            Ok((stream, _)) => {
                if clients.fetch_add(1, Ordering::Relaxed) >= 64 {
                    clients.fetch_sub(1, Ordering::Relaxed);
                    drop(stream);
                    continue;
                }
                let clients = Arc::clone(&clients);
                let deployment = d.clone();
                let state = Arc::clone(&shared);
                thread::spawn(move || {
                    let mut errors = stream.try_clone().ok();
                    if let Err(error) = handle(stream, deployment, state) {
                        if let Some(stream) = &mut errors {
                            let _ = send_json(
                                stream,
                                &Reply {
                                    ok: false,
                                    message: error.to_string(),
                                    ready: false,
                                },
                            );
                        }
                    }
                    clients.fetch_sub(1, Ordering::Relaxed);
                });
            }
            Err(e) if e.kind() == io::ErrorKind::WouldBlock => (),
            Err(e) => return Err(e.into()),
        }
        if Instant::now() >= next_check {
            let mut state = shared.lock().unwrap();
            let exited = if let Some(child) = &mut state.child {
                child.try_wait()?
            } else {
                None
            };
            if let Some(status) = exited {
                state.child = None;
                state.writer = None;
                state.ready = false;
                if let Some(sender) = &state.events {
                    let _ = sender.send(Event::Failed(format!(
                        "VPN process exited ({status}); run hku connect to retry"
                    )));
                }
            }
            let ready = state.child.is_some() && network.ready();
            if ready && !state.ready {
                if let Some(sender) = &state.events {
                    let _ = sender.send(Event::Ready);
                }
                next_probe = Instant::now();
            }
            state.ready = ready;
            next_check = Instant::now() + Duration::from_millis(500);
        }
        if d.config.keepalive.enabled
            && shared.lock().unwrap().ready
            && Instant::now() >= next_probe
        {
            let probe = network
                .command(d.binary())
                .args([
                    "native-internal",
                    "probe",
                    "--owner",
                    &d.owner.to_string(),
                    "--group",
                    &d.group.to_string(),
                    "--host",
                    &d.config.keepalive.host,
                    "--port",
                    &d.config.keepalive.port.to_string(),
                    "--timeout",
                    &d.config.keepalive.timeout.to_string(),
                ])
                .stdout(Stdio::null())
                .stderr(Stdio::null())
                .status();
            let mut state = shared.lock().unwrap();
            if probe.is_ok_and(|s| s.success()) {
                state.failures = 0;
                state.last_probe = format!(
                    "ok (Unix time {})",
                    SystemTime::now().duration_since(UNIX_EPOCH)?.as_secs()
                );
            } else {
                state.failures += 1;
                state.last_probe = "failed".into();
            }
            next_probe =
                Instant::now() + Duration::from_secs(u64::from(d.config.keepalive.interval));
        }
        thread::sleep(Duration::from_millis(50));
    }
    let _ = stop(&shared);
    let deadline = Instant::now() + Duration::from_secs(5);
    while Instant::now() < deadline {
        let mut state = shared.lock().unwrap();
        if let Some(child) = &mut state.child {
            if child.try_wait()?.is_some() {
                break;
            }
        } else {
            break;
        }
        drop(state);
        thread::sleep(Duration::from_millis(100));
    }
    Ok(0)
}

fn drop_privileges(owner: u32, group: u32) -> Result<()> {
    ensure!(owner > 0, "Refusing a privileged proxy UID");
    nix::unistd::setgroups(&[])?;
    nix::unistd::setgid(Gid::from_raw(group))?;
    nix::unistd::setuid(Uid::from_raw(owner))?;
    Ok(())
}
fn tcp(host: &str, port: u16, timeout: u32) -> Result<TcpStream> {
    ensure!(
        port > 0 && (crate::config::host(host) || host.parse::<std::net::IpAddr>().is_ok()),
        "Invalid TCP destination"
    );
    let mut last = None;
    for address in (host, port).to_socket_addrs()?.take(8) {
        match TcpStream::connect_timeout(&address, Duration::from_secs(u64::from(timeout))) {
            Ok(socket) => return Ok(socket),
            Err(e) => last = Some(e),
        }
    }
    Err(last
        .map(anyhow::Error::from)
        .unwrap_or_else(|| anyhow::anyhow!("No destination addresses")))
}
pub fn stream_child(owner: u32, group: u32, host: &str, port: u16) -> Result<i32> {
    drop_privileges(owner, group)?;
    let mut socket = tcp(host, port, 10)?;
    let mut output = raw_stdout()?;
    output.write_all(&[0])?;
    let mut input = socket.try_clone()?;
    thread::spawn(move || {
        let _ = relay(&mut io::stdin(), &mut input);
        let _ = input.shutdown(Shutdown::Write);
    });
    relay(&mut socket, &mut output)?;
    Ok(0)
}
pub fn probe_child(owner: u32, group: u32, host: &str, port: u16, timeout: u32) -> Result<i32> {
    drop_privileges(owner, group)?;
    let socket = tcp(host, port, timeout)?;
    socket.shutdown(Shutdown::Both)?;
    Ok(0)
}
