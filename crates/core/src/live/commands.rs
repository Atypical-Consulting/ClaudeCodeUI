//! Port of the user commands of `Core/LiveSession.cs` — spec 01 §6 (table), §6.1-6.6. Owner: core-live-session.
//! Each maps to one Tauri command `session_*` (desktop/src-tauri/src/commands.rs). Requests go through
//! `ensure_process()` (spawn if needed) unless the doc says "if proc" (then a missing process is a no-op / None).

use super::LiveSession;
use crate::CoreResult;
use ccui_shared::api::{AnswerArgs, TrustPrompt};
use ccui_shared::hooks::HookListing;
use ccui_shared::model::{McpAuthStart, ReloadSummary, RewindPreview, UserImage};
use ccui_shared::permission_rules::PermissionRulesState;
use serde_json::Value;
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::time::Duration;

/// TitlePrompt, exact (auto-title via Haiku).
pub const TITLE_PROMPT: &str = "Write a 3-7 word title for a coding session that starts with the user's request. Same language as the request. Keep issue/PR numbers. Reply with the title only, no quotes, no trailing period.";

impl LiveSession {
    /// Generic control request through ensure_process (default timeout 15 s).
    pub async fn request(self: &Arc<Self>, subtype: &str, fields: Value, timeout: Option<Duration>) -> CoreResult<Value> {
        todo!("port: LiveSession.Request")
    }

    /// §6.1: spawn if needed, queue mid-turn (chip) or begin the turn, write the user message; a failed write of a
    /// queued message removes its chip and errors (the Composer puts the text back). Returns the message uuid.
    pub async fn send(self: &Arc<Self>, text: String, images: Vec<UserImage>) -> CoreResult<String> {
        todo!("port: LiveSession.Send")
    }

    /// `interrupt {send_now:true, message_uuid}`: stop the turn and start that queued message at once.
    pub async fn send_now(self: &Arc<Self>, uuid: &str) -> CoreResult<()> {
        todo!("port: LiveSession.SendNow")
    }

    /// `cancel_async_message {message_uuid}` (the `cancelled` lifecycle frame then removes the chip).
    pub async fn cancel(self: &Arc<Self>, uuid: &str) -> CoreResult<()> {
        todo!("port: LiveSession.Cancel")
    }

    /// `rewind_files {user_message_id, dry_run:true}`.
    pub async fn preview_rewind(self: &Arc<Self>, uuid: &str) -> CoreResult<RewindPreview> {
        todo!("port: LiveSession.PreviewRewind")
    }

    /// §6.2: rewind_conversation newest → target one step at a time, cut the thread in `finally`, then rewind_files.
    pub async fn rewind(self: &Arc<Self>, uuid: &str, files: bool) -> CoreResult<()> {
        todo!("port: LiveSession.Rewind")
    }

    /// §6.3: AskUser/Elicit form needs input (checked BEFORE NoProcess); double answers no-op; setMode suggestion
    /// with AllowSession; destination rewrite; resolve.
    pub async fn answer(self: &Arc<Self>, args: AnswerArgs) -> CoreResult<()> {
        todo!("port: LiveSession.Answer")
    }

    /// If proc: `background_tasks {tool_use_id}` (Ctrl+B).
    pub async fn background(self: &Arc<Self>, tool_use_id: &str) -> CoreResult<()> {
        todo!("port: LiveSession.Background")
    }

    /// If proc: `interrupt`.
    pub async fn interrupt(self: &Arc<Self>) -> CoreResult<()> {
        todo!("port: LiveSession.Interrupt")
    }

    /// If proc: `set_permission_mode {mode}`; then mode = m.
    pub async fn set_mode(self: &Arc<Self>, mode: &str) -> CoreResult<()> {
        todo!("port: LiveSession.SetMode")
    }

    /// `set_model {model}`; model = m.
    pub async fn set_model(self: &Arc<Self>, model: &str) -> CoreResult<()> {
        todo!("port: LiveSession.SetModel")
    }

    /// `apply_flag_settings {settings:{effortLevel}}`; effort = e.
    pub async fn set_effort(self: &Arc<Self>, effort: &str) -> CoreResult<()> {
        todo!("port: LiveSession.SetEffort")
    }

    /// `set_max_thinking_tokens {max_thinking_tokens: on ? null : 0}`; thinking = on.
    pub async fn set_thinking(self: &Arc<Self>, on: bool) -> CoreResult<()> {
        todo!("port: LiveSession.SetThinking")
    }

    /// `apply_flag_settings {settings:{advisorModel}}` then get_settings → advisor (the CLI may drop it).
    pub async fn set_advisor(self: &Arc<Self>, model: Option<String>) -> CoreResult<()> {
        todo!("port: LiveSession.SetAdvisor")
    }

    /// `update_settings {source:"localSettings", settings:{outputStyle}}`.
    pub async fn set_output_style(self: &Arc<Self>, style: &str) -> CoreResult<()> {
        todo!("port: LiveSession.SetOutputStyle")
    }

    /// `UserSettings::set_model` + userSettings effortLevel unless "max" (session-only).
    pub async fn set_default(self: &Arc<Self>, model: &str, effort: Option<String>) -> CoreResult<()> {
        todo!("port: LiveSession.SetDefault")
    }

    /// `apply_flag_settings {settings:{fastMode}}`.
    pub async fn set_fast(self: &Arc<Self>, on: bool) -> CoreResult<()> {
        todo!("port: LiveSession.SetFast")
    }

    /// Local only: ultracode = on, notify.
    pub fn set_ultracode(&self, on: bool) {
        todo!("port: LiveSession.SetUltracode")
    }

    /// Composer unsent text (survives navigation/reload); no notify loop back to the sender is needed beyond the patch.
    pub fn set_draft(&self, text: String, images: Vec<UserImage>) {
        todo!("port: LiveSession.Draft / DraftImages setters")
    }

    /// §6.4: validate (`session_rules::extra_dir`), apply the whole list live, record `dirs/<id>.txt`.
    pub async fn add_dir(self: &Arc<Self>, dir: &str) -> CoreResult<()> {
        todo!("port: LiveSession.AddDir")
    }

    /// `get_context_usage` → context.
    pub async fn refresh_context(self: &Arc<Self>) -> CoreResult<()> {
        todo!("port: LiveSession.RefreshContext")
    }

    /// `mcp_status` → mcp_status.
    pub async fn refresh_mcp(self: &Arc<Self>) -> CoreResult<()> {
        todo!("port: LiveSession.RefreshMcp")
    }

    /// If proc: `get_memory_dialog` → memory_dialog.
    pub async fn refresh_memory(self: &Arc<Self>) -> CoreResult<()> {
        todo!("port: LiveSession.RefreshMemory")
    }

    /// `claude edit-memory-settings --json` with `{"autoMemoryEnabled":on}` (CliEdit), then refresh_memory.
    pub async fn set_auto_memory(self: &Arc<Self>, on: bool) -> CoreResult<()> {
        todo!("port: LiveSession.SetAutoMemory")
    }

    /// If proc: `get_usage` (skip_behaviors unless behaviors), 60 s → usage.
    pub async fn refresh_usage(self: &Arc<Self>, behaviors: bool) -> CoreResult<()> {
        todo!("port: LiveSession.RefreshUsage")
    }

    /// If proc: `get_status` (30 s) → status_rows.
    pub async fn refresh_status(self: &Arc<Self>) -> CoreResult<()> {
        todo!("port: LiveSession.RefreshStatus")
    }

    /// `reload_plugins` then `reload_skills`; reducer `reloaded`; counts. A process that exited is NOT respawned.
    pub async fn reload_plugins(self: &Arc<Self>) -> CoreResult<ReloadSummary> {
        todo!("port: LiveSession.ReloadPlugins")
    }

    /// If proc: `get_hooks_listing` → `events::hooks` (the hooks tab only reads; never spawns).
    pub async fn hooks_listing(self: &Arc<Self>) -> CoreResult<Option<HookListing>> {
        todo!("port: Extensions.LoadHooks (request part)")
    }

    /// If proc: `list_permission_rules` → `permission_rules::parse`.
    pub async fn permission_rules(self: &Arc<Self>) -> CoreResult<Option<PermissionRulesState>> {
        todo!("port: Extensions.LoadPerms (request part)")
    }

    /// `mcp_clear_auth` then `mcp_authenticate` → McpAuthStart.
    pub async fn sign_in_again(self: &Arc<Self>, server: &str) -> CoreResult<McpAuthStart> {
        todo!("port: LiveSession.SignInAgain")
    }

    /// `set_cwd {path [, trust_accepted, trusted_directory]}`: needs_trust → Some(prompt); ok → cwd moved (worktree
    /// dropped, git refreshed) → None; else Err(message ?? reason ?? raw). Refused while a turn runs.
    pub async fn set_cwd(self: &Arc<Self>, path: &str, trusted: Option<String>) -> CoreResult<Option<TrustPrompt>> {
        todo!("port: LiveSession.SetCwd")
    }

    /// `rename_session {title: trimmed, source:"host"}` (session_title_changed carries the cleaned name).
    pub async fn rename(self: &Arc<Self>, title: &str) -> CoreResult<()> {
        todo!("port: LiveSession.Rename")
    }

    /// `export_conversation` → text written to `free_file(downloads, default_filename)`; returns the path.
    pub async fn export(self: &Arc<Self>) -> CoreResult<String> {
        todo!("port: LiveSession.Export")
    }

    /// `remote_control {enabled}` → remote_url (absolute https only).
    pub async fn set_remote(self: &Arc<Self>, on: bool) -> CoreResult<()> {
        todo!("port: LiveSession.SetRemote")
    }

    /// `claude_authenticate {loginWithClaudeAi:true}` → automaticUrl if absolute https.
    pub async fn start_sign_in(self: &Arc<Self>) -> CoreResult<Option<String>> {
        todo!("port: LiveSession.StartSignIn")
    }

    /// `claude_oauth_wait_for_completion` (600 s) → initialize_info.account; then refresh_status.
    pub async fn finish_sign_in(self: &Arc<Self>) -> CoreResult<()> {
        todo!("port: LiveSession.FinishSignIn")
    }

    /// If proc: `get_task_output {task_id}` → output ?? ""; else None.
    pub async fn task_output(self: &Arc<Self>, task_id: &str) -> CoreResult<Option<String>> {
        todo!("port: LiveSession.TaskOutput")
    }

    /// If proc: `stop_task {task_id}` (task_updated then reports "killed").
    pub async fn stop_task(self: &Arc<Self>, task_id: &str) -> CoreResult<()> {
        todo!("port: LiveSession.StopTask")
    }

    /// UI-only: drop non-running tasks; notify.
    pub fn clear_ended_tasks(&self) {
        todo!("port: LiveSession.ClearEndedTasks")
    }

    /// §6.6: one-shot Haiku title (cwd = temp dir, 5 min); user rename wins; failure → trunc(prompt).
    pub(crate) async fn auto_title(self: Arc<Self>, prompt: String) {
        todo!("port: LiveSession.AutoTitle")
    }
}

/// Bare file name in `dir`, numbered `stem-2.ext`, `-3`… while taken; invalid/empty → `conversation.txt`.
pub fn free_file(dir: &Path, name: Option<&str>) -> PathBuf {
    todo!("port: LiveSession.FreeFile")
}
