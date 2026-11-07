//! Tauri commands: THIN wrappers over ccui-core (state = `State<Arc<Core>>`). Owner: tauri-glue.
//! Every command: `#[tauri::command(rename_all = "snake_case")]` (the UI passes snake_case argument keys, see
//! ui/src/ipc.rs), `async`, `Result<T, String>`. Blocking file scans run in `spawn_blocking`.
//! The typed UI bindings in ui/src/ipc.rs mirror this file 1:1 — change both together.

use ccui_core::live::LiveSession;
use ccui_core::Core;
use ccui_shared::api::*;
use ccui_shared::edit_diff::DiffLine;
use ccui_shared::hooks::HookListing;
use ccui_shared::memory::{AutoMemory, MemoryFile, MemorySave};
use ccui_shared::model::{McpAuthStart, ReloadSummary, RewindPreview, UserImage};
use ccui_shared::permission_rules::PermissionRulesState;
use ccui_shared::plugins::{Market, Outcome, PluginList, PluginOption};
use ccui_shared::push::Push;
use ccui_shared::session::{SessionSnapshot, SessionsSummary};
use ccui_shared::workflow::WorkflowRun;
use ccui_shared::worktree_rules::{CleanupStep, WorktreeInfo};
use ccui_shared::Time;
use serde_json::{Map, Value};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use tauri::{AppHandle, Builder, Manager, State, Wry};

type Res<T> = Result<T, String>;
type Core_<'a> = State<'a, Arc<Core>>;

fn session(core: &Core, id: &str) -> Res<Arc<LiveSession>> {
    core.sessions.get(id).ok_or_else(|| ccui_shared::t!("Session.NotFound"))
}

async fn blocking<T: Send + 'static>(f: impl FnOnce() -> T + Send + 'static) -> Res<T> {
    tauri::async_runtime::spawn_blocking(f).await.map_err(|e| e.to_string())
}

// ============================================================================================ app / prefs

#[tauri::command(rename_all = "snake_case")]
async fn app_boot(app: AppHandle) -> Res<BootInfo> {
    Ok(ccui_core::app_flag::boot_info(&app.package_info().version.to_string(), sys_locale::get_locale().as_deref()))
}

#[tauri::command(rename_all = "snake_case")]
async fn app_version(app: AppHandle) -> Res<String> {
    Ok(app.package_info().version.to_string())
}

#[tauri::command(rename_all = "snake_case")]
async fn check_for_updates(app: AppHandle) -> Res<()> {
    crate::check_for_updates(app, true);
    Ok(())
}

/// Set (Some) or delete (None) an AppPref; broadcasts `Push::PrefChanged` when the file changed.
#[tauri::command(rename_all = "snake_case")]
async fn app_pref_set(core: Core_<'_>, name: String, value: Option<String>) -> Res<()> {
    if ccui_core::app_flag::set_pref(&name, value.as_deref())? {
        core.sink.emit(Push::PrefChanged { name, value });
    }
    Ok(())
}

#[tauri::command(rename_all = "snake_case")]
async fn app_flag_set(core: Core_<'_>, name: String, on: bool) -> Res<()> {
    ccui_core::app_flag::set_flag(&name, on)?;
    core.sink.emit(Push::PrefChanged { name, value: on.then(String::new) });
    Ok(())
}

/// Persist the UI culture (`en` | `fr`); the UI reloads the webview after.
#[tauri::command(rename_all = "snake_case")]
async fn set_culture(culture: String) -> Res<()> {
    ccui_core::app_flag::set_culture(ccui_shared::i18n::Lang::from_code(&culture))
}

/// System browser for http/https/mailto only.
#[tauri::command(rename_all = "snake_case")]
async fn open_url(url: String) -> Res<()> {
    let parsed = tauri::Url::parse(url.trim()).map_err(|e| e.to_string())?;
    crate::open_external(&parsed);
    Ok(())
}

// ============================================================================================ sessions (manager)

#[tauri::command(rename_all = "snake_case")]
async fn sessions_summary(core: Core_<'_>) -> Res<SessionsSummary> {
    Ok(core.sessions.summary())
}

/// Live session snapshot, or None.
#[tauri::command(rename_all = "snake_case")]
async fn session_snapshot(core: Core_<'_>, id: String) -> Res<Option<SessionSnapshot>> {
    Ok(core.sessions.get(&id).map(|s| s.snapshot()))
}

/// Resolve `/session/:id`: the live session, else its transcript opened as history; None = not found.
#[tauri::command(rename_all = "snake_case")]
async fn session_open(core: Core_<'_>, id: String) -> Res<Option<SessionSnapshot>> {
    Ok(core.sessions.open_id(&id).await.map(|s| s.snapshot()))
}

#[tauri::command(rename_all = "snake_case")]
async fn session_start(core: Core_<'_>, args: StartArgs) -> Res<String> {
    Ok(core.sessions.start(args).await.id().to_string())
}

#[tauri::command(rename_all = "snake_case")]
async fn session_stop(core: Core_<'_>, id: String) -> Res<()> {
    core.sessions.stop(&id).await;
    Ok(())
}

/// Returns the fork's id.
#[tauri::command(rename_all = "snake_case")]
async fn session_fork(core: Core_<'_>, id: String, at: Option<String>) -> Res<String> {
    let src = session(&core, &id)?;
    Ok(core.sessions.fork(&src, at).await.id().to_string())
}

// ============================================================================================ session commands

#[tauri::command(rename_all = "snake_case")]
async fn session_send(core: Core_<'_>, id: String, text: String, images: Vec<UserImage>) -> Res<String> {
    session(&core, &id)?.send(text, images).await
}

#[tauri::command(rename_all = "snake_case")]
async fn session_send_now(core: Core_<'_>, id: String, uuid: String) -> Res<()> {
    session(&core, &id)?.send_now(&uuid).await
}

#[tauri::command(rename_all = "snake_case")]
async fn session_cancel_queued(core: Core_<'_>, id: String, uuid: String) -> Res<()> {
    session(&core, &id)?.cancel(&uuid).await
}

#[tauri::command(rename_all = "snake_case")]
async fn session_interrupt(core: Core_<'_>, id: String) -> Res<()> {
    session(&core, &id)?.interrupt().await
}

#[tauri::command(rename_all = "snake_case")]
async fn session_answer(core: Core_<'_>, id: String, args: AnswerArgs) -> Res<()> {
    session(&core, &id)?.answer(args).await
}

#[tauri::command(rename_all = "snake_case")]
async fn session_set_mode(core: Core_<'_>, id: String, mode: String) -> Res<()> {
    session(&core, &id)?.set_mode(&mode).await
}

#[tauri::command(rename_all = "snake_case")]
async fn session_set_model(core: Core_<'_>, id: String, model: String) -> Res<()> {
    session(&core, &id)?.set_model(&model).await
}

#[tauri::command(rename_all = "snake_case")]
async fn session_set_effort(core: Core_<'_>, id: String, effort: String) -> Res<()> {
    session(&core, &id)?.set_effort(&effort).await
}

#[tauri::command(rename_all = "snake_case")]
async fn session_set_thinking(core: Core_<'_>, id: String, on: bool) -> Res<()> {
    session(&core, &id)?.set_thinking(on).await
}

#[tauri::command(rename_all = "snake_case")]
async fn session_set_advisor(core: Core_<'_>, id: String, model: Option<String>) -> Res<()> {
    session(&core, &id)?.set_advisor(model).await
}

#[tauri::command(rename_all = "snake_case")]
async fn session_set_output_style(core: Core_<'_>, id: String, style: String) -> Res<()> {
    session(&core, &id)?.set_output_style(&style).await
}

#[tauri::command(rename_all = "snake_case")]
async fn session_set_default(core: Core_<'_>, id: String, model: String, effort: Option<String>) -> Res<()> {
    session(&core, &id)?.set_default(&model, effort).await
}

#[tauri::command(rename_all = "snake_case")]
async fn session_set_fast(core: Core_<'_>, id: String, on: bool) -> Res<()> {
    session(&core, &id)?.set_fast(on).await
}

#[tauri::command(rename_all = "snake_case")]
async fn session_set_ultracode(core: Core_<'_>, id: String, on: bool) -> Res<()> {
    session(&core, &id)?.set_ultracode(on);
    Ok(())
}

/// Composer unsent text + images (debounced by the UI).
#[tauri::command(rename_all = "snake_case")]
async fn session_set_draft(core: Core_<'_>, id: String, text: String, images: Vec<UserImage>) -> Res<()> {
    session(&core, &id)?.set_draft(text, images);
    Ok(())
}

/// Generic control request (spawns the process if needed).
#[tauri::command(rename_all = "snake_case")]
async fn session_request(core: Core_<'_>, id: String, subtype: String, fields: Option<Value>) -> Res<Value> {
    session(&core, &id)?.request(&subtype, fields.unwrap_or_else(|| Value::Object(Map::new())), None).await
}

#[tauri::command(rename_all = "snake_case")]
async fn session_restart(core: Core_<'_>, id: String) -> Res<()> {
    session(&core, &id)?.restart().await
}

#[tauri::command(rename_all = "snake_case")]
async fn session_rename(core: Core_<'_>, id: String, title: String) -> Res<()> {
    session(&core, &id)?.rename(&title).await
}

#[tauri::command(rename_all = "snake_case")]
async fn session_set_remote(core: Core_<'_>, id: String, on: bool) -> Res<()> {
    session(&core, &id)?.set_remote(on).await
}

/// Returns the written file's path (for "Show in folder").
#[tauri::command(rename_all = "snake_case")]
async fn session_export(core: Core_<'_>, id: String) -> Res<String> {
    session(&core, &id)?.export().await
}

#[tauri::command(rename_all = "snake_case")]
async fn session_start_sign_in(core: Core_<'_>, id: String) -> Res<Option<String>> {
    session(&core, &id)?.start_sign_in().await
}

#[tauri::command(rename_all = "snake_case")]
async fn session_finish_sign_in(core: Core_<'_>, id: String) -> Res<()> {
    session(&core, &id)?.finish_sign_in().await
}

#[tauri::command(rename_all = "snake_case")]
async fn session_refresh_status(core: Core_<'_>, id: String) -> Res<()> {
    session(&core, &id)?.refresh_status().await
}

#[tauri::command(rename_all = "snake_case")]
async fn session_refresh_context(core: Core_<'_>, id: String) -> Res<()> {
    session(&core, &id)?.refresh_context().await
}

#[tauri::command(rename_all = "snake_case")]
async fn session_refresh_memory(core: Core_<'_>, id: String) -> Res<()> {
    session(&core, &id)?.refresh_memory().await
}

#[tauri::command(rename_all = "snake_case")]
async fn session_refresh_mcp(core: Core_<'_>, id: String) -> Res<()> {
    session(&core, &id)?.refresh_mcp().await
}

#[tauri::command(rename_all = "snake_case")]
async fn session_refresh_usage(core: Core_<'_>, id: String, behaviors: bool) -> Res<()> {
    session(&core, &id)?.refresh_usage(behaviors).await
}

#[tauri::command(rename_all = "snake_case")]
async fn session_set_auto_memory(core: Core_<'_>, id: String, on: bool) -> Res<()> {
    session(&core, &id)?.set_auto_memory(on).await
}

#[tauri::command(rename_all = "snake_case")]
async fn session_set_cwd(core: Core_<'_>, id: String, path: String, trusted: Option<String>) -> Res<Option<TrustPrompt>> {
    session(&core, &id)?.set_cwd(&path, trusted).await
}

#[tauri::command(rename_all = "snake_case")]
async fn session_add_dir(core: Core_<'_>, id: String, dir: String) -> Res<()> {
    session(&core, &id)?.add_dir(&dir).await
}

#[tauri::command(rename_all = "snake_case")]
async fn session_background(core: Core_<'_>, id: String, tool_use_id: String) -> Res<()> {
    session(&core, &id)?.background(&tool_use_id).await
}

#[tauri::command(rename_all = "snake_case")]
async fn session_stop_task(core: Core_<'_>, id: String, task_id: String) -> Res<()> {
    session(&core, &id)?.stop_task(&task_id).await
}

#[tauri::command(rename_all = "snake_case")]
async fn session_task_output(core: Core_<'_>, id: String, task_id: String) -> Res<Option<String>> {
    session(&core, &id)?.task_output(&task_id).await
}

#[tauri::command(rename_all = "snake_case")]
async fn session_clear_ended_tasks(core: Core_<'_>, id: String) -> Res<()> {
    session(&core, &id)?.clear_ended_tasks();
    Ok(())
}

#[tauri::command(rename_all = "snake_case")]
async fn session_preview_rewind(core: Core_<'_>, id: String, uuid: String) -> Res<RewindPreview> {
    session(&core, &id)?.preview_rewind(&uuid).await
}

#[tauri::command(rename_all = "snake_case")]
async fn session_rewind(core: Core_<'_>, id: String, uuid: String, files: bool) -> Res<()> {
    session(&core, &id)?.rewind(&uuid, files).await
}

#[tauri::command(rename_all = "snake_case")]
async fn session_sign_in_again(core: Core_<'_>, id: String, server: String) -> Res<McpAuthStart> {
    session(&core, &id)?.sign_in_again(&server).await
}

#[tauri::command(rename_all = "snake_case")]
async fn session_reload_plugins(core: Core_<'_>, id: String) -> Res<ReloadSummary> {
    session(&core, &id)?.reload_plugins().await
}

/// None when the session has no process (the hooks tab never spawns one).
#[tauri::command(rename_all = "snake_case")]
async fn session_hooks(core: Core_<'_>, id: String) -> Res<Option<HookListing>> {
    session(&core, &id)?.hooks_listing().await
}

#[tauri::command(rename_all = "snake_case")]
async fn session_permission_rules(core: Core_<'_>, id: String) -> Res<Option<PermissionRulesState>> {
    session(&core, &id)?.permission_rules().await
}

#[tauri::command(rename_all = "snake_case")]
async fn claude_auth_logout(cwd: String) -> Res<CliOutput> {
    ccui_core::cli::auth_logout(&cwd).await
}

// ============================================================================================ transcripts

#[tauri::command(rename_all = "snake_case")]
async fn recent_sessions(take: usize) -> Res<RecentList> {
    blocking(move || {
        let generation = ccui_core::transcript_store::generation();
        RecentList { generation, list: ccui_core::transcript_store::recent(take) }
    })
    .await
}

#[tauri::command(rename_all = "snake_case")]
async fn sessions_since(since: Time) -> Res<Vec<PastSession>> {
    blocking(move || ccui_core::transcript_store::since(since)).await
}

/// `EndOf(id)` for each id that has one.
#[tauri::command(rename_all = "snake_case")]
async fn transcript_endings(ids: Vec<String>) -> Res<Vec<(String, Ending)>> {
    blocking(move || ids.into_iter().filter_map(|id| ccui_core::transcript_store::end_of(&id).map(|e| (id, e))).collect()).await
}

#[tauri::command(rename_all = "snake_case")]
async fn plain_claude_cwds() -> Res<Vec<String>> {
    blocking(|| ccui_core::transcript_store::plain_claude_cwds().into_iter().collect()).await
}

/// Ctrl K message search; a newer `search_id` (or `search_transcripts_cancel`) stops this one.
#[tauri::command(rename_all = "snake_case")]
async fn search_transcripts(core: Core_<'_>, search_id: u64, query: String, take: usize) -> Res<SearchResult> {
    core.searches.current.store(search_id, Ordering::SeqCst);
    let core = core.inner().clone();
    blocking(move || {
        let cancelled = || core.searches.current.load(Ordering::SeqCst) != search_id;
        ccui_core::transcript_store::search(&query, take, &cancelled, ccui_core::transcript_store::SEARCH_BUDGET)
    })
    .await
}

#[tauri::command(rename_all = "snake_case")]
async fn search_transcripts_cancel(core: Core_<'_>, search_id: u64) -> Res<()> {
    let _ = core.searches.current.compare_exchange(search_id, 0, Ordering::SeqCst, Ordering::SeqCst);
    Ok(())
}

#[tauri::command(rename_all = "snake_case")]
async fn past_prompts(cwd: String, except_id: Option<String>) -> Res<Vec<String>> {
    blocking(move || ccui_core::transcript_store::prompts(&cwd, except_id.as_deref(), 10)).await
}

#[tauri::command(rename_all = "snake_case")]
async fn linked_to_pr(pr: String, cwd: String) -> Res<Option<PastSession>> {
    blocking(move || ccui_core::transcript_store::linked_to(&pr, &cwd)).await
}

#[tauri::command(rename_all = "snake_case")]
async fn last_limits() -> Res<Option<SavedLimits>> {
    Ok(ccui_core::session_manager::SessionManager::last_limits())
}

#[tauri::command(rename_all = "snake_case")]
async fn folder_used(dir: String) -> Res<bool> {
    Ok(ccui_core::transcript_store::used(&dir))
}

// ============================================================================================ new session

#[tauri::command(rename_all = "snake_case")]
async fn claude_found() -> Res<ClaudeFound> {
    Ok(ccui_core::new_session::claude_found())
}

#[tauri::command(rename_all = "snake_case")]
async fn recent_roots() -> Res<Vec<String>> {
    blocking(ccui_core::new_session::recent_roots).await
}

#[tauri::command(rename_all = "snake_case")]
async fn default_cwd() -> Res<String> {
    Ok(ccui_core::new_session::default_cwd())
}

#[tauri::command(rename_all = "snake_case")]
async fn local_dir(path: String) -> Res<bool> {
    Ok(ccui_core::new_session::local_dir(&path))
}

#[tauri::command(rename_all = "snake_case")]
async fn git_branch(dir: String) -> Res<Option<String>> {
    Ok(ccui_core::git::branch(&dir).await)
}

#[tauri::command(rename_all = "snake_case")]
async fn agent_names(core: Core_<'_>, dir: String) -> Res<Vec<String>> {
    Ok(ccui_core::new_session::agent_names(&dir, &core.sessions))
}

#[tauri::command(rename_all = "snake_case")]
async fn free_worktree_name(core: Core_<'_>, cwd: String, name: String) -> Res<String> {
    Ok(ccui_core::new_session::free_worktree_name(&cwd, &name, &core.sessions).await)
}

// ============================================================================================ bookmarks

#[tauri::command(rename_all = "snake_case")]
async fn bookmarks_of(session: String) -> Res<Vec<Bookmark>> {
    Ok(ccui_core::bookmarks::of(&session))
}

#[tauri::command(rename_all = "snake_case")]
async fn bookmark_toggle(core: Core_<'_>, session: String, uuid: String, gist: String) -> Res<bool> {
    let on = ccui_core::bookmarks::toggle(&session, &uuid, &gist)?;
    core.sink.emit(Push::Bookmarks { session_id: session });
    Ok(on)
}

// ============================================================================================ background agents

#[tauri::command(rename_all = "snake_case")]
async fn bg_list(core: Core_<'_>) -> Res<BgList> {
    ccui_core::background_agents::list(&core.sink).await
}

#[tauri::command(rename_all = "snake_case")]
async fn bg_run(command: String, id: String) -> Res<BgRunResult> {
    ccui_core::background_agents::run(&command, &id).await
}

/// Start the 1-minute watcher (once per process; the Rail calls it on mount).
#[tauri::command(rename_all = "snake_case")]
async fn bg_watch(core: Core_<'_>) -> Res<()> {
    ccui_core::background_agents::watch(core.sink.clone());
    Ok(())
}

// ============================================================================================ worktrees

#[tauri::command(rename_all = "snake_case")]
async fn worktrees_discover(core: Core_<'_>, refresh: bool) -> Res<Vec<String>> {
    core.worktrees.discover_repos(refresh).await
}

#[tauri::command(rename_all = "snake_case")]
async fn worktrees_scan(core: Core_<'_>, repo: String, only: Option<String>) -> Res<Vec<WorktreeInfo>> {
    core.worktrees.scan(&repo, only.as_deref()).await
}

#[tauri::command(rename_all = "snake_case")]
async fn worktrees_cached(core: Core_<'_>) -> Res<WorktreeCache> {
    Ok(core.worktrees.cached())
}

#[tauri::command(rename_all = "snake_case")]
async fn worktrees_cleanable_count(core: Core_<'_>) -> Res<Option<i32>> {
    Ok(core.worktrees.cleanable_count())
}

#[tauri::command(rename_all = "snake_case")]
async fn worktrees_size(path: String) -> Res<i64> {
    blocking(move || ccui_core::worktrees::WorktreeService::size(&path)).await
}

#[tauri::command(rename_all = "snake_case")]
async fn worktrees_push(core: Core_<'_>, path: String, branch: Option<String>) -> Res<i32> {
    core.worktrees.push(&path, branch.as_deref()).await
}

#[tauri::command(rename_all = "snake_case")]
async fn worktrees_ignores(core: Core_<'_>, repo: String) -> Res<bool> {
    Ok(core.worktrees.ignores_worktrees(&repo).await)
}

/// Runs the plan; progress arrives as `Push::WorktreeRunStep` / `WorktreeRunDone` with this run_id.
#[tauri::command(rename_all = "snake_case")]
async fn worktrees_run(core: Core_<'_>, run_id: u64, plan: Vec<CleanupStep>) -> Res<()> {
    core.worktrees.run(run_id, plan).await
}

#[tauri::command(rename_all = "snake_case")]
async fn worktrees_cancel_run(core: Core_<'_>, run_id: u64) -> Res<()> {
    core.worktrees.cancel_run(run_id);
    Ok(())
}

// ============================================================================================ files

/// Native folder dialog (tauri-plugin-dialog); None = cancelled. One at a time is the UI's job (busy button).
#[tauri::command(rename_all = "snake_case")]
async fn pick_folder(app: AppHandle, start: Option<String>, title: String) -> Res<Option<String>> {
    use tauri_plugin_dialog::DialogExt;
    let start = ccui_core::folder::pick_start(start.as_deref());
    let (tx, rx) = tokio::sync::oneshot::channel();
    app.dialog().file().set_directory(start).set_title(title).pick_folder(move |p| {
        let _ = tx.send(p);
    });
    let picked = rx.await.map_err(|e| e.to_string())?;
    Ok(picked.and_then(|p| p.into_path().ok()).map(|p| ccui_core::folder::trim_picked(&p.to_string_lossy())))
}

/// "Show in folder": reveal only, never open (a file Claude sends could be a script).
#[tauri::command(rename_all = "snake_case")]
async fn reveal_path(path: String) -> Res<()> {
    ccui_core::folder::reveal_check(&path)?;
    tauri_plugin_opener::reveal_item_in_dir(&path).map_err(|e| e.to_string())
}

#[tauri::command(rename_all = "snake_case")]
async fn file_index_scan(core: Core_<'_>, request_id: u64, cwd: String) -> Res<FileIndexResult> {
    let flag = Arc::new(AtomicBool::new(false));
    core.file_scans.running.lock().insert(request_id, flag.clone());
    let result = blocking(move || ccui_core::file_index::scan(&cwd, &flag)).await;
    core.file_scans.running.lock().remove(&request_id);
    result
}

#[tauri::command(rename_all = "snake_case")]
async fn file_index_cancel(core: Core_<'_>, request_id: u64) -> Res<()> {
    if let Some(f) = core.file_scans.running.lock().get(&request_id) {
        f.store(true, Ordering::SeqCst);
    }
    Ok(())
}

#[tauri::command(rename_all = "snake_case")]
async fn edit_diff_from_input(tool: String, input: Value) -> Res<Vec<DiffLine>> {
    blocking(move || ccui_core::edit_diff::from_input(&tool, &input)).await
}

#[tauri::command(rename_all = "snake_case")]
async fn git_diff(cwd: String) -> Res<GitDiff> {
    Ok(ccui_core::edit_diff::git_diff(&cwd).await)
}

// ============================================================================================ extensions

#[tauri::command(rename_all = "snake_case")]
async fn plugins_list(cwd: String) -> Res<PluginList> {
    ccui_core::plugins::list(&cwd).await
}

#[tauri::command(rename_all = "snake_case")]
async fn plugins_markets(cwd: String) -> Res<Vec<Market>> {
    ccui_core::plugins::markets(&cwd).await
}

#[tauri::command(rename_all = "snake_case")]
async fn plugins_act(cwd: String, args: Vec<String>) -> Res<Outcome> {
    ccui_core::plugins::act(&cwd, args).await
}

#[tauri::command(rename_all = "snake_case")]
async fn plugins_options(cwd: String, id: String) -> Res<Vec<PluginOption>> {
    ccui_core::plugins::options(&cwd, &id).await
}

/// None = saved; Some(message) = refused by the CLI.
#[tauri::command(rename_all = "snake_case")]
async fn plugins_save(cwd: String, id: String, values: Map<String, Value>) -> Res<Option<String>> {
    ccui_core::plugins::save(&cwd, &id, values).await
}

/// `claude edit-hook --json` with the edit built by `ccui_shared::cli_edit::hook`; None = written.
#[tauri::command(rename_all = "snake_case")]
async fn cli_edit_hook(cwd: String, edit: Value) -> Res<Option<String>> {
    Ok(ccui_core::cli_edit::run(&cwd, "edit-hook", &edit).await)
}

#[tauri::command(rename_all = "snake_case")]
async fn permission_rule_edit(cwd: String, source: String, behavior: String, rule: String, add: bool) -> Res<()> {
    ccui_core::permission_rules::edit_rule(&cwd, &source, &behavior, &rule, add).await
}

// ============================================================================================ memory

#[tauri::command(rename_all = "snake_case")]
async fn memory_list(core: Core_<'_>, id: String) -> Res<Vec<MemoryFile>> {
    Ok(ccui_core::memory_files::list(&*session(&core, &id)?))
}

#[tauri::command(rename_all = "snake_case")]
async fn memory_autos(core: Core_<'_>, id: String) -> Res<Vec<AutoMemory>> {
    Ok(ccui_core::memory_files::autos(&*session(&core, &id)?))
}

#[tauri::command(rename_all = "snake_case")]
async fn memory_read(core: Core_<'_>, id: String, path: String) -> Res<Option<String>> {
    let known = ccui_core::memory_files::list(&*session(&core, &id)?);
    if !known.iter().any(|f| ccui_core::memory_files::same(&f.path, &path)) {
        return Err(ccui_shared::t!("Mem.NotAllowed"));
    }
    ccui_core::memory_files::read(&path)
}

#[tauri::command(rename_all = "snake_case")]
async fn memory_save(core: Core_<'_>, id: String, path: String, loaded: Option<String>, text: String) -> Res<MemorySave> {
    let known = ccui_core::memory_files::list(&*session(&core, &id)?);
    ccui_core::memory_files::save(&known, &path, loaded.as_deref(), &text)
}

#[tauri::command(rename_all = "snake_case")]
async fn memory_delete(core: Core_<'_>, id: String, path: String) -> Res<bool> {
    let known = ccui_core::memory_files::autos(&*session(&core, &id)?);
    ccui_core::memory_files::delete(&known, &path)
}

// ============================================================================================ workflow runs

/// None when the session or the tool is unknown. The UI re-asks once a second while `live`.
#[tauri::command(rename_all = "snake_case")]
async fn workflow_run(core: Core_<'_>, id: String, tool_id: String) -> Res<Option<WorkflowRun>> {
    let s = session(&core, &id)?;
    let tool = s.with_state(|st| st.tool_index(&tool_id).and_then(|i| st.items()[i].as_tool().cloned()));
    let now = chrono::Utc::now();
    Ok(match tool {
        Some(t) => Some(blocking(move || ccui_core::workflow_runs::read(&t, now)).await?),
        None => None,
    })
}

#[tauri::command(rename_all = "snake_case")]
async fn workflow_agent_log(core: Core_<'_>, id: String, tool_id: String, agent_id: String) -> Res<WorkflowAgentLog> {
    let s = session(&core, &id)?;
    let tool = s
        .with_state(|st| st.tool_index(&tool_id).and_then(|i| st.items()[i].as_tool().cloned()))
        .ok_or_else(|| ccui_shared::t!("Session.NotFound"))?;
    let core = core.inner().clone();
    blocking(move || core.agent_logs.read(&id, &tool, &agent_id)).await
}

// ============================================================================================ registration

pub fn register(builder: Builder<Wry>) -> Builder<Wry> {
    builder.invoke_handler(tauri::generate_handler![
        // app / prefs
        app_boot,
        app_version,
        check_for_updates,
        app_pref_set,
        app_flag_set,
        set_culture,
        open_url,
        // sessions (manager)
        sessions_summary,
        session_snapshot,
        session_open,
        session_start,
        session_stop,
        session_fork,
        // session commands
        session_send,
        session_send_now,
        session_cancel_queued,
        session_interrupt,
        session_answer,
        session_set_mode,
        session_set_model,
        session_set_effort,
        session_set_thinking,
        session_set_advisor,
        session_set_output_style,
        session_set_default,
        session_set_fast,
        session_set_ultracode,
        session_set_draft,
        session_request,
        session_restart,
        session_rename,
        session_set_remote,
        session_export,
        session_start_sign_in,
        session_finish_sign_in,
        session_refresh_status,
        session_refresh_context,
        session_refresh_memory,
        session_refresh_mcp,
        session_refresh_usage,
        session_set_auto_memory,
        session_set_cwd,
        session_add_dir,
        session_background,
        session_stop_task,
        session_task_output,
        session_clear_ended_tasks,
        session_preview_rewind,
        session_rewind,
        session_sign_in_again,
        session_reload_plugins,
        session_hooks,
        session_permission_rules,
        claude_auth_logout,
        // transcripts
        recent_sessions,
        sessions_since,
        transcript_endings,
        plain_claude_cwds,
        search_transcripts,
        search_transcripts_cancel,
        past_prompts,
        linked_to_pr,
        last_limits,
        folder_used,
        // new session
        claude_found,
        recent_roots,
        default_cwd,
        local_dir,
        git_branch,
        agent_names,
        free_worktree_name,
        // bookmarks
        bookmarks_of,
        bookmark_toggle,
        // background agents
        bg_list,
        bg_run,
        bg_watch,
        // worktrees
        worktrees_discover,
        worktrees_scan,
        worktrees_cached,
        worktrees_cleanable_count,
        worktrees_size,
        worktrees_push,
        worktrees_ignores,
        worktrees_run,
        worktrees_cancel_run,
        // files
        pick_folder,
        reveal_path,
        file_index_scan,
        file_index_cancel,
        edit_diff_from_input,
        git_diff,
        // extensions
        plugins_list,
        plugins_markets,
        plugins_act,
        plugins_options,
        plugins_save,
        cli_edit_hook,
        permission_rule_edit,
        // memory
        memory_list,
        memory_autos,
        memory_read,
        memory_save,
        memory_delete,
        // workflow runs
        workflow_run,
        workflow_agent_log,
    ])
}

/// Stops every claude process (app exit, before an update install). Blocks until done.
pub fn shutdown(app: &AppHandle) {
    if let Some(core) = app.try_state::<Arc<Core>>() {
        let core = core.inner().clone();
        tauri::async_runtime::block_on(async move { core.shutdown().await });
    }
}
