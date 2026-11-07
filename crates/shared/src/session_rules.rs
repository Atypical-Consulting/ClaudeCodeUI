//! Port of the pure helpers of `Core/LiveSession.cs` both sides need — spec 01 §2.4 (constants), §6.2, §6.4, §6.5.
//! Owner: core-live-session. The reducer and commands are `ccui_core::live`.

use crate::model::{HookEvt, Item, PendingPermission, ToolItem};
use crate::session::{SessionMeta, SessionsSummary};
use serde_json::Value;
use std::borrow::Borrow;
use std::collections::HashMap;

/// Modes offered by the UI (bypassPermissions / dontAsk deliberately not offered).
pub const MODES: [&str; 4] = ["default", "acceptEdits", "plan", "auto"];
/// Protocol sentinel for an interrupted turn (never displayed).
pub const INTERRUPTED: &str = "interrupted";
pub const PUSH_TOOL: &str = "PushNotification";
pub const CONFIG_USAGE: &str = "Usage: /config key=value";

/// auto | acceptEdits: the UI confirms these when not isolated.
pub fn risky(mode: &str) -> bool {
    todo!("port: LiveSession.Risky")
}

/// The text a user line shows as a prompt: None for interrupts, local-command echoes, task notifications and
/// subagent hand-backs; `<command-name>/x</command-name><command-args>a</command-args>` → "/x a"; else t.
pub fn user_text(t: &str) -> Option<String> {
    todo!("port: LiveSession.UserText")
}

/// First line that is non-empty after trimming `" ' \` * .` → `text::trunc`.
pub fn title_from(output: &str) -> Option<String> {
    todo!("port: LiveSession.TitleFrom")
}

/// `--add-dir` candidate: trimmed, unquoted, trailing separator dropped ("/" stays); None when empty, equal to cwd or
/// already listed. `case_sensitive` = Linux.
pub fn extra_dir(raw: &str, cwd: &str, dirs: &[String], case_sensitive: bool) -> Option<String> {
    todo!("port: LiveSession.ExtraDir")
}

/// `effective.maxEffortLevel`, overridden by the first `modelSettings` key the model starts with (case-insensitive);
/// "max" → None (exempt).
pub fn effort_cap(effective: &Value, model: Option<&str>) -> Option<String> {
    todo!("port: LiveSession.EffortCap")
}

/// Levels up to and including cap when cap ∈ levels, else all.
pub fn capped<'a>(levels: &[&'a str], cap: Option<&str>) -> Vec<&'a str> {
    todo!("port: LiveSession.Capped")
}

/// `/config` usage text → `(key, allowed values)` (value starting with '<' → free text = []); else None.
pub fn config_keys(text: &str) -> Option<Vec<(String, Vec<String>)>> {
    todo!("port: LiveSession.ConfigKeys")
}

/// EnterWorktree → worktreePath; ExitWorktree → restoredCwd ?? originalCwd; else None.
pub fn worktree_move(tool: &str, structured: Option<&Value>) -> Option<String> {
    todo!("port: LiveSession.WorktreeMove")
}

/// Fork history: up to and including the TextItem with uuid `at`, else all.
pub fn upto(items: Vec<Item>, at: Option<&str>) -> Vec<Item> {
    todo!("port: LiveSession.Upto")
}

// Item-slice helpers are generic over `I: Borrow<Item>` so the backend passes `&[Item]` and the UI passes the
// `&[&Item]` of its per-row signals (`SessionHandle::with_items`) without cloning.

/// uuids of the target and every UserItem after it, newest first (rewind_conversation takes only the latest).
pub fn rewind_steps<I: Borrow<Item>>(items: &[I], uuid: &str) -> Vec<String> {
    todo!("port: LiveSession.RewindSteps")
}

/// `CanRewind(u)`: u.uuid ∈ meta.sent && has_process && Idle && no running background ToolItem.
pub fn can_rewind<I: Borrow<Item>>(meta: &SessionMeta, items: &[I], uuid: Option<&str>) -> bool {
    todo!("port: LiveSession.CanRewind")
}

/// Last ToolItem with that id.
pub fn tool<'a, I: Borrow<Item>>(items: &'a [I], id: &str) -> Option<&'a ToolItem> {
    todo!("port: LiveSession.Tool")
}

/// parent_tool_use_id → indexes of its Tool/Text items, in order (`Children`).
pub fn children<I: Borrow<Item>>(items: &[I]) -> HashMap<String, Vec<usize>> {
    todo!("port: LiveSession.Children")
}

/// `AskingAgent(p)`: last ToolItem with task_id == info.agent_id, else the parent tool of tool(p.tool_use_id).
pub fn asking_agent<'a, I: Borrow<Item>>(items: &'a [I], p: &PendingPermission) -> Option<&'a ToolItem> {
    todo!("port: LiveSession.AskingAgent")
}

/// `PermissionCard.AgentLabel(a)`: input.description ?? input.subagent_type ?? name. Used by the PermissionCard and
/// by the backend for `DecisionEntry.agent_label`.
pub fn agent_label(agent: &ToolItem) -> String {
    todo!("port: PermissionCard.AgentLabel")
}

/// Hook shown in the thread: outcome != success, or output non-empty and not SessionStart*/SubagentStart* and not
/// `hooks::context_only`.
pub fn shown(hook: &HookEvt) -> bool {
    todo!("port: LiveSession.Shown")
}

/// `SessionManager.RunningElsewhere(id)`: no live session with a process for id, and id held by some claude.
pub fn running_elsewhere(summary: &SessionsSummary, id: &str) -> bool {
    todo!("port: SessionManager.RunningElsewhere")
}
