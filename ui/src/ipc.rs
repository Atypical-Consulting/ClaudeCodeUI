//! THE typed binding to the Tauri backend: one async fn per command of desktop/src-tauri/src/commands.rs (same
//! name, snake_case argument keys), `listen_push` for the single push event, `boot_info` for the data injected
//! before first paint. IMPLEMENTED. Owner: tauri-glue — change together with commands.rs.
//!
//! Transport: `window.__TAURI__.core.invoke` / `window.__TAURI__.event.listen` (withGlobalTauri is on);
//! values cross as JSON-compatible JS (serde-wasm-bindgen `json_compatible`). Errors are the backend's `String`.

use ccui_shared::api::*;
use ccui_shared::edit_diff::DiffLine;
use ccui_shared::hooks::HookListing;
use ccui_shared::memory::{AutoMemory, MemoryFile, MemorySave};
use ccui_shared::model::{McpAuthStart, ReloadSummary, RewindPreview, UserImage};
use ccui_shared::permission_rules::PermissionRulesState;
use ccui_shared::plugins::{Market, Outcome, PluginList, PluginOption};
use ccui_shared::push::{Push, PUSH_EVENT};
use ccui_shared::session::{SessionSnapshot, SessionsSummary};
use ccui_shared::workflow::WorkflowRun;
use ccui_shared::worktree_rules::{CleanupStep, WorktreeInfo};
use ccui_shared::Time;
use serde::de::DeserializeOwned;
use serde::Serialize;
use serde_json::{json, Map, Value};
use wasm_bindgen::prelude::*;
use wasm_bindgen::JsCast;

#[wasm_bindgen]
extern "C" {
    #[wasm_bindgen(js_namespace = ["window", "__TAURI__", "core"], js_name = invoke, catch)]
    async fn tauri_invoke(cmd: &str, args: JsValue) -> Result<JsValue, JsValue>;

    #[wasm_bindgen(js_namespace = ["window", "__TAURI__", "event"], js_name = listen, catch)]
    async fn tauri_listen(event: &str, handler: &js_sys::Function) -> Result<JsValue, JsValue>;
}

pub type IpcResult<T> = Result<T, String>;

/// Invoke one command with a JSON object of arguments.
pub async fn call<R: DeserializeOwned>(cmd: &str, args: Value) -> IpcResult<R> {
    let js_args = args.serialize(&serde_wasm_bindgen::Serializer::json_compatible()).map_err(|e| e.to_string())?;
    match tauri_invoke(cmd, js_args).await {
        Ok(v) => serde_wasm_bindgen::from_value(v).map_err(|e| e.to_string()),
        Err(e) => Err(e.as_string().unwrap_or_else(|| format!("{e:?}"))),
    }
}

/// Subscribe to every backend push for the life of the page (call once, at boot).
pub fn listen_push(handler: impl Fn(Push) + 'static) {
    let cb = Closure::<dyn FnMut(JsValue)>::new(move |ev: JsValue| {
        let payload = js_sys::Reflect::get(&ev, &JsValue::from_str("payload")).unwrap_or(JsValue::NULL);
        match serde_wasm_bindgen::from_value::<Push>(payload) {
            Ok(p) => handler(p),
            Err(e) => web_sys::console::error_1(&JsValue::from_str(&format!("push: {e}"))),
        }
    });
    let f: js_sys::Function = cb.as_ref().unchecked_ref::<js_sys::Function>().clone();
    cb.forget(); // lives as long as the page
    wasm_bindgen_futures::spawn_local(async move {
        let _ = tauri_listen(PUSH_EVENT, &f).await;
    });
}

/// `window.__CCUI_BOOT__` (set by the shell's initialization script), else defaults (plain `trunk serve`).
pub fn boot_info() -> BootInfo {
    web_sys::window()
        .and_then(|w| js_sys::Reflect::get(&w, &JsValue::from_str("__CCUI_BOOT__")).ok())
        .filter(|v| !v.is_undefined() && !v.is_null())
        .and_then(|v| serde_wasm_bindgen::from_value(v).ok())
        .unwrap_or_default()
}

// ============================================================================================ app / prefs

pub async fn app_boot() -> IpcResult<BootInfo> {
    call("app_boot", json!({})).await
}
pub async fn app_version() -> IpcResult<String> {
    call("app_version", json!({})).await
}
pub async fn check_for_updates() -> IpcResult<()> {
    call("check_for_updates", json!({})).await
}
pub async fn app_pref_set(name: &str, value: Option<&str>) -> IpcResult<()> {
    call("app_pref_set", json!({"name": name, "value": value})).await
}
pub async fn app_flag_set(name: &str, on: bool) -> IpcResult<()> {
    call("app_flag_set", json!({"name": name, "on": on})).await
}
pub async fn set_culture(culture: &str) -> IpcResult<()> {
    call("set_culture", json!({"culture": culture})).await
}
pub async fn open_url(url: &str) -> IpcResult<()> {
    call("open_url", json!({"url": url})).await
}

// ============================================================================================ sessions (manager)

pub async fn sessions_summary() -> IpcResult<SessionsSummary> {
    call("sessions_summary", json!({})).await
}
pub async fn session_snapshot(id: &str) -> IpcResult<Option<SessionSnapshot>> {
    call("session_snapshot", json!({"id": id})).await
}
pub async fn session_open(id: &str) -> IpcResult<Option<SessionSnapshot>> {
    call("session_open", json!({"id": id})).await
}
pub async fn session_start(args: &StartArgs) -> IpcResult<String> {
    call("session_start", json!({"args": args})).await
}
pub async fn session_stop(id: &str) -> IpcResult<()> {
    call("session_stop", json!({"id": id})).await
}
pub async fn session_fork(id: &str, at: Option<&str>) -> IpcResult<String> {
    call("session_fork", json!({"id": id, "at": at})).await
}

// ============================================================================================ session commands

pub async fn session_send(id: &str, text: &str, images: &[UserImage]) -> IpcResult<String> {
    call("session_send", json!({"id": id, "text": text, "images": images})).await
}
pub async fn session_send_now(id: &str, uuid: &str) -> IpcResult<()> {
    call("session_send_now", json!({"id": id, "uuid": uuid})).await
}
pub async fn session_cancel_queued(id: &str, uuid: &str) -> IpcResult<()> {
    call("session_cancel_queued", json!({"id": id, "uuid": uuid})).await
}
pub async fn session_interrupt(id: &str) -> IpcResult<()> {
    call("session_interrupt", json!({"id": id})).await
}
pub async fn session_answer(id: &str, args: &AnswerArgs) -> IpcResult<()> {
    call("session_answer", json!({"id": id, "args": args})).await
}
pub async fn session_set_mode(id: &str, mode: &str) -> IpcResult<()> {
    call("session_set_mode", json!({"id": id, "mode": mode})).await
}
pub async fn session_set_model(id: &str, model: &str) -> IpcResult<()> {
    call("session_set_model", json!({"id": id, "model": model})).await
}
pub async fn session_set_effort(id: &str, effort: &str) -> IpcResult<()> {
    call("session_set_effort", json!({"id": id, "effort": effort})).await
}
pub async fn session_set_thinking(id: &str, on: bool) -> IpcResult<()> {
    call("session_set_thinking", json!({"id": id, "on": on})).await
}
pub async fn session_set_advisor(id: &str, model: Option<&str>) -> IpcResult<()> {
    call("session_set_advisor", json!({"id": id, "model": model})).await
}
pub async fn session_set_output_style(id: &str, style: &str) -> IpcResult<()> {
    call("session_set_output_style", json!({"id": id, "style": style})).await
}
pub async fn session_set_default(id: &str, model: &str, effort: Option<&str>) -> IpcResult<()> {
    call("session_set_default", json!({"id": id, "model": model, "effort": effort})).await
}
pub async fn session_set_fast(id: &str, on: bool) -> IpcResult<()> {
    call("session_set_fast", json!({"id": id, "on": on})).await
}
pub async fn session_set_ultracode(id: &str, on: bool) -> IpcResult<()> {
    call("session_set_ultracode", json!({"id": id, "on": on})).await
}
pub async fn session_set_draft(id: &str, text: &str, images: &[UserImage]) -> IpcResult<()> {
    call("session_set_draft", json!({"id": id, "text": text, "images": images})).await
}
pub async fn session_request(id: &str, subtype: &str, fields: Option<Value>) -> IpcResult<Value> {
    call("session_request", json!({"id": id, "subtype": subtype, "fields": fields})).await
}
pub async fn session_restart(id: &str) -> IpcResult<()> {
    call("session_restart", json!({"id": id})).await
}
pub async fn session_rename(id: &str, title: &str) -> IpcResult<()> {
    call("session_rename", json!({"id": id, "title": title})).await
}
pub async fn session_set_remote(id: &str, on: bool) -> IpcResult<()> {
    call("session_set_remote", json!({"id": id, "on": on})).await
}
pub async fn session_export(id: &str) -> IpcResult<String> {
    call("session_export", json!({"id": id})).await
}
pub async fn session_start_sign_in(id: &str) -> IpcResult<Option<String>> {
    call("session_start_sign_in", json!({"id": id})).await
}
pub async fn session_finish_sign_in(id: &str) -> IpcResult<()> {
    call("session_finish_sign_in", json!({"id": id})).await
}
pub async fn session_refresh_status(id: &str) -> IpcResult<()> {
    call("session_refresh_status", json!({"id": id})).await
}
pub async fn session_refresh_context(id: &str) -> IpcResult<()> {
    call("session_refresh_context", json!({"id": id})).await
}
pub async fn session_refresh_memory(id: &str) -> IpcResult<()> {
    call("session_refresh_memory", json!({"id": id})).await
}
pub async fn session_refresh_mcp(id: &str) -> IpcResult<()> {
    call("session_refresh_mcp", json!({"id": id})).await
}
pub async fn session_refresh_usage(id: &str, behaviors: bool) -> IpcResult<()> {
    call("session_refresh_usage", json!({"id": id, "behaviors": behaviors})).await
}
pub async fn session_set_auto_memory(id: &str, on: bool) -> IpcResult<()> {
    call("session_set_auto_memory", json!({"id": id, "on": on})).await
}
pub async fn session_set_cwd(id: &str, path: &str, trusted: Option<&str>) -> IpcResult<Option<TrustPrompt>> {
    call("session_set_cwd", json!({"id": id, "path": path, "trusted": trusted})).await
}
pub async fn session_add_dir(id: &str, dir: &str) -> IpcResult<()> {
    call("session_add_dir", json!({"id": id, "dir": dir})).await
}
pub async fn session_background(id: &str, tool_use_id: &str) -> IpcResult<()> {
    call("session_background", json!({"id": id, "tool_use_id": tool_use_id})).await
}
pub async fn session_stop_task(id: &str, task_id: &str) -> IpcResult<()> {
    call("session_stop_task", json!({"id": id, "task_id": task_id})).await
}
pub async fn session_task_output(id: &str, task_id: &str) -> IpcResult<Option<String>> {
    call("session_task_output", json!({"id": id, "task_id": task_id})).await
}
pub async fn session_clear_ended_tasks(id: &str) -> IpcResult<()> {
    call("session_clear_ended_tasks", json!({"id": id})).await
}
pub async fn session_preview_rewind(id: &str, uuid: &str) -> IpcResult<RewindPreview> {
    call("session_preview_rewind", json!({"id": id, "uuid": uuid})).await
}
pub async fn session_rewind(id: &str, uuid: &str, files: bool) -> IpcResult<()> {
    call("session_rewind", json!({"id": id, "uuid": uuid, "files": files})).await
}
pub async fn session_sign_in_again(id: &str, server: &str) -> IpcResult<McpAuthStart> {
    call("session_sign_in_again", json!({"id": id, "server": server})).await
}
pub async fn session_reload_plugins(id: &str) -> IpcResult<ReloadSummary> {
    call("session_reload_plugins", json!({"id": id})).await
}
pub async fn session_hooks(id: &str) -> IpcResult<Option<HookListing>> {
    call("session_hooks", json!({"id": id})).await
}
pub async fn session_permission_rules(id: &str) -> IpcResult<Option<PermissionRulesState>> {
    call("session_permission_rules", json!({"id": id})).await
}
pub async fn claude_auth_logout(cwd: &str) -> IpcResult<CliOutput> {
    call("claude_auth_logout", json!({"cwd": cwd})).await
}

// ============================================================================================ transcripts

pub async fn recent_sessions(take: usize) -> IpcResult<RecentList> {
    call("recent_sessions", json!({"take": take})).await
}
pub async fn sessions_since(since: Time) -> IpcResult<Vec<PastSession>> {
    call("sessions_since", json!({"since": since})).await
}
pub async fn transcript_endings(ids: &[String]) -> IpcResult<Vec<(String, Ending)>> {
    call("transcript_endings", json!({"ids": ids})).await
}
pub async fn plain_claude_cwds() -> IpcResult<Vec<String>> {
    call("plain_claude_cwds", json!({})).await
}
pub async fn search_transcripts(search_id: u64, query: &str, take: usize) -> IpcResult<SearchResult> {
    call("search_transcripts", json!({"search_id": search_id, "query": query, "take": take})).await
}
pub async fn search_transcripts_cancel(search_id: u64) -> IpcResult<()> {
    call("search_transcripts_cancel", json!({"search_id": search_id})).await
}
pub async fn past_prompts(cwd: &str, except_id: Option<&str>) -> IpcResult<Vec<String>> {
    call("past_prompts", json!({"cwd": cwd, "except_id": except_id})).await
}
pub async fn linked_to_pr(pr: &str, cwd: &str) -> IpcResult<Option<PastSession>> {
    call("linked_to_pr", json!({"pr": pr, "cwd": cwd})).await
}
pub async fn last_limits() -> IpcResult<Option<SavedLimits>> {
    call("last_limits", json!({})).await
}
pub async fn folder_used(dir: &str) -> IpcResult<bool> {
    call("folder_used", json!({"dir": dir})).await
}

// ============================================================================================ new session

pub async fn claude_found() -> IpcResult<ClaudeFound> {
    call("claude_found", json!({})).await
}
pub async fn recent_roots() -> IpcResult<Vec<String>> {
    call("recent_roots", json!({})).await
}
pub async fn default_cwd() -> IpcResult<String> {
    call("default_cwd", json!({})).await
}
pub async fn local_dir(path: &str) -> IpcResult<bool> {
    call("local_dir", json!({"path": path})).await
}
pub async fn git_branch(dir: &str) -> IpcResult<Option<String>> {
    call("git_branch", json!({"dir": dir})).await
}
pub async fn agent_names(dir: &str) -> IpcResult<Vec<String>> {
    call("agent_names", json!({"dir": dir})).await
}
pub async fn free_worktree_name(cwd: &str, name: &str) -> IpcResult<String> {
    call("free_worktree_name", json!({"cwd": cwd, "name": name})).await
}

// ============================================================================================ bookmarks

pub async fn bookmarks_of(session: &str) -> IpcResult<Vec<Bookmark>> {
    call("bookmarks_of", json!({"session": session})).await
}
pub async fn bookmark_toggle(session: &str, uuid: &str, gist: &str) -> IpcResult<bool> {
    call("bookmark_toggle", json!({"session": session, "uuid": uuid, "gist": gist})).await
}

// ============================================================================================ background agents

pub async fn bg_list() -> IpcResult<BgList> {
    call("bg_list", json!({})).await
}
pub async fn bg_run(command: &str, id: &str) -> IpcResult<BgRunResult> {
    call("bg_run", json!({"command": command, "id": id})).await
}
pub async fn bg_watch() -> IpcResult<()> {
    call("bg_watch", json!({})).await
}

// ============================================================================================ worktrees

pub async fn worktrees_discover(refresh: bool) -> IpcResult<Vec<String>> {
    call("worktrees_discover", json!({"refresh": refresh})).await
}
pub async fn worktrees_scan(repo: &str, only: Option<&str>) -> IpcResult<Vec<WorktreeInfo>> {
    call("worktrees_scan", json!({"repo": repo, "only": only})).await
}
pub async fn worktrees_cached() -> IpcResult<WorktreeCache> {
    call("worktrees_cached", json!({})).await
}
pub async fn worktrees_cleanable_count() -> IpcResult<Option<i32>> {
    call("worktrees_cleanable_count", json!({})).await
}
pub async fn worktrees_size(path: &str) -> IpcResult<i64> {
    call("worktrees_size", json!({"path": path})).await
}
pub async fn worktrees_push(path: &str, branch: Option<&str>) -> IpcResult<i32> {
    call("worktrees_push", json!({"path": path, "branch": branch})).await
}
pub async fn worktrees_ignores(repo: &str) -> IpcResult<bool> {
    call("worktrees_ignores", json!({"repo": repo})).await
}
pub async fn worktrees_run(run_id: u64, plan: &[CleanupStep]) -> IpcResult<()> {
    call("worktrees_run", json!({"run_id": run_id, "plan": plan})).await
}
pub async fn worktrees_cancel_run(run_id: u64) -> IpcResult<()> {
    call("worktrees_cancel_run", json!({"run_id": run_id})).await
}

// ============================================================================================ files

pub async fn pick_folder(start: Option<&str>, title: &str) -> IpcResult<Option<String>> {
    call("pick_folder", json!({"start": start, "title": title})).await
}
pub async fn reveal_path(path: &str) -> IpcResult<()> {
    call("reveal_path", json!({"path": path})).await
}
pub async fn file_index_scan(request_id: u64, cwd: &str) -> IpcResult<FileIndexResult> {
    call("file_index_scan", json!({"request_id": request_id, "cwd": cwd})).await
}
pub async fn file_index_cancel(request_id: u64) -> IpcResult<()> {
    call("file_index_cancel", json!({"request_id": request_id})).await
}
pub async fn edit_diff_from_input(tool: &str, input: &Value) -> IpcResult<Vec<DiffLine>> {
    call("edit_diff_from_input", json!({"tool": tool, "input": input})).await
}
pub async fn git_diff(cwd: &str) -> IpcResult<GitDiff> {
    call("git_diff", json!({"cwd": cwd})).await
}

// ============================================================================================ extensions

pub async fn plugins_list(cwd: &str) -> IpcResult<PluginList> {
    call("plugins_list", json!({"cwd": cwd})).await
}
pub async fn plugins_markets(cwd: &str) -> IpcResult<Vec<Market>> {
    call("plugins_markets", json!({"cwd": cwd})).await
}
pub async fn plugins_act(cwd: &str, args: &[String]) -> IpcResult<Outcome> {
    call("plugins_act", json!({"cwd": cwd, "args": args})).await
}
pub async fn plugins_options(cwd: &str, id: &str) -> IpcResult<Vec<PluginOption>> {
    call("plugins_options", json!({"cwd": cwd, "id": id})).await
}
pub async fn plugins_save(cwd: &str, id: &str, values: &Map<String, Value>) -> IpcResult<Option<String>> {
    call("plugins_save", json!({"cwd": cwd, "id": id, "values": values})).await
}
pub async fn cli_edit_hook(cwd: &str, edit: &Value) -> IpcResult<Option<String>> {
    call("cli_edit_hook", json!({"cwd": cwd, "edit": edit})).await
}
pub async fn permission_rule_edit(cwd: &str, source: &str, behavior: &str, rule: &str, add: bool) -> IpcResult<()> {
    call("permission_rule_edit", json!({"cwd": cwd, "source": source, "behavior": behavior, "rule": rule, "add": add})).await
}

// ============================================================================================ memory

pub async fn memory_list(id: &str) -> IpcResult<Vec<MemoryFile>> {
    call("memory_list", json!({"id": id})).await
}
pub async fn memory_autos(id: &str) -> IpcResult<Vec<AutoMemory>> {
    call("memory_autos", json!({"id": id})).await
}
pub async fn memory_read(id: &str, path: &str) -> IpcResult<Option<String>> {
    call("memory_read", json!({"id": id, "path": path})).await
}
pub async fn memory_save(id: &str, path: &str, loaded: Option<&str>, text: &str) -> IpcResult<MemorySave> {
    call("memory_save", json!({"id": id, "path": path, "loaded": loaded, "text": text})).await
}
pub async fn memory_delete(id: &str, path: &str) -> IpcResult<bool> {
    call("memory_delete", json!({"id": id, "path": path})).await
}

// ============================================================================================ workflow runs

pub async fn workflow_run(id: &str, tool_id: &str) -> IpcResult<Option<WorkflowRun>> {
    call("workflow_run", json!({"id": id, "tool_id": tool_id})).await
}
pub async fn workflow_agent_log(id: &str, tool_id: &str, agent_id: &str) -> IpcResult<WorkflowAgentLog> {
    call("workflow_agent_log", json!({"id": id, "tool_id": tool_id, "agent_id": agent_id})).await
}
