//! Port of `Core/ProcessJob.cs` — kill a child's whole process tree — spec 01 §3.1, 04 §4. Owner: core-events.
//! Unix: the child is a session leader (`pre_exec(setsid)`, no perl), kill = `kill(-pgid, SIGKILL)` guarded by
//! pgid > 1. Windows (`windows-sys`, target dep): `CreateJobObjectW` + `SetInformationJobObject(
//! JobObjectExtendedLimitInformation, JOB_OBJECT_LIMIT_KILL_ON_JOB_CLOSE)` + `AssignProcessToJobObject(child)`;
//! kill = `CloseHandle(job)` (KILL_ON_JOB_CLOSE ends every process of the job). ponytail: a grandchild spawned
//! before the assignment escapes — upgrade path CREATE_SUSPENDED + ResumeThread. Kill is idempotent (handle
//! swapped to 0 first). Used by `process::ClaudeProcess`, `cli::run_claude`, `git`, `status_command`.
//! On Unix a descendant that calls setsid itself escapes.

use std::sync::atomic::{AtomicI64, Ordering};

pub struct ProcessJob {
    /// Unix: process group id. Windows: the job `HANDLE` as an integer. 0 = disposed.
    handle: AtomicI64,
}

impl ProcessJob {
    /// Configure a command before spawn (Unix: setsid in pre_exec; Windows: CREATE_NO_WINDOW).
    pub fn prepare(cmd: &mut tokio::process::Command) {
        #[cfg(unix)]
        // SAFETY: setsid is async-signal-safe; the closure allocates nothing.
        unsafe {
            cmd.pre_exec(|| {
                if libc::setsid() == -1 {
                    return Err(std::io::Error::last_os_error());
                }
                Ok(())
            });
        }
        #[cfg(windows)]
        cmd.creation_flags(0x0800_0000); // CREATE_NO_WINDOW
    }

    /// Track a spawned child's tree; None (logged `ProcessJob: arbre de processus non suivi (…)`) when it cannot be
    /// tracked — the child then runs untracked. Dispatches to `attach_unix` / `attach_windows`.
    pub fn attach(child: &tokio::process::Child) -> Option<ProcessJob> {
        #[cfg(unix)]
        let job = Self::attach_unix(child);
        #[cfg(windows)]
        let job = Self::attach_windows(child);
        #[cfg(not(any(unix, windows)))]
        let job: Result<ProcessJob, String> = Err("unsupported platform".into());
        job.map_err(|e| eprintln!("ProcessJob: arbre de processus non suivi ({e})")).ok()
    }

    /// Unix: pgid = child pid (it is a session leader after `prepare`).
    #[cfg(unix)]
    fn attach_unix(child: &tokio::process::Child) -> Result<ProcessJob, String> {
        let pid = child.id().ok_or("the process already exited")?;
        Ok(ProcessJob { handle: AtomicI64::new(pid as i64) })
    }

    /// Windows: create the job, set KILL_ON_JOB_CLOSE, assign `child.raw_handle()`; any failure closes the job → None.
    #[cfg(windows)]
    fn attach_windows(child: &tokio::process::Child) -> Result<ProcessJob, String> {
        use windows_sys::Win32::Foundation::{CloseHandle, GetLastError, HANDLE};
        use windows_sys::Win32::System::JobObjects::{
            AssignProcessToJobObject, CreateJobObjectW, JobObjectExtendedLimitInformation, SetInformationJobObject,
            JOBOBJECT_EXTENDED_LIMIT_INFORMATION, JOB_OBJECT_LIMIT_KILL_ON_JOB_CLOSE,
        };
        let process = child.raw_handle().ok_or("the process already exited")? as HANDLE;
        // SAFETY: plain Win32 calls on handles we own; the info struct outlives the call.
        unsafe {
            let job = CreateJobObjectW(std::ptr::null(), std::ptr::null());
            if job.is_null() {
                return Err(format!("CreateJobObjectW failed ({})", GetLastError()));
            }
            let mut info: JOBOBJECT_EXTENDED_LIMIT_INFORMATION = Default::default();
            info.BasicLimitInformation.LimitFlags = JOB_OBJECT_LIMIT_KILL_ON_JOB_CLOSE;
            let ok = SetInformationJobObject(
                job,
                JobObjectExtendedLimitInformation,
                &info as *const JOBOBJECT_EXTENDED_LIMIT_INFORMATION as *const core::ffi::c_void,
                std::mem::size_of::<JOBOBJECT_EXTENDED_LIMIT_INFORMATION>() as u32,
            ) != 0
                && AssignProcessToJobObject(job, process) != 0;
            if !ok {
                let err = GetLastError();
                CloseHandle(job);
                return Err(format!("Win32 error {err}"));
            }
            Ok(ProcessJob { handle: AtomicI64::new(job as isize as i64) })
        }
    }

    /// Kill every process of the tree (idempotent): Unix `kill(-pgid, SIGKILL)`, Windows `CloseHandle(job)`.
    pub fn kill(&self) {
        let h = self.handle.swap(0, Ordering::SeqCst);
        #[cfg(unix)]
        // pgid > 1 only: kill(-1) would hit every process of the user, kill(0) our own group.
        if h > 1 && h <= i32::MAX as i64 {
            // SAFETY: plain syscall.
            unsafe {
                libc::kill(-(h as i32), libc::SIGKILL);
            }
        }
        #[cfg(windows)]
        if h != 0 {
            // SAFETY: the handle came from CreateJobObjectW and is closed once (swapped to 0 above).
            unsafe {
                windows_sys::Win32::Foundation::CloseHandle(h as isize as windows_sys::Win32::Foundation::HANDLE);
            }
        }
    }
}

impl Drop for ProcessJob {
    fn drop(&mut self) {
        self.kill();
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::process::Stdio;
    use std::time::Duration;
    use tokio::io::AsyncReadExt;

    /// Kills only the parent of a two-level tree, disposes the job, and expects the grandchild (holder of the
    /// stdout pipe) gone.
    #[cfg(unix)]
    #[tokio::test]
    async fn kills_the_grandchild() {
        let mut cmd = tokio::process::Command::new("/bin/sh");
        cmd.args(["-c", "sleep 60 & sleep 60"]).stdout(Stdio::piped()).stdin(Stdio::null());
        ProcessJob::prepare(&mut cmd);
        let mut child = cmd.spawn().unwrap();
        let job = ProcessJob::attach(&child).expect("ProcessJob: attach returned None");
        tokio::time::sleep(Duration::from_millis(500)).await;
        // The child leads its own group, distinct from ours.
        let pid = child.id().unwrap() as i32;
        assert_eq!(unsafe { libc::getpgid(pid) }, pid);
        child.start_kill().unwrap();
        let _ = child.wait().await;
        job.kill();
        job.kill(); // idempotent
        let mut out = child.stdout.take().unwrap();
        let mut buf = Vec::new();
        let eof = tokio::time::timeout(Duration::from_secs(5), out.read_to_end(&mut buf)).await;
        assert!(eof.is_ok(), "ProcessJob: the grandchild survives");
    }
}
