# Architecture — the Rust rewrite (Tauri 2 + Leptos 0.8)

The .NET 10 Blazor Server app (C# sources still in the repo: `Core/*.cs`, `ClaudeSession.cs`,
`Components/**/*.razor`, `wwwroot/app.js`, `Resources/*.resx`) is being rewritten in Rust. The sidecar server is
gone: one Tauri process holds the backend (`ccui-core`) and serves the Leptos UI (`ccui-ui`, wasm) in its webview.

The requirements are the port specs `01-live-session.md` … `07-ui-session.md` (spec N §x below). This file is the
contract between the ~13 implementer units: crate map, state sync, pushes, commands, file ownership, conventions.

## 1. Crate map

| Crate | Path | Target | What it holds |
|---|---|---|---|
| `ccui-shared` | `crates/shared` | wasm32 **and** native | IPC types (`model`, `session`, `push`, `api`) + every pure helper both sides need (ToolKinds, ThreadBlocks, TodoList, AskUser, Elicit, ApiErrors, PermissionRules pure parts, Usage, Fmt, ListNav, FileIndex filter, PromptHistory, Invisible, Md, i18n, text helpers, worktree rules, edit diff, images…). No tokio, fs or process. |
| `ccui-core` | `crates/core` | native | Everything the .NET server did: claude processes (stream-json), the event parser, the session reducer and commands, SessionManager, transcripts, worktrees, git, plugins/MCP/memory/workflow runs, background agents, prefs/flags/ledgers. **No tauri dependency**: talks to the UI through `trait UiSink`. Testable with `cargo test`. |
| `claude-code-ui` | `desktop/src-tauri` | native | The shell: updater, menu, notifications, login PATH (unchanged), `sink.rs` (UiSink → Tauri events), `commands.rs` (106 thin commands over `Arc<Core>` in `tauri::State`). |
| `ccui-ui` | `ui` | wasm32 (trunk) | Leptos CSR app: router, layout, pages, session components; `ipc.rs` typed bindings; `state.rs` stores; `interop.rs` (app.js port). `ui/assets/app.css` is the C# app's CSS **verbatim**: DOM structure and class names must match the Razor markup. |

Dependency graph: `ccui-ui → ccui-shared ← ccui-core ← claude-code-ui`.

On-disk state keeps the C# paths and formats so users keep their data: app data =
`dirs::data_local_dir()/ClaudeCodeUI` (flags, prefs, `costs/`, `dirs/`, `limits.json`, `bookmarks.json`,
`repos.json`, new `culture` file), Claude config = `$CLAUDE_CONFIG_DIR` or `~/.claude` (`core::paths`).

## 2. State sync protocol

The backend owns all session state. The UI mirrors it in `ui/src/state.rs`.

1. **First render**: `session_open(id)` (live session, else its transcript opened as history; None = not found) or
   `session_snapshot(id)` → `SessionSnapshot { version, meta, items, streaming_text }`.
2. **Structural changes**: `Push::SessionPatch(SessionPatch { id, version, meta, items: Vec<(index, Item)>, len })`.
   - Throttled to ≤ 1 per 50 ms per session; **immediate** on a permission request and on a result (spec 01 §7.2).
   - `meta` = the whole `SessionMeta` (everything except items); `items` = only the items changed since the last
     push; `len` = item count after the patch. Apply: ignore unless `version` > held version; `items.truncate(len)`;
     write each upsert at its index (index == current len appends); replace meta.
   - Backend side: `SessionState` mutates items only through `push_item/set_item/insert_item/remove_range`, which
     record dirty indexes (`take_dirty`). A removal at i marks every remaining index ≥ i dirty.
3. **Streaming text**: `Push::SessionStream(SessionStream { id, text })` — the whole current text (not a delta),
   sent with the throttled raise when it changed; does not bump `version`. Only `StreamingTail` reads it.
4. **App-wide**: `Push::Sessions(SessionsSummary)` (rail, palette, mascot, quota, Overview, decision queue, held
   ids) — relayed by the SessionManager only when some session's version moved, and on add/stop.
   Initial value: `sessions_summary()`.
5. Every other cross-cutting change is one `Push` variant (below). The UI never polls the backend except where the
   C# app did (Background 15 s, workflow runs 1 s while live, Extensions post-write polls, auth poll).

Composer draft and draft images live in the **backend** (`SessionMeta.draft`, `session_set_draft`, debounced by the
UI) because rewind writes the restored prompt there. The memory panel edit lives in the **frontend** per-session
store (`SessionHandle.memory`). Both survive navigation; `SessionHandle`s are created under the root owner keyed by
session id, so `SessionPage` can remount per id freely.

## 3. Pushes (backend → UI)

One Tauri event, `ccui_shared::push::PUSH_EVENT = "ccui://push"`, payload `ccui_shared::push::Push`
(`#[serde(tag = "type", content = "data")]`). The UI subscribes once (`ipc::listen_push`) and dispatches in
`AppStore::apply_push`.

| Variant | Emitted by | Replaces (C#) |
|---|---|---|
| `SessionPatch(SessionPatch)` | `LiveSession::raise` | `LiveSession.Changed` (version moved) |
| `SessionStream(SessionStream)` | `LiveSession::raise` | streamed-text renders |
| `Sessions(SessionsSummary)` | `SessionManager` change hook / add / stop | `SessionManager.Changed` |
| `SessionRemoved { id }` | `SessionManager::stop` | Stop → page re-resolves |
| `TranscriptsInvalidated { generation }` | `SessionManager::stop` (`transcript_store::invalidate`) | `TranscriptStore.Generation` |
| `WorktreeCount { count }` | `WorktreeService::set_cleanable` | `WorktreeService.Changed` |
| `WorktreeRunStep(WorktreeRunStep)`, `WorktreeRunDone { run_id }` | `WorktreeService::run` | `RunAsync` stream |
| `BgAgents { needs_you }` | `background_agents::list` | `BackgroundAgents.Changed` |
| `Bookmarks { session_id }` | `bookmark_toggle` command | `Bookmarks.Changed` |
| `PrefChanged { name, value }` | `app_pref_set` / `app_flag_set` commands | `AppPref.Changed` |
| `Navigate { path }` | notification click (`main.rs::notify`) | `ccui://open-session` |
| `Updating` | updater, before install | `ccui://updating` |

Native notifications are decided in the backend (`core::notifications::NotifyObserver`, on every session change)
and posted through `UiSink::notify` when the `notify-on` flag is set and the window is not focused.

## 4. Commands (UI → backend)

All in `desktop/src-tauri/src/commands.rs`, `#[tauri::command(rename_all = "snake_case")]`, async,
`Result<T, String>`. Typed 1:1 wrappers with the same names in `ui/src/ipc.rs`. Arguments are passed as a JSON object
with snake_case keys.

- **App / prefs**: `app_boot`, `app_version`, `check_for_updates`, `app_pref_set(name, value?)`,
  `app_flag_set(name, on)`, `set_culture(culture)`, `open_url(url)`.
- **Sessions (manager)**: `sessions_summary`, `session_snapshot(id)`, `session_open(id)`, `session_start(args)`,
  `session_stop(id)`, `session_fork(id, at?)`.
- **Session commands** (`id` first): `session_send(text, images)`, `session_send_now(uuid)`,
  `session_cancel_queued(uuid)`, `session_interrupt`, `session_answer(args)`, `session_set_mode(mode)`,
  `session_set_model(model)`, `session_set_effort(effort)`, `session_set_thinking(on)`, `session_set_advisor(model?)`,
  `session_set_output_style(style)`, `session_set_default(model, effort?)`, `session_set_fast(on)`,
  `session_set_ultracode(on)`, `session_set_draft(text, images)`, `session_request(subtype, fields?)`,
  `session_restart`, `session_rename(title)`, `session_set_remote(on)`, `session_export`, `session_start_sign_in`,
  `session_finish_sign_in`, `session_refresh_status`, `session_refresh_context`, `session_refresh_memory`,
  `session_refresh_mcp`, `session_refresh_usage(behaviors)`, `session_set_auto_memory(on)`,
  `session_set_cwd(path, trusted?)`, `session_add_dir(dir)`, `session_background(tool_use_id)`,
  `session_stop_task(task_id)`, `session_task_output(task_id)`, `session_clear_ended_tasks`,
  `session_preview_rewind(uuid)`, `session_rewind(uuid, files)`, `session_sign_in_again(server)`,
  `session_reload_plugins`, `session_hooks`, `session_permission_rules`; `claude_auth_logout(cwd)`.
- **Transcripts**: `recent_sessions(take)`, `sessions_since(since)`, `transcript_endings(ids)`, `plain_claude_cwds`,
  `search_transcripts(search_id, query, take)`, `search_transcripts_cancel(search_id)`,
  `past_prompts(cwd, except_id?)`, `linked_to_pr(pr, cwd)`, `last_limits`, `folder_used(dir)`.
- **New session**: `claude_found`, `recent_roots`, `default_cwd`, `local_dir(path)`, `git_branch(dir)`,
  `agent_names(dir)`, `free_worktree_name(cwd, name)`.
- **Bookmarks**: `bookmarks_of(session)`, `bookmark_toggle(session, uuid, gist)`.
- **Background agents**: `bg_list`, `bg_run(command, id)`, `bg_watch`.
- **Worktrees**: `worktrees_discover(refresh)`, `worktrees_scan(repo, only?)`, `worktrees_cached`,
  `worktrees_cleanable_count`, `worktrees_size(path)`, `worktrees_push(path, branch?)`, `worktrees_ignores(repo)`,
  `worktrees_run(run_id, plan)`, `worktrees_cancel_run(run_id)`.
- **Files**: `pick_folder(start?, title)`, `reveal_path(path)`, `file_index_scan(request_id, cwd)`,
  `file_index_cancel(request_id)`, `edit_diff_from_input(tool, input)`, `git_diff(cwd)`.
- **Extensions**: `plugins_list(cwd)`, `plugins_markets(cwd)`, `plugins_act(cwd, args)`, `plugins_options(cwd, id)`,
  `plugins_save(cwd, id, values)`, `cli_edit_hook(cwd, edit)`, `permission_rule_edit(cwd, source, behavior, rule, add)`.
  MCP actions (`mcp_status`, `mcp_toggle`, `mcp_reconnect`, `mcp_clear_auth`, `mcp_authenticate`, `reload_skills`)
  go through `session_request`.
- **Memory**: `memory_list(id)`, `memory_autos(id)`, `memory_read(id, path)`, `memory_save(id, path, loaded?, text)`,
  `memory_delete(id, path)` (paths validated against the session's known list in the backend).
- **Workflow runs**: `workflow_run(id, tool_id)`, `workflow_agent_log(id, tool_id, agent_id)`.

Pure helpers the UI calls in place (no IPC): everything in `ccui-shared` (file mention filter, image sniffing,
markdown, todo list, thread blocks, diff from structured patches, worktree plan preview, i18n, formatting…).

**Boot (decision 7)**: the shell's window initialization script (`core::app_flag::boot_script`) sets
`window.__CCUI_BOOT__ = BootInfo` and applies theme / code size / pane widths / `<html lang>` before first paint;
`ui/src/main.rs` reads it synchronously (`ipc::boot_info`) before mounting. `app_boot` returns the same on demand.

`--boot-probe <dir> [secs]` is handled in `main.rs` before the Tauri builder (`core::boot_probe`). `--self-check`
becomes `cargo test`. `--probe-cli` is not ported.

## 5. File ownership (implementer units)

Each file belongs to exactly one unit. A unit edits only its files; a change to someone else's signature goes
through that owner (contract files: through `tauri-glue`). Every file's header names its C# source and spec section.

| Unit | Spec | Files |
|---|---|---|
| **shared-pure-a** | 02 | `crates/shared/src/{json,tool_kinds,thread_blocks,todo_list,ask_user,elicit,api_errors,permission_rules,usage}.rs` |
| **shared-pure-b** | 05 §5-9, 03 §3 | `crates/shared/src/{md,i18n,fmt,list_nav,invisible,prompt_history}.rs` (i18n is implemented; also `scripts/resx2json.py`, `crates/shared/i18n/*.json`) |
| **core-events** | 02 §2-5, 01 §3, 04 §4, 03 §10 | `crates/core/src/{events,process,process_job,boot_probe}.rs`, `crates/shared/src/hooks.rs` |
| **core-live-session** | 01 | `crates/core/src/live/{mod,state,reducer,args,lifecycle,commands}.rs`, `crates/core/src/status_command.rs`, `crates/shared/src/{session_rules,status_command}.rs` |
| **core-store-sessions** | 03 | `crates/core/src/{paths,transcript_store,replay,session_manager,bookmarks,background_agents,user_settings,notifications,app_flag}.rs`, `crates/shared/src/{text,notifications,bg_agents}.rs` |
| **core-worktrees-files** | 04 | `crates/core/src/{worktrees,git,file_index,edit_diff,folder,new_session}.rs`, `crates/shared/src/{worktree_rules,edit_diff,file_index,images}.rs` |
| **core-extensions** | 05 §1-4, 04 §8 | `crates/core/src/{cli,cli_edit,plugins,memory_files,workflow_runs,permission_rules}.rs`, `crates/shared/src/{plugins,memory,workflow,mcp,cli_edit}.rs` |
| **tauri-glue** | 06 §0, decisions 3-7 | `desktop/src-tauri/**` (`main.rs`, `commands.rs`, `sink.rs`, conf, capabilities), `ui/src/ipc.rs`, contract `crates/shared/src/{model,session,push,api,lib}.rs`, `crates/core/src/lib.rs` (`Core`, `UiSink`), workspace `Cargo.toml`s |
| **ui-shell** | 06 §1-5, §7, §9, §11 | `ui/src/{main,app,state,interop}.rs`, `ui/src/layout/*`, `ui/src/common/*`, `ui/src/pages/{mod,not_found}.rs`, `ui/index.html`, `ui/assets/**` (app.css stays verbatim; optional `ui/assets/interop.js`) |
| **ui-pages-a** | 06 §6.1, 6.3-6.6 | `ui/src/pages/{new_session,overview,appearance,background,worktrees}.rs` |
| **ui-extensions** | 06 §6.7 | `ui/src/pages/extensions/*` |
| **ui-session-thread-composer** | 07 §1-4, 6-7, 9, 11-14, 16, 18-19, 21-23, 26, 30, 33, 36 | `ui/src/pages/session_page.rs`, `ui/src/session/{mod,session_title,api_error_card,crash_banner,ledger,todo_panel,hook_activity,file_card,streaming_tail,slash_popover,workflow_card,mode_menu,config_menu,rewind_dialog,tool_row,workflow,status_line,mention_popover,session_header,model_menu,thread,composer}.rs` |
| **ui-session-inspector** | 07 §5, 8, 10, 15, 17, 20, 24-25, 27-29, 31-32, 34-35 | `ui/src/session/{inspector,permission_card,question_form,elicit_form,tasks_panel,task_actions,workflow_panel,agent_panel,tool_detail,diff_panel,context_panel,extra_dirs,memory_panel,status_panel,sign_in}.rs` |

Natural order: shared-pure-a/b and core-events first (everyone uses them), then core-live-session and
core-store-sessions, the rest in parallel. UI units can start against the contract immediately.

## 6. Conventions

- **Errors**: `Result<T, String>` over IPC (`CoreResult<T>` in core). Messages for a person are already localized
  (`ccui_shared::t!`). `ClaudeError` (process) converts to String.
- **No panics on malformed input**: read CLI JSON only through `ccui_shared::json` (`str`, `prop`, `bool`, `long`,
  `names`, `result_text`…); a malformed line/block is skipped, never thrown. Never hold `parking_lot` locks across
  `.await`.
- **Tests**: port every C# `Check()` / `SelfCheck` case as a `#[test]` beside the code, with the same JSON literals
  (real claude 2.1.295/2.1.296 captures). FR-culture asserts: `ccui_shared::i18n::set_thread_lang(Some(Lang::Fr))`.
  Clock-dependent functions take `now`.
- **i18n**: `t!("Key")` (raw value, like `L["Key"]`), `t!("Key", a, b)` (formatted, .NET composite rules incl.
  `{0:N0}` and `{{ }}`), `i18n::plural(n, "Base")`, `i18n::name(prefix, id)`, `i18n::kbd("Ctrl K")`. Values
  containing HTML (`Bg.Text`, `Wt.Why.*`, `New.Step2Text`…) are rendered with `inner_html`, everything else as text.
- **Key order** of JSON written to settings files / CLI stdin is load-bearing: `serde_json` has `preserve_order`
  workspace-wide; write objects in the documented order.
- **Blocking work** (transcript scans, file walks, directory sizes) runs in `spawn_blocking` (the commands do it).
- **UI**: DOM structure and class names follow the Razor markup exactly (app.css verbatim). Elements whose content
  JS mutates (`code.lc`, hljs `pre>code`, copy-button labels) are rendered static. External links go through
  `ipc::open_url`; internal hrefs through the router (base `/`, relative hrefs like `session/{id}`).
- **Skeleton phase**: crates carry `#![allow(dead_code, unused_variables, unused_imports)]`; remove it once a crate
  is fully ported. `todo!("port: <C# member>")` marks every stub.

## 7. Known gaps / decisions to confirm

- Windows process-tree kill (Job Object) needs `win32job` or `windows-sys` in `ccui-core` (not added yet).
- `RateLimits` resets are `Option<Time>` on the wire; the `limits.json` DTO (`SavedLimitsFile`) maps C#
  `default(DateTimeOffset)` ("0001-01-01…") to None.
- `memory_read` takes the session id (the backend validates the path against the known list) — spec 07 listed
  `memory_read(path)`.
- `ExtraDirs.on_add` is a plain `Callback<String>`; the async failure comes back through the `error` prop.
- The single push event carries every session's patches to the one window; a second window would need per-window
  filtering (single window today).
