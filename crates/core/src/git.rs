//! Git invocation contracts — spec 04 §1.12 (WorktreeService) and spec 01 §5.7 (LiveSession.Git / GitRoot).
//! Owner: core-worktrees-files.
//! Every call: program `git`, args `-C <dir> …`, stdout/stderr drained concurrently (a chatty diff must not block),
//! optional stdin written after the readers started, no window on Windows.

use std::time::Duration;

#[derive(Clone, Debug, Default, PartialEq)]
pub struct GitOut {
    pub code: i32,
    pub stdout: String,
    pub stderr: String,
}

/// WorktreeService contract: env `GIT_OPTIONAL_LOCKS=0`, `GIT_TERMINAL_PROMPT=0`, `GIT_SSH_COMMAND` (BatchMode,
/// ConnectTimeout=8 unless set), under the global semaphore (available_parallelism permits), optional timeout
/// (kill tree → Err). Spawn failure → Err(`Wt.GitMissing(os error)`), distinct from a non-zero exit (Ok).
pub async fn run(dir: &str, args: &[&str], stdin: Option<String>, timeout: Option<Duration>) -> Result<GitOut, String> {
    todo!("port: WorktreeService.GitAsync")
}

/// LiveSession.Git: exit 0 → Some(stdout), anything else (incl. spawn failure) → None.
pub async fn output(dir: &str, args: &[&str]) -> Option<String> {
    todo!("port: LiveSession.Git")
}

/// `gh` with `GH_PROMPT_DISABLED=1`, 15 s timeout, cwd = dir: Some(stdout) on exit 0, None otherwise (gh missing
/// included).
pub async fn gh(dir: &str, args: &[&str]) -> Option<String> {
    todo!("port: WorktreeService.Gh")
}

/// `LiveSession.GitRoot(dir)`: the main checkout root (`--git-common-dir` whose name is `.git` → its parent), else None.
pub async fn root(dir: &str) -> Option<String> {
    todo!("port: LiveSession.GitRoot")
}

/// `git -C dir branch --show-current`: empty output → "HEAD" (detached); failure → None (NewSession.Branch).
pub async fn branch(dir: &str) -> Option<String> {
    todo!("port: NewSession.Branch")
}
