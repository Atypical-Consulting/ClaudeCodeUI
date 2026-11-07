//! Port of `Core/ProcessJob.cs` — kill a child's whole process tree — spec 04 §4. Owner: core-events.
//! Unix: the child is a session leader (`pre_exec(setsid)`, no perl), dispose = `kill(-pgid, SIGKILL)` guarded by
//! pgid > 1. Windows: Job Object with KILL_ON_JOB_CLOSE (ponytail: add `win32job`/`windows-sys` when porting;
//! a child spawned before assignment escapes — upgrade path CREATE_SUSPENDED). Dispose is idempotent.
//! Used by `process::ClaudeProcess`, `cli::run_claude`, `git`, `status_command` (timeouts kill the tree).

use std::sync::atomic::AtomicI64;

pub struct ProcessJob {
    /// Unix: process group id (0 = disposed). Windows: job handle as integer.
    handle: AtomicI64,
}

impl ProcessJob {
    /// Configure a command before spawn (Unix: setsid in pre_exec; Windows: CREATE_NO_WINDOW).
    pub fn prepare(cmd: &mut tokio::process::Command) {
        todo!("port: ProcessJob.Wrap (perl setsid wrapper → pre_exec)")
    }

    /// Track a spawned child's tree; None (logged) when it cannot be tracked — the child then runs untracked.
    pub fn attach(child: &tokio::process::Child) -> Option<ProcessJob> {
        todo!("port: ProcessJob.Attach")
    }

    /// Kill every process of the tree (idempotent).
    pub fn kill(&self) {
        todo!("port: ProcessJob.Dispose")
    }
}

impl Drop for ProcessJob {
    fn drop(&mut self) {
        // ponytail: implementers call kill() here once it is ported (skeleton: no-op so tests never panic in drop).
    }
}
