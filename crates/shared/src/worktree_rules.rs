//! Port of the records and pure rules of `Core/WorktreeService.cs` (classify, cleanable, plan, anchor…) — spec 04
//! §1.2, §1.7 (pure helpers), §1.8-1.10. Owner: core-worktrees-files. Scans and git I/O are `ccui_core::worktrees`.
//! INVARIANT: no plan ever contains `--force`, `-f`, `-D`, `--delete-force`.

use crate::model::Time;
use serde::{Deserialize, Serialize};

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum WtState {
    Safe,
    Check,
    Active,
    Orphan,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct WorktreeInfo {
    /// Normalized repo root.
    pub repo: String,
    /// Normalized worktree path; branch-only rows: "{repo}/refs/heads/{branch}".
    pub path: String,
    /// Folder name; branch-only rows: the full branch name.
    pub name: String,
    /// None = detached HEAD.
    pub branch: Option<String>,
    pub head: Option<String>,
    pub state: WtState,
    /// HTML (`<b>…</b>`; interpolated values encoded) — rendered raw.
    pub why: String,
    /// Always None from a scan (measured by `worktrees_size`).
    pub size_bytes: Option<i64>,
    /// max(index mtime, branch committerdate); None when unknown (C# DateTime.MinValue).
    pub last_activity: Option<Time>,
    pub locked: bool,
    pub lock_reason: Option<String>,
    pub dirty: i32,
    pub unpushed: i32,
    pub merged: bool,
    pub squash_merged: bool,
    pub upstream_gone: bool,
    pub branch_only: bool,
    pub unreadable: Option<String>,
    /// Remote ref holding every commit, confirmed live on origin.
    pub pushed: Option<String>,
    pub recent_min: Option<i32>,
    /// Ignored files that are not build output ("a, b +3").
    pub kept: Option<String>,
}

impl WorktreeInfo {
    /// merged || squash_merged ? 0 : unpushed.
    pub fn at_risk(&self) -> i32 {
        todo!("port: WorktreeInfo.AtRisk")
    }
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct CleanupStep {
    pub repo: String,
    /// "git " + args, repo-prefixed args shown relative.
    pub display: String,
    pub git_args: Vec<String>,
    pub failure_is_fatal: bool,
    /// "Delete anyway" acknowledged for this path's kept files.
    pub kept_ok: bool,
}

/// Input of `classify` (all default 0/false/None except exists = true).
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct WtFacts {
    pub dirty: i32,
    pub ahead: i32,
    pub merged: bool,
    pub squash_merged: bool,
    pub upstream_gone: bool,
    pub locked: bool,
    pub lock_pid: Option<i32>,
    pub pid_alive: bool,
    pub exists: bool,
    pub prunable: bool,
    pub active_session_name: Option<String>,
    pub detached: bool,
    pub no_worktree: bool,
    pub unreadable: Option<String>,
    pub on_remote: Option<String>,
    pub in_use_by: Option<String>,
    pub recent_min: Option<i32>,
    pub twin_of: Option<String>,
    pub twin_active: bool,
    pub kept: Option<String>,
    pub on_remote_stale: Option<String>,
    pub remote_gone: Option<String>,
    pub merged_pr: Option<i32>,
    pub dirty_names: Option<String>,
}

impl Default for WtFacts {
    fn default() -> Self {
        WtFacts {
            dirty: 0,
            ahead: 0,
            merged: false,
            squash_merged: false,
            upstream_gone: false,
            locked: false,
            lock_pid: None,
            pid_alive: false,
            exists: true,
            prunable: false,
            active_session_name: None,
            detached: false,
            no_worktree: false,
            unreadable: None,
            on_remote: None,
            in_use_by: None,
            recent_min: None,
            twin_of: None,
            twin_active: false,
            kept: None,
            on_remote_stale: None,
            remote_gone: None,
            merged_pr: None,
            dirty_names: None,
        }
    }
}

/// `(state, why_html)` with the recency wrapper (spec 04 §1.8). `{base}` stays literal in the why.
pub fn classify(f: &WtFacts) -> (WtState, String) {
    todo!("port: WorktreeService.Classify")
}

/// The 24-rule core, first match wins.
pub fn classify_core(f: &WtFacts) -> (WtState, String) {
    todo!("port: WorktreeService.Core")
}

/// Rebuild facts from a row (exact quirks of spec 04 §1.9).
pub fn facts_of(w: &WorktreeInfo) -> WtFacts {
    todo!("port: WorktreeService.FactsOf")
}

/// Safe/Orphan, or a stale lock that classifies Safe unlocked, or (kept_ok) Check held only by kept files.
pub fn cleanable(w: &WorktreeInfo, kept_ok: bool) -> bool {
    todo!("port: WorktreeService.Cleanable")
}

/// Non-destructive git steps for the cleanable rows, grouped by repo (spec 04 §1.10). `kept_ok` = acknowledged paths.
pub fn plan(rows: &[WorktreeInfo], kept_ok: &[String]) -> Vec<CleanupStep> {
    todo!("port: WorktreeService.Plan")
}

/// branch ?? name.
pub fn display_name(w: &WorktreeInfo) -> &str {
    todo!("port: WorktreeService.DisplayName")
}

/// `"wt-" + sha1_hex(path)[..12]` (stable DOM id).
pub fn anchor(path: &str) -> String {
    todo!("port: WorktreeService.Anchor")
}

/// `Wt.State.{Safe,Orphan,Active,Check}`.
pub fn label(state: WtState) -> String {
    todo!("port: WorktreeService.Label")
}

/// Basenames (rename → right side), deduped; first 2 joined ", " + " +N".
pub fn names_of(paths: &[String]) -> Option<String> {
    todo!("port: WorktreeService.NamesOf")
}

/// Ignored entries that are not regenerable build output (`ls-files --others --ignored --directory` output).
pub fn kept_of(ls: &str) -> Vec<String> {
    todo!("port: WorktreeService.KeptOf")
}

/// "a, b" / "a, b +N" / None.
pub fn kept_summary(kept: &[String]) -> Option<String> {
    todo!("port: WorktreeService.KeptIgnored (format)")
}

/// `lsof -Fpcn` output → `(pid, name, path)` users (not self, not lsof/git, absolute paths).
pub fn parse_lsof(o: &str, self_pid: u32) -> Vec<(u32, String, String)> {
    todo!("port: WorktreeService.ParseLsof")
}

/// `worktree-*` branches no worktree checks out (case-insensitive), sorted ordinally.
pub fn orphan_branches(branches: &[String], checked_out: &[String]) -> Vec<String> {
    todo!("port: WorktreeService.OrphanBranches")
}

/// `pid (\d+)` in a lock reason.
pub fn pid_of(reason: Option<&str>) -> Option<i32> {
    todo!("port: WorktreeService.PidOf")
}

/// Is a step forbidden (`--force`, `-f`, `-D`, `--delete-force`)?
pub fn forbidden(args: &[String]) -> bool {
    todo!("port: WorktreeService.RunAsync (guard)")
}
