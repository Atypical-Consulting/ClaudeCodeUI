//! CONTRACT — argument / result types of the Tauri commands that are not domain records of another module.
//! Command list and signatures: docs/ARCHITECTURE.md §Commands (desktop/src-tauri/src/commands.rs is the source).
//! Ports records of `Core/TranscriptStore.cs` (PastSession, TranscriptHit, Ending, Stop — spec 03 §1.1),
//! `Core/Bookmarks.cs`, `Core/AppFlag.cs` (names + allowed values — spec 03 §9), `SessionManager.Start` args.
//!
//! Owner: tauri-glue.

use crate::edit_diff::FileDiff;
use crate::model::*;
use crate::worktree_rules::WorktreeInfo;
use rust_decimal::Decimal;
use serde::{Deserialize, Serialize};
use serde_json::Value;

// ----------------------------------------------------------------------------------------------- boot / prefs

/// AppPref file names (`<AppData>/ClaudeCodeUI/<name>`, plain text).
pub const PREF_THEME: &str = "theme";
pub const PREF_CODE_SIZE: &str = "code-size";
pub const PREF_RAIL_WIDTH: &str = "rail-width";
pub const PREF_INSPECTOR_WIDTH: &str = "inspector-width";
/// Persisted UI culture file (replaces the `.AspNetCore.Culture` cookie): `en` | `fr`. NOT an AppPref: written only
/// by `set_culture` (which also switches the backend's i18n language), so `valid_pref` refuses it.
pub const PREF_CULTURE: &str = "culture";

/// localStorage key of the UI's boot mirror (`{culture, prefs, flags}` JSON). The shell's initialization script is
/// built once per webview, so after `set_culture` + `location.reload()` (spec 06 §6.4) or any reload it would
/// re-inject stale values: the script overlays this mirror on the injected `BootInfo` before applying theme / sizes
/// / lang. The UI rewrites it on every `PrefChanged`, `set_culture` and at boot (`ipc::save_boot_mirror`).
pub const BOOT_MIRROR_KEY: &str = "ccui.boot";

pub const THEMES: [&str; 5] = ["graphite", "encre", "ristretto", "mousse", "contraste"];
pub const CODE_SIZES: [&str; 4] = ["12px", "13px", "14px", "15px"];
/// `rail-width`: every `Npx` for N in this range; `inspector-width` likewise.
pub const RAIL_WIDTHS: std::ops::RangeInclusive<u32> = 200..=400;
pub const INSPECTOR_WIDTHS: std::ops::RangeInclusive<u32> = 320..=720;

/// AppFlag file names (empty file; existence = on).
pub const FLAG_THINKING: &str = "thinking-summaries";
pub const FLAG_TIMESTAMPS: &str = "timestamps";
pub const FLAG_NOTIFY: &str = "notify-on";

/// Whether `value` is allowed for pref `name` (whitelist; a hand-edited file outside it reads as None).
/// Names: theme, code-size, rail-width, inspector-width only (`culture` goes through `set_culture`).
pub fn valid_pref(name: &str, value: &str) -> bool {
    let px = |range: std::ops::RangeInclusive<u32>| {
        value.strip_suffix("px").and_then(|n| n.parse::<u32>().ok()).is_some_and(|n| {
            range.contains(&n) && n.to_string().len() + 2 == value.len()
        })
    };
    match name {
        PREF_THEME => THEMES.contains(&value),
        PREF_CODE_SIZE => CODE_SIZES.contains(&value),
        PREF_RAIL_WIDTH => px(RAIL_WIDTHS),
        PREF_INSPECTOR_WIDTH => px(INSPECTOR_WIDTHS),
        _ => false,
    }
}

#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
pub struct Prefs {
    pub theme: Option<String>,
    pub code_size: Option<String>,
    pub rail_width: Option<String>,
    pub inspector_width: Option<String>,
}

#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
pub struct Flags {
    pub thinking: bool,
    pub timestamps: bool,
    pub notify: bool,
}

/// Everything the UI needs synchronously before mounting (decision 7). Injected by the shell as
/// `window.__CCUI_BOOT__` through the window's initialization script (which also applies theme / code size /
/// pane widths to `<html>` before first paint); `app_boot` returns the same for a reload.
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
pub struct BootInfo {
    /// `en` | `fr` (persisted choice, else OS language).
    pub culture: String,
    pub mac: bool,
    pub version: String,
    /// User home (Fmt.Home / TranscriptStore.Short on the UI side), native separators.
    pub home: String,
    /// Native path separator ("/" or "\\").
    pub sep: String,
    pub prefs: Prefs,
    pub flags: Flags,
}

// ----------------------------------------------------------------------------------------------- sessions

/// `SessionManager.Start` arguments (NewSession page).
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
pub struct StartArgs {
    pub cwd: String,
    /// "" = no --permission-mode.
    pub mode: String,
    pub name: String,
    pub worktree: Option<String>,
    pub model: Option<String>,
    pub effort: Option<String>,
    pub add_dirs: Vec<String>,
    pub fallback: Option<String>,
    pub restricted: bool,
    pub safe_mode: bool,
    pub agent: Option<String>,
}

/// `LiveSession.Answer` arguments.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct AnswerArgs {
    pub request_id: String,
    pub decision: Decision,
    pub mode: Option<String>,
    pub message: Option<String>,
    /// Updated input (AskUserQuestion answers, elicitation content).
    pub input: Option<Value>,
    /// session | localSettings | projectSettings | userSettings (default session).
    pub destination: Option<String>,
}

/// `set_cwd` answered `needs_trust`: ask the user, then call again with `trusted = Some(directory)`.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct TrustPrompt {
    pub directory: String,
    /// Inside another git root: accepting trusts that whole repo (the prompt must name it).
    pub root: Option<String>,
}

/// A `claude <subcommand>` run (`Plugins.Run`).
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
pub struct CliOutput {
    pub code: i32,
    pub stdout: String,
    pub stderr: String,
}

/// `ClaudeSession.OnPath()` / `NpmShimOnly()`.
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
pub struct ClaudeFound {
    pub found: bool,
    pub npm_shim_only: bool,
}

/// `WorkflowRuns.AgentLog` read for the inspector (`workflow_agent_log`).
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
pub struct WorkflowAgentLog {
    /// Markdown of the agent's last TextItem.
    pub text: Option<String>,
    /// Its top-level ToolItems.
    pub tools: Vec<ToolItem>,
}

// ----------------------------------------------------------------------------------------------- transcripts

/// `TranscriptStore.PastSession`.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct PastSession {
    pub id: String,
    pub cwd: String,
    pub branch: Option<String>,
    pub title: String,
    pub cost_usd: Option<Decimal>,
    /// File mtime.
    pub last_write: Time,
    pub worktree_path: Option<String>,
    pub mode: Option<String>,
    pub pr_url: Option<String>,
}

/// One Ctrl K message hit.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct TranscriptHit {
    pub session: PastSession,
    pub snippet: String,
    pub at: Time,
}

/// How a conversation ended.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub enum Stop {
    #[default]
    Done,
    Asked,
    Cut,
}

/// `TranscriptStore.Ending` (Overview cards).
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
pub struct Ending {
    pub text: Option<String>,
    pub stop: Stop,
    pub tool: Option<String>,
    pub target: Option<String>,
    pub unanswered: bool,
    pub interrupted: bool,
}

/// `recent_sessions` answer: the generation read with the list (reload when a push announces a newer one).
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
pub struct RecentList {
    pub generation: u64,
    pub list: Vec<PastSession>,
}

/// `search_transcripts` answer. complete = every transcript was read (else budget/take/cancel stopped it).
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
pub struct SearchResult {
    pub hits: Vec<TranscriptHit>,
    pub complete: bool,
}

/// `SessionManager.LastLimits()` (limits.json): the idle Overview shows it, no process to ask.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct SavedLimits {
    pub limits: RateLimits,
    pub at: Time,
}

/// `Bookmarks.Bookmark`.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct Bookmark {
    pub session: String,
    pub uuid: String,
    pub gist: String,
    pub at: Time,
}

// ----------------------------------------------------------------------------------------------- files

/// `FileIndex.Index` (paths relative to cwd, `/`-separated, folders end with `/`).
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
pub struct FileIndexResult {
    pub paths: Vec<String>,
    pub truncated: bool,
}

/// `/diff` panel data (`git_diff`).
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
pub struct GitDiff {
    /// Repo root the diff was taken in; None = not a git folder.
    pub root: Option<String>,
    pub files: Vec<FileDiff>,
}

// ----------------------------------------------------------------------------------------------- worktrees

/// The last full scan (`WorktreeService.Cached()` + `CachedAt`).
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
pub struct WorktreeCache {
    pub rows: Vec<WorktreeInfo>,
    pub cached_at: Option<Time>,
}

/// One executed cleanup step (`RunAsync` yield), pushed as `Push::WorktreeRunStep`.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct WorktreeRunStep {
    pub run_id: u64,
    pub index: usize,
    pub repo: String,
    pub display: String,
    /// -1 = refused / skipped by the runner (output says why).
    pub exit: i32,
    pub output: String,
}

// ----------------------------------------------------------------------------------------------- background agents

/// `BackgroundAgents.List()`.
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
pub struct BgList {
    pub rows: Vec<crate::bg_agents::Agent>,
    pub daemon_up: bool,
}

/// `BackgroundAgents.Run(command, id)`.
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
pub struct BgRunResult {
    pub ok: bool,
    pub text: String,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn prefs_whitelist() {
        assert!(valid_pref(PREF_THEME, "mousse") && !valid_pref(PREF_THEME, "Mousse"));
        assert!(valid_pref(PREF_CODE_SIZE, "13px") && !valid_pref(PREF_CODE_SIZE, "16px"));
        assert!(valid_pref(PREF_RAIL_WIDTH, "200px") && valid_pref(PREF_RAIL_WIDTH, "400px"));
        assert!(!valid_pref(PREF_RAIL_WIDTH, "199px") && !valid_pref(PREF_RAIL_WIDTH, "0300px"));
        assert!(!valid_pref(PREF_RAIL_WIDTH, "+300px") && !valid_pref(PREF_RAIL_WIDTH, "300"));
        assert!(valid_pref(PREF_INSPECTOR_WIDTH, "720px") && !valid_pref(PREF_INSPECTOR_WIDTH, "721px"));
        assert!(!valid_pref(PREF_CULTURE, "fr") && !valid_pref("x", "a"));
    }
}
