//! Port of `Plugins.Run` — the generic "run a claude subcommand" primitive shared by Plugins, CliEdit,
//! BackgroundAgents, StatusPanel (`claude auth logout`) and the auto-title — spec 05 §1 "Run process contract".
//! Owner: core-extensions.
//!
//! `claude <command> <args…>` (same PATH resolution as the session spawner, argv list, no shell), cwd given, stdin a
//! pipe written then closed (never a TTY: confirmations are refused, not awaited), UTF-8 without BOM, stdout/stderr
//! drained concurrently, env minus `CLAUDECODE` and `CLAUDE_CODE_CHILD_SESSION`, no window on Windows, timeout
//! (default 5 min) kills the whole tree → Err("claude <command> <args…>").

use crate::CoreResult;
use ccui_shared::api::CliOutput;
use std::time::Duration;

pub const DEFAULT_TIMEOUT: Duration = Duration::from_secs(300);

pub async fn run_claude(cwd: &str, command: &str, args: &[String], stdin: Option<&str>, timeout: Duration) -> CoreResult<CliOutput> {
    todo!("port: Plugins.Run")
}

/// StatusPanel "Sign out": `claude auth logout` in cwd.
pub async fn auth_logout(cwd: &str) -> CoreResult<CliOutput> {
    todo!("port: StatusPanel.razor SignOut")
}
