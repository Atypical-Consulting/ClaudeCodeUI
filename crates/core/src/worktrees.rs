//! Port of `Core/WorktreeService.cs` (I/O): discover repos, scan/classify worktrees, run a cleanup plan, push,
//! size — spec 04 §1. Owner: core-worktrees-files. Rules (classify, cleanable, plan…) are pure in
//! `ccui_shared::worktree_rules`. INVARIANT: never `--force`, `-f`, `-D`, `--delete-force` (run refuses them).
//!
//! Paths: `norm` = absolute, `.`/`..` collapsed lexically (no symlink resolution), trailing separator trimmed,
//! `\` → `/`; every comparison case-insensitive. Caches: live heads 2 min, merged PRs 10 min, behind forever,
//! lsof 5 s; global git semaphore (`git::run`). `repos.json` in app data (sorted, case-insensitive set).

use crate::session_manager::SessionManager;
use crate::{CoreResult, UiSink};
use ccui_shared::api::WorktreeCache;
use ccui_shared::worktree_rules::{CleanupStep, WorktreeInfo};
use parking_lot::Mutex;
use std::collections::HashMap;
use std::sync::atomic::AtomicBool;
use std::sync::Arc;

pub struct WorktreeService {
    sessions: Arc<SessionManager>,
    sink: Arc<dyn UiSink>,
    /// repo (lowercased) → Safe + Orphan count of its last scan.
    cleanable: Mutex<HashMap<String, i32>>,
    last_scan: Mutex<HashMap<String, Vec<WorktreeInfo>>>,
    cached_at: Mutex<Option<ccui_shared::Time>>,
    /// Transcript-discovered cwds (until refresh).
    repos: Mutex<Option<Vec<String>>>,
    /// repos.json (lazy-loaded).
    remembered: Mutex<Option<Vec<String>>>,
    /// Cancel flags of running cleanup runs by run id.
    runs: Mutex<HashMap<u64, Arc<AtomicBool>>>,
}

impl WorktreeService {
    pub fn new(sessions: Arc<SessionManager>, sink: Arc<dyn UiSink>) -> Arc<WorktreeService> {
        todo!("port: WorktreeService constructor")
    }

    /// Roots of live sessions ∪ remembered ∪ recent transcripts (refresh re-reads transcripts), sorted.
    pub async fn discover_repos(&self, refresh: bool) -> CoreResult<Vec<String>> {
        todo!("port: WorktreeService.DiscoverReposAsync")
    }

    /// Rows of one repo (worktrees except the main one, then orphan `worktree-*` branches). With `only`, just that
    /// path's row and no cache/count update. A per-repo git failure → Err(stderr); git missing → Err(Wt.GitMissing).
    pub async fn scan(&self, repo: &str, only: Option<&str>) -> CoreResult<Vec<WorktreeInfo>> {
        todo!("port: WorktreeService.ScanAsync")
    }

    /// Last full scan (Overview / Worktrees reuse it under 2 min).
    pub fn cached(&self) -> WorktreeCache {
        todo!("port: WorktreeService.Cached / CachedAt")
    }

    /// Sum of Safe + Orphan of the last scans; None when nothing was scanned.
    pub fn cleanable_count(&self) -> Option<i32> {
        todo!("port: WorktreeService.CleanableCount")
    }

    /// Emits `Push::WorktreeCount` only when the repo is new or its count changed.
    fn set_cleanable(&self, repo: &str, n: i32) {
        todo!("port: WorktreeService.SetCleanable")
    }

    /// Recursive size, links and reparse points skipped, inaccessible entries ignored (blocking: spawn_blocking).
    pub fn size(path: &str) -> i64 {
        todo!("port: WorktreeService.Size")
    }

    /// `git -C <path> push -u origin <branch>` → exit code; detached → -1.
    pub async fn push(&self, path: &str, branch: Option<&str>) -> CoreResult<i32> {
        todo!("port: WorktreeService.PushAsync")
    }

    /// `git check-ignore -q .claude/worktrees/x` exits 0.
    pub async fn ignores_worktrees(&self, repo: &str) -> bool {
        todo!("port: WorktreeService.IgnoresWorktreesAsync")
    }

    /// Clean the chosen worktrees. The plan is REBUILT here with `worktree_rules::plan` from this service's own
    /// cached rows (`last_scan`) for `paths` (unknown paths skipped): command arguments never come from the webview
    /// (a `-c core.fsmonitor=…` would be code execution). `kept_ok` = paths whose kept files were acknowledged.
    /// Then step by step, re-validating each path's row first; emit `Push::WorktreeRunStep` per step and
    /// `Push::WorktreeRunDone`; full rescan of each repo after. Cancellable by `cancel_run(run_id)`.
    pub async fn run(&self, run_id: u64, paths: Vec<String>, kept_ok: Vec<String>) -> CoreResult<()> {
        todo!("port: WorktreeService.RunAsync")
    }

    pub fn cancel_run(&self, run_id: u64) {
        todo!("port: (Worktrees page CancellationTokenSource)")
    }
}

/// Lexical normalization (see module doc).
pub fn norm(path: &str) -> String {
    todo!("port: WorktreeService.Norm")
}

/// `norm(cwd) == path` or starts with `path + "/"` (case-insensitive).
pub fn inside(cwd: &str, path: &str) -> bool {
    todo!("port: WorktreeService.Inside")
}

/// Remove `[\\/]\.claude[\\/]worktrees[\\/][^\\/]+$` (a `claude -w` cwd maps to its repo).
pub fn strip_worktree(cwd: &str) -> String {
    todo!("port: WorktreeService.StripWorktree")
}

/// Process exists and has not exited (Unix `kill(pid, 0) == 0 || EPERM`).
pub fn alive(pid: i32) -> bool {
    todo!("port: WorktreeService.Alive")
}
