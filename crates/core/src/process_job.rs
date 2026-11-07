//! Port of `Core/ProcessJob.cs` — kill a child's whole process tree — spec 01 §3.1, 04 §4. Owner: core-events.
//! Unix: the child is a session leader (`pre_exec(setsid)`, no perl), kill = `kill(-pgid, SIGKILL)` guarded by
//! pgid > 1. Windows (`windows-sys`, target dep): `CreateJobObjectW` + `SetInformationJobObject(
//! JobObjectExtendedLimitInformation, JOB_OBJECT_LIMIT_KILL_ON_JOB_CLOSE)` + `AssignProcessToJobObject(child)`;
//! kill = `CloseHandle(job)` (KILL_ON_JOB_CLOSE ends every process of the job). ponytail: a grandchild spawned
//! before the assignment escapes — upgrade path CREATE_SUSPENDED + ResumeThread. Kill is idempotent (handle
//! swapped to 0 first). Used by `process::ClaudeProcess`, `cli::run_claude`, `git`, `status_command`.

use std::sync::atomic::AtomicI64;

pub struct ProcessJob {
    /// Unix: process group id. Windows: the job `HANDLE` as an integer. 0 = disposed.
    handle: AtomicI64,
}

impl ProcessJob {
    /// Configure a command before spawn (Unix: setsid in pre_exec; Windows: CREATE_NO_WINDOW).
    pub fn prepare(cmd: &mut tokio::process::Command) {
        todo!("port: ProcessJob.Prepare (perl setsid wrapper → pre_exec / CREATE_NO_WINDOW)")
    }

    /// Track a spawned child's tree; None (logged `ProcessJob: arbre de processus non suivi (…)`) when it cannot be
    /// tracked — the child then runs untracked. Dispatches to `attach_unix` / `attach_windows`.
    pub fn attach(child: &tokio::process::Child) -> Option<ProcessJob> {
        todo!("port: ProcessJob.Attach")
    }

    /// Unix: pgid = child pid (it is a session leader after `prepare`).
    #[cfg(unix)]
    fn attach_unix(child: &tokio::process::Child) -> Option<ProcessJob> {
        todo!("port: ProcessJob.Attach (Unix)")
    }

    /// Windows: create the job, set KILL_ON_JOB_CLOSE, assign `child.raw_handle()`; any failure closes the job → None.
    #[cfg(windows)]
    fn attach_windows(child: &tokio::process::Child) -> Option<ProcessJob> {
        todo!("port: ProcessJob.Attach (Windows: CreateJobObjectW, SetInformationJobObject, AssignProcessToJobObject)")
    }

    /// Kill every process of the tree (idempotent): Unix `kill(-pgid, SIGKILL)`, Windows `CloseHandle(job)`.
    pub fn kill(&self) {
        todo!("port: ProcessJob.Dispose")
    }
}

impl Drop for ProcessJob {
    fn drop(&mut self) {
        // ponytail: implementers call kill() here once it is ported (skeleton: no-op so tests never panic in drop).
    }
}
