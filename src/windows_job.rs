use anyhow::{ensure, Context, Result};
use std::os::windows::io::AsRawHandle;
use std::process::{Child, Command, ExitStatus};
use windows_sys::Win32::Foundation::{CloseHandle, HANDLE};
use windows_sys::Win32::System::JobObjects::{
    AssignProcessToJobObject, CreateJobObjectW, JobObjectExtendedLimitInformation,
    SetInformationJobObject, JOBOBJECT_EXTENDED_LIMIT_INFORMATION,
    JOB_OBJECT_LIMIT_KILL_ON_JOB_CLOSE,
};

struct Job(HANDLE);
impl Drop for Job {
    fn drop(&mut self) {
        // The non-inheritable handle is owned solely by this parent process.
        // Closing it also terminates any descendants left after SSH exits.
        unsafe {
            CloseHandle(self.0);
        }
    }
}

pub fn run(command: &mut Command) -> Result<ExitStatus> {
    // OpenSSH may terminate its ProxyCommand abruptly. A job object prevents
    // the proxy's nested ssh.exe process from surviving that termination.
    let handle = unsafe { CreateJobObjectW(std::ptr::null(), std::ptr::null()) };
    ensure!(
        !handle.is_null(),
        "Cannot create SSH process job: {}",
        std::io::Error::last_os_error()
    );
    let job = Job(handle);
    let mut limits: JOBOBJECT_EXTENDED_LIMIT_INFORMATION = unsafe { std::mem::zeroed() };
    limits.BasicLimitInformation.LimitFlags = JOB_OBJECT_LIMIT_KILL_ON_JOB_CLOSE;
    let configured = unsafe {
        SetInformationJobObject(
            job.0,
            JobObjectExtendedLimitInformation,
            &limits as *const _ as _,
            std::mem::size_of_val(&limits) as u32,
        )
    };
    ensure!(
        configured != 0,
        "Cannot configure SSH process job: {}",
        std::io::Error::last_os_error()
    );
    let mut child: Child = command.spawn().context("Cannot start SSH process")?;
    let assigned = unsafe { AssignProcessToJobObject(job.0, child.as_raw_handle()) };
    if assigned == 0 {
        let error = std::io::Error::last_os_error();
        let _ = child.kill();
        let _ = child.wait();
        anyhow::bail!("Cannot supervise SSH process: {error}");
    }
    Ok(child.wait()?)
}
