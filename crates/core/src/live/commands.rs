//! Port of the user commands of `Core/LiveSession.cs` — spec 01 §6 (table), §6.1-6.6. Owner: core-live-session.
//! Each maps to one Tauri command `session_*` (desktop/src-tauri/src/commands.rs). Requests go through
//! `ensure_process()` (spawn if needed) unless the doc says "if proc" (then a missing process is a no-op / None).

use super::{spawn, LiveSession};
use crate::process::{ClaudeError, ClaudeProcess, DEFAULT_TIMEOUT};
use crate::CoreResult;
use ccui_shared::api::{AnswerArgs, TrustPrompt};
use ccui_shared::hooks::HookListing;
use ccui_shared::json;
use ccui_shared::model::{Decision, McpAuthStart, QueuedMessage, ReloadSummary, RewindPreview, SessionStatus, TaskOutput, UserImage};
use ccui_shared::permission_rules::PermissionRulesState;
use ccui_shared::session_rules;
use ccui_shared::t;
use serde_json::{json, Value};
use std::path::{Path, PathBuf};
use std::sync::atomic::Ordering;
use std::sync::Arc;
use std::time::Duration;

/// TitlePrompt, exact (auto-title via Haiku).
pub const TITLE_PROMPT: &str = "Write a 3-7 word title for a coding session that starts with the user's request. Same language as the request. Keep issue/PR numbers. Reply with the title only, no quotes, no trailing period.";

/// `Uri.TryCreate(u, Absolute) && scheme == https`.
fn https(u: &str) -> bool {
    u.get(..8).is_some_and(|p| p.eq_ignore_ascii_case("https://"))
        && u[8..].chars().next().is_some_and(|c| !matches!(c, '/' | '?' | '#') && !c.is_whitespace())
        && !u.chars().any(char::is_control)
}

/// Every suggestion of an AllowSession sent back with the destination the user picked (`Destined`).
fn destined(suggestions: &Value, destination: &str) -> Value {
    let mut a = suggestions.clone();
    if let Value::Array(items) = &mut a {
        for s in items.iter_mut() {
            if let Value::Object(o) = s {
                o.insert("destination".into(), Value::String(destination.into()));
            }
        }
    }
    a
}

impl LiveSession {
    /// The live process, if any (the "if proc" commands).
    fn proc(&self) -> Option<Arc<ClaudeProcess>> {
        self.gate.lock().proc.clone()
    }

    /// `request` keeping the error kind (rewind and task output tell a CLI refusal from a timeout).
    async fn req(self: &Arc<Self>, subtype: &str, fields: Value, timeout: Option<Duration>) -> Result<Value, ClaudeError> {
        let p = self.ensure_process(false).await.map_err(ClaudeError::Io)?;
        p.request(subtype, fields, timeout.unwrap_or(DEFAULT_TIMEOUT)).await
    }

    /// Generic control request through ensure_process (default timeout 15 s).
    pub async fn request(self: &Arc<Self>, subtype: &str, fields: Value, timeout: Option<Duration>) -> CoreResult<Value> {
        self.req(subtype, fields, timeout).await.map_err(String::from)
    }

    /// §6.1: spawn if needed, queue mid-turn (chip) or begin the turn, write the user message; a failed write of a
    /// queued message removes its chip and errors (the Composer puts the text back). Returns the message uuid.
    pub async fn send(self: &Arc<Self>, text: String, images: Vec<UserImage>) -> CoreResult<String> {
        let p = self.ensure_process(false).await?;
        let uuid = uuid::Uuid::new_v4().to_string();
        let untitled = self.with_state_mut(|s| {
            s.suggestion = None;
            s.untitled
        });
        if untitled && !self.titling.swap(true, Ordering::SeqCst) {
            spawn(self.clone().auto_title(text.clone()));
        }
        // During a turn the CLI queues the message (folded at the next tool result, or run as the next turn). A queued
        // message never arms ultracode: it runs under whatever flag the CLI holds when it starts.
        let (queued, ultracode) = self.apply(|s| {
            let queued = matches!(s.status, SessionStatus::Running | SessionStatus::Waiting);
            if queued {
                s.queued.push(QueuedMessage { uuid: uuid.clone(), text: text.clone(), at: chrono::Utc::now() });
            }
            s.sent.insert(uuid.clone());
            (queued, s.ultracode)
        });
        if !queued {
            if ultracode {
                p.request("apply_flag_settings", json!({"settings": {"ultracode": true}}), DEFAULT_TIMEOUT).await?;
            }
            let images = (!images.is_empty()).then(|| images.clone());
            self.apply(|s| {
                s.turn_ultra = s.ultracode;
                s.begin_turn(&text, images, &uuid, chrono::Utc::now());
            });
        }
        self.notify();
        if let Err(e) = p.send_user(&text, &images, &uuid).await {
            if queued {
                // Never written: drop the chip, the Composer puts the text back (never lose a prompt, no duplicate).
                self.apply(|s| s.queued.retain(|m| m.uuid != uuid));
                self.notify();
            }
            return Err(e.into());
        }
        Ok(uuid)
    }

    /// `interrupt {send_now:true, message_uuid}`: stop the turn and start that queued message at once.
    pub async fn send_now(self: &Arc<Self>, uuid: &str) -> CoreResult<()> {
        self.request("interrupt", json!({"send_now": true, "message_uuid": uuid}), None).await.map(|_| ())
    }

    /// `cancel_async_message {message_uuid}` (the `cancelled` lifecycle frame then removes the chip).
    pub async fn cancel(self: &Arc<Self>, uuid: &str) -> CoreResult<()> {
        self.request("cancel_async_message", json!({"message_uuid": uuid}), None).await.map(|_| ())
    }

    /// `rewind_files {user_message_id, dry_run:true}`.
    pub async fn preview_rewind(self: &Arc<Self>, uuid: &str) -> CoreResult<RewindPreview> {
        Ok(RewindPreview::parse(&self.request("rewind_files", json!({"user_message_id": uuid, "dry_run": true}), None).await?))
    }

    /// §6.2: rewind_conversation newest → target one step at a time, cut the thread in `finally`, then rewind_files.
    pub async fn rewind(self: &Arc<Self>, uuid: &str, files: bool) -> CoreResult<()> {
        let steps = {
            let g = self.gate.lock();
            if !g.can_rewind(uuid) {
                return Err(t!("Rewind.Unavailable"));
            }
            session_rules::rewind_steps(g.state.items(), uuid)
        };
        let (mut cut, mut prefill, mut failed) = (None, None, None);
        for id in &steps {
            match self.request("rewind_conversation", json!({"target_message_uuid": id}), None).await {
                Ok(r) if r.get("rewound") == Some(&Value::Bool(true)) => {
                    cut = Some(id.clone());
                    prefill = json::str(&r, "prefillText").map(str::to_string);
                }
                Ok(r) => {
                    let why = json::str(&r, "error").or_else(|| json::str(&r, "reason")).map(str::to_string).unwrap_or_else(|| r.to_string());
                    failed = Some(t!("Rewind.Refused", why));
                    break;
                }
                Err(e) => {
                    failed = Some(e);
                    break;
                }
            }
        }
        // finally: the thread is cut where the CLI's is, even after a refusal.
        if let Some(cut) = cut {
            self.apply(|s| s.cut_back(&cut, prefill.as_deref(), Some(&steps)));
            self.notify();
        }
        if let Some(e) = failed {
            return Err(e);
        }
        if !files {
            return Ok(());
        }
        match self.req("rewind_files", json!({"user_message_id": uuid}), None).await {
            Ok(_) => Ok(()),
            Err(ClaudeError::Request(m)) => Err(t!("Rewind.FilesFailed", m)),
            Err(e) => Err(e.into()),
        }
    }

    /// §6.3: AskUser/Elicit form needs input (checked BEFORE NoProcess); double answers no-op; setMode suggestion
    /// with AllowSession; destination rewrite; resolve.
    pub async fn answer(self: &Arc<Self>, args: AnswerArgs) -> CoreResult<()> {
        let Some(p) = self.with_state(|s| s.pending.iter().find(|p| p.request_id == args.request_id).cloned()) else {
            return Ok(()); // answered meanwhile, or the turn ended: nothing waits for it
        };
        let d = args.decision;
        // A bare allow makes AskUserQuestion return "The user did not answer the questions."
        if d != Decision::Deny && args.input.is_none() {
            if p.tool == ccui_shared::ask_user::TOOL || (p.tool == ccui_shared::elicit::TOOL && !ccui_shared::elicit::is_url(&p.input)) {
                return Err(t!("Ask.NeedsAnswers"));
            }
        }
        let c = self.proc().ok_or_else(|| t!("Session.NoProcess"))?;
        // The request stays in Pending (its card stays up) until the CLI has its reply; a second answer meanwhile is a no-op.
        if !self.gate.lock().answering.insert(p.request_id.clone()) {
            return Ok(());
        }
        let res = self.answer_with(&c, &p, d, args).await;
        self.gate.lock().answering.remove(&p.request_id);
        res
    }

    async fn answer_with(
        self: &Arc<Self>,
        c: &Arc<ClaudeProcess>,
        p: &ccui_shared::model::PendingPermission,
        d: Decision,
        args: AnswerArgs,
    ) -> CoreResult<()> {
        if p.tool == ccui_shared::elicit::TOOL {
            // An elicitation: a deny declines, an allow accepts with the form's content (none in URL mode).
            c.answer(&p.request_id, ccui_shared::elicit::reply(if d == Decision::Deny { "decline" } else { "accept" }, args.input)).await?;
        } else if d == Decision::Deny {
            c.respond(&p.request_id, false, &p.input, None, args.message.as_deref()).await?;
        } else {
            let mut mode = args.mode;
            let mut updated = None;
            if let (Decision::AllowSession, Some(sg @ Value::Array(list))) = (d, &p.suggestions) {
                match list.iter().find(|s| json::str(s, "type") == Some("setMode")).and_then(|s| json::str(s, "mode")) {
                    Some(m) => mode = Some(m.to_string()),
                    None => updated = Some(destined(sg, args.destination.as_deref().unwrap_or("session"))),
                }
            }
            // Answers {"mode":…}; the CLI then emits system/status with it once the allow lands.
            if let Some(m) = mode {
                let r = c.request("set_permission_mode", json!({"mode": m}), DEFAULT_TIMEOUT).await?;
                let now = json::str(&r, "mode").map(str::to_string).unwrap_or(m);
                self.apply(|s| s.mode = now);
                self.notify();
            }
            c.respond(&p.request_id, true, args.input.as_ref().unwrap_or(&p.input), updated, None).await?;
        }
        self.apply(|s| s.resolve(&p.request_id, d));
        self.notify();
        Ok(())
    }

    /// If proc: `background_tasks {tool_use_id}` (Ctrl+B).
    pub async fn background(self: &Arc<Self>, tool_use_id: &str) -> CoreResult<()> {
        if let Some(p) = self.proc() {
            p.request("background_tasks", json!({"tool_use_id": tool_use_id}), DEFAULT_TIMEOUT).await?;
        }
        Ok(())
    }

    /// If proc: `interrupt`.
    pub async fn interrupt(self: &Arc<Self>) -> CoreResult<()> {
        if let Some(p) = self.proc() {
            p.interrupt().await?;
        }
        Ok(())
    }

    /// If proc: `set_permission_mode {mode}`; then mode = m.
    pub async fn set_mode(self: &Arc<Self>, mode: &str) -> CoreResult<()> {
        // Before the first send of a resumed session there is no process yet: the mode goes into --permission-mode.
        if let Some(p) = self.proc() {
            p.request("set_permission_mode", json!({"mode": mode}), DEFAULT_TIMEOUT).await?;
        }
        self.apply(|s| s.mode = mode.to_string());
        self.notify();
        Ok(())
    }

    /// `set_model {model}`; model = m.
    pub async fn set_model(self: &Arc<Self>, model: &str) -> CoreResult<()> {
        self.request("set_model", json!({"model": model}), None).await?;
        self.apply(|s| s.model = Some(model.to_string()));
        self.notify();
        Ok(())
    }

    /// `apply_flag_settings {settings:{effortLevel}}`; effort = e.
    pub async fn set_effort(self: &Arc<Self>, effort: &str) -> CoreResult<()> {
        self.request("apply_flag_settings", json!({"settings": {"effortLevel": effort}}), None).await?;
        self.apply(|s| s.effort = Some(effort.to_string()));
        self.notify();
        Ok(())
    }

    /// `set_max_thinking_tokens {max_thinking_tokens: on ? null : 0}`; thinking = on.
    pub async fn set_thinking(self: &Arc<Self>, on: bool) -> CoreResult<()> {
        let max = if on { Value::Null } else { json!(0) };
        self.request("set_max_thinking_tokens", json!({"max_thinking_tokens": max}), None).await?;
        self.apply(|s| s.thinking = on);
        self.notify();
        Ok(())
    }

    /// `apply_flag_settings {settings:{advisorModel}}` then get_settings → advisor (the CLI may drop it).
    pub async fn set_advisor(self: &Arc<Self>, model: Option<String>) -> CoreResult<()> {
        self.request("apply_flag_settings", json!({"settings": {"advisorModel": model}}), None).await?;
        let r = self.request("get_settings", Value::Null, None).await?;
        let advisor = json::prop(&r, "applied").and_then(|a| json::str(a, "advisor")).map(str::to_string);
        self.apply(|s| s.advisor = advisor);
        self.notify();
        Ok(())
    }

    /// `update_settings {source:"localSettings", settings:{outputStyle}}`.
    pub async fn set_output_style(self: &Arc<Self>, style: &str) -> CoreResult<()> {
        self.request("update_settings", json!({"source": "localSettings", "settings": {"outputStyle": style}}), None).await?;
        self.apply(|s| s.output_style = Some(style.to_string()));
        self.notify();
        Ok(())
    }

    /// `UserSettings::set_model` + userSettings effortLevel unless "max" (session-only).
    pub async fn set_default(self: &Arc<Self>, model: &str, effort: Option<String>) -> CoreResult<()> {
        crate::user_settings::set_model(model)?;
        if let Some(e) = effort.filter(|e| e != "max") {
            self.request("update_settings", json!({"source": "userSettings", "settings": {"effortLevel": e}}), None).await?;
        }
        Ok(())
    }

    /// `apply_flag_settings {settings:{fastMode}}`.
    pub async fn set_fast(self: &Arc<Self>, on: bool) -> CoreResult<()> {
        self.request("apply_flag_settings", json!({"settings": {"fastMode": on}}), None).await.map(|_| ())
    }

    /// Local only: ultracode = on, notify.
    pub fn set_ultracode(&self, on: bool) {
        self.apply(|s| s.ultracode = on);
        self.notify();
    }

    /// Composer unsent text (survives navigation/reload). Ignored (false) when `rev != draft.rev` (a stale debounced
    /// write after cut_back restored a prompt). Never bumps `version` or `draft.rev`: the sender is never echoed.
    pub fn set_draft_text(&self, text: String, rev: u64) -> bool {
        self.with_state_mut(|s| {
            let ok = s.draft.rev == rev;
            if ok {
                s.draft.text = text;
            }
            ok
        })
    }

    /// Composer images (sent only when they change, never per keystroke). Same rev rule as `set_draft_text`.
    pub fn set_draft_images(&self, images: Vec<UserImage>, rev: u64) -> bool {
        self.with_state_mut(|s| {
            let ok = s.draft.rev == rev;
            if ok {
                s.draft.images = images;
            }
            ok
        })
    }

    /// §6.4: validate (`session_rules::extra_dir`), apply the whole list live, record `dirs/<id>.txt`.
    pub async fn add_dir(self: &Arc<Self>, dir: &str) -> CoreResult<()> {
        // Listed under the gate before anything is sent: a process started after this gets it as --add-dir, the one
        // running now gets it below. Taken back if that request fails.
        let (all, p) = {
            let mut g = self.gate.lock();
            let Some(d) = session_rules::extra_dir(dir, &g.state.cwd, &g.state.add_dirs, cfg!(target_os = "linux")) else {
                return Ok(());
            };
            g.state.add_dirs.push(d);
            g.version += 1;
            (g.state.add_dirs.clone(), g.proc.clone())
        };
        if let Some(p) = p {
            let r = p.request("apply_flag_settings", json!({"settings": {"permissions": {"additionalDirectories": all}}}), DEFAULT_TIMEOUT).await;
            if let Err(e) = r {
                let last = all.last().cloned();
                self.apply(|s| {
                    if let Some(i) = s.add_dirs.iter().rposition(|x| Some(x) == last.as_ref()) {
                        s.add_dirs.remove(i);
                    }
                });
                self.notify();
                return Err(e.into());
            }
        }
        let id = self.id().to_string();
        let dirs = all.clone();
        let _ = tokio::task::spawn_blocking(move || crate::transcript_store::record_dirs(&id, &dirs)).await;
        self.notify();
        Ok(())
    }

    /// `get_context_usage` → context.
    pub async fn refresh_context(self: &Arc<Self>) -> CoreResult<()> {
        let r = self.request("get_context_usage", Value::Null, None).await?;
        self.apply(|s| s.heavy.context = Some(r));
        self.notify();
        Ok(())
    }

    /// `mcp_status` → mcp_status.
    pub async fn refresh_mcp(self: &Arc<Self>) -> CoreResult<()> {
        let r = self.request("mcp_status", Value::Null, None).await?;
        self.apply(|s| s.heavy.mcp_status = Some(r));
        self.notify();
        Ok(())
    }

    /// If proc: `get_memory_dialog` → memory_dialog.
    pub async fn refresh_memory(self: &Arc<Self>) -> CoreResult<()> {
        let Some(p) = self.proc() else { return Ok(()) };
        let r = p.request("get_memory_dialog", Value::Null, DEFAULT_TIMEOUT).await?;
        self.apply(|s| s.heavy.memory_dialog = Some(r));
        self.notify();
        Ok(())
    }

    /// `claude edit-memory-settings --json` with `{"autoMemoryEnabled":on}` (CliEdit), then refresh_memory.
    pub async fn set_auto_memory(self: &Arc<Self>, on: bool) -> CoreResult<()> {
        let cwd = self.with_state(|s| s.cwd.clone());
        if let Some(err) = crate::cli_edit::run(&cwd, "edit-memory-settings", &json!({"autoMemoryEnabled": on})).await {
            return Err(err);
        }
        self.refresh_memory().await
    }

    /// If proc: `get_usage` (skip_behaviors unless behaviors), 60 s → usage.
    pub async fn refresh_usage(self: &Arc<Self>, behaviors: bool) -> CoreResult<()> {
        let Some(p) = self.proc() else { return Ok(()) };
        let fields = if behaviors { Value::Null } else { json!({"skip_behaviors": true}) };
        let r = p.request("get_usage", fields, Duration::from_secs(60)).await?;
        self.apply(|s| s.heavy.usage = Some(ccui_shared::usage::Usage::parse(&r)));
        self.notify();
        Ok(())
    }

    /// If proc: `get_status` (30 s) → status_rows.
    pub async fn refresh_status(self: &Arc<Self>) -> CoreResult<()> {
        let Some(p) = self.proc() else { return Ok(()) };
        let r = p.request("get_status", Value::Null, Duration::from_secs(30)).await?;
        self.apply(|s| s.heavy.status_rows = Some(crate::events::status_sections(&r)));
        self.notify();
        Ok(())
    }

    /// `reload_plugins` then `reload_skills`; reducer `reloaded`; counts. A process that exited is NOT respawned.
    pub async fn reload_plugins(self: &Arc<Self>) -> CoreResult<ReloadSummary> {
        let p = self.proc().ok_or_else(|| t!("Session.NoProcess"))?;
        let r = p.request("reload_plugins", Value::Null, DEFAULT_TIMEOUT).await?;
        let k = p.request("reload_skills", Value::Null, DEFAULT_TIMEOUT).await?;
        self.apply(|s| s.reloaded(&r, &k));
        self.notify();
        Ok(ReloadSummary {
            plugins: json::names(&r, "plugins").len() as i32,
            skills: json::names(&k, "skills").len() as i32,
            agents: json::names(&r, "agents").len() as i32,
            errors: json::long(&r, "error_count") as i32,
        })
    }

    /// If proc: `get_hooks_listing` → `events::hooks` (the hooks tab only reads; never spawns).
    pub async fn hooks_listing(self: &Arc<Self>) -> CoreResult<Option<HookListing>> {
        let Some(p) = self.proc() else { return Ok(None) };
        Ok(Some(crate::events::hooks(&p.request("get_hooks_listing", Value::Null, DEFAULT_TIMEOUT).await?)))
    }

    /// If proc: `list_permission_rules` → `permission_rules::parse`.
    pub async fn permission_rules(self: &Arc<Self>) -> CoreResult<Option<PermissionRulesState>> {
        let Some(p) = self.proc() else { return Ok(None) };
        Ok(Some(ccui_shared::permission_rules::parse(&p.request("list_permission_rules", Value::Null, DEFAULT_TIMEOUT).await?)))
    }

    /// Extensions sign-in (spec 06 §6.7): `mcp_authenticate {serverName}` → `events::mcp_auth` (the parser stays in
    /// core; the UI gets the typed answer). Spawns the process if needed, like `request`.
    pub async fn mcp_authenticate(self: &Arc<Self>, server: &str) -> CoreResult<McpAuthStart> {
        Ok(crate::events::mcp_auth(&self.request("mcp_authenticate", json!({"serverName": server}), None).await?))
    }

    /// `mcp_clear_auth` then `mcp_authenticate` → McpAuthStart.
    pub async fn sign_in_again(self: &Arc<Self>, server: &str) -> CoreResult<McpAuthStart> {
        self.request("mcp_clear_auth", json!({"serverName": server}), None).await?;
        self.mcp_authenticate(server).await
    }

    /// `set_cwd {path [, trust_accepted, trusted_directory]}`: needs_trust → Some(prompt); ok → cwd moved (worktree
    /// dropped, git refreshed) → None; else Err(message ?? reason ?? raw). Refused while a turn runs.
    pub async fn set_cwd(self: &Arc<Self>, path: &str, trusted: Option<String>) -> CoreResult<Option<TrustPrompt>> {
        let mut f = json!({"path": path});
        if let Some(t) = trusted {
            f["trust_accepted"] = json!(true);
            f["trusted_directory"] = json!(t);
        }
        let r = self.request("set_cwd", f, None).await?;
        match json::str(&r, "status") {
            Some("needs_trust") => Ok(Some(TrustPrompt {
                directory: json::str(&r, "directory").unwrap_or(path).to_string(),
                root: json::str(&r, "trust_root").map(str::to_string),
            })),
            Some("ok") => {
                if let Some(c) = json::non_empty(&r, "cwd") {
                    self.apply(|s| {
                        if s.cwd != c {
                            s.cwd = c;
                            s.worktree = None;
                        }
                    });
                }
                spawn(self.clone().refresh_git());
                Ok(None)
            }
            _ => Err(json::str(&r, "message").or_else(|| json::str(&r, "reason")).map(str::to_string).unwrap_or_else(|| r.to_string())),
        }
    }

    /// `rename_session {title: trimmed, source:"host"}` (session_title_changed carries the cleaned name).
    pub async fn rename(self: &Arc<Self>, title: &str) -> CoreResult<()> {
        self.request("rename_session", json!({"title": title.trim(), "source": "host"}), None).await.map(|_| ())
    }

    /// `export_conversation` → text written to `free_file(downloads, default_filename)`; returns the path.
    pub async fn export(self: &Arc<Self>) -> CoreResult<String> {
        let r = self.request("export_conversation", Value::Null, None).await?;
        let path = free_file(&crate::paths::downloads(), json::str(&r, "default_filename"));
        tokio::fs::write(&path, json::str(&r, "text").unwrap_or_default()).await.map_err(|e| e.to_string())?;
        Ok(path.to_string_lossy().into_owned())
    }

    /// `remote_control {enabled}` → remote_url (absolute https only).
    pub async fn set_remote(self: &Arc<Self>, on: bool) -> CoreResult<()> {
        let r = self.request("remote_control", json!({"enabled": on}), None).await?;
        let url = json::str(&r, "session_url").filter(|u| on && https(u)).map(str::to_string);
        self.apply(|s| s.remote_url = url);
        self.notify();
        Ok(())
    }

    /// `claude_authenticate {loginWithClaudeAi:true}` → automaticUrl if absolute https.
    pub async fn start_sign_in(self: &Arc<Self>) -> CoreResult<Option<String>> {
        let r = self.request("claude_authenticate", json!({"loginWithClaudeAi": true}), None).await?;
        Ok(json::str(&r, "automaticUrl").filter(|u| https(u)).map(str::to_string))
    }

    /// `claude_oauth_wait_for_completion` (600 s) → initialize_info.account; then refresh_status.
    pub async fn finish_sign_in(self: &Arc<Self>) -> CoreResult<()> {
        let r = self.request("claude_oauth_wait_for_completion", Value::Null, Some(Duration::from_secs(600))).await?;
        if let Some(a) = json::prop(&r, "account").filter(|a| a.is_object()) {
            self.apply(|s| {
                if let Some(info) = &s.heavy.initialize_info {
                    s.heavy.initialize_info = Some(super::reducer::with_key(info, "account", a.clone()));
                }
            });
        }
        self.refresh_status().await
    }

    /// If proc: `get_task_output {task_id}` → Text(output ?? ""); CLI error reply → RequestFailed(msg); no reply in
    /// time → Timeout; no proc → NoProcess.
    pub async fn task_output(self: &Arc<Self>, task_id: &str) -> CoreResult<TaskOutput> {
        let Some(p) = self.proc() else { return Ok(TaskOutput::NoProcess) };
        match p.request("get_task_output", json!({"task_id": task_id}), DEFAULT_TIMEOUT).await {
            Ok(r) => Ok(TaskOutput::Text(json::string(&r, "output"))),
            Err(ClaudeError::Timeout) => Ok(TaskOutput::Timeout),
            Err(e @ (ClaudeError::Request(_) | ClaudeError::Stopped)) => Ok(TaskOutput::RequestFailed(e.to_string())),
            Err(e) => Err(e.into()),
        }
    }

    /// If proc: `stop_task {task_id}` (task_updated then reports "killed").
    pub async fn stop_task(self: &Arc<Self>, task_id: &str) -> CoreResult<()> {
        if let Some(p) = self.proc() {
            p.request("stop_task", json!({"task_id": task_id}), DEFAULT_TIMEOUT).await?;
        }
        Ok(())
    }

    /// UI-only: drop non-running tasks; notify.
    pub fn clear_ended_tasks(&self) {
        self.apply(|s| s.tasks.retain(super::state::task_running));
        self.notify(); // bumps the version: the Inspector drops the panel once no task is left
    }

    /// §6.6: one-shot Haiku title (cwd = temp dir, 5 min); user rename wins; failure → trunc(prompt).
    pub(crate) async fn auto_title(self: Arc<Self>, prompt: String) {
        let input: String = prompt.chars().take(2000).collect();
        // Temp folder, no settings: no CLAUDE.md, hooks or MCP servers loaded for one line of output.
        let args: Vec<String> =
            ["--model", "haiku", "--no-session-persistence", "--tools", "", "--setting-sources", "", "--system-prompt", TITLE_PROMPT]
                .iter()
                .map(|s| s.to_string())
                .collect();
        let tmp = std::env::temp_dir().to_string_lossy().into_owned();
        let title = match crate::cli::run_claude(&tmp, "-p", &args, Some(&input), crate::cli::DEFAULT_TIMEOUT).await {
            Ok(o) if o.code == 0 => session_rules::title_from(&o.stdout),
            _ => None, // no claude, timeout: the prompt's first words
        };
        let Some(title) = title.or_else(|| ccui_shared::text::trunc(Some(&prompt))) else { return };
        if !self.with_state(|s| s.untitled) {
            return; // a user rename meanwhile wins
        }
        if self.rename(&title).await.is_err() {
            // process gone: shown, not persisted
            self.apply(|s| {
                s.name = title;
                s.untitled = false;
            });
            self.notify();
        }
    }
}

/// Bare file name in `dir`, numbered `stem-2.ext`, `-3`… while taken; invalid/empty → `conversation.txt`.
pub fn free_file(dir: &Path, name: Option<&str>) -> PathBuf {
    let invalid: &[char] = if cfg!(windows) { &['<', '>', ':', '"', '/', '\\', '|', '?', '*'] } else { &['/'] };
    let name = name
        .and_then(|n| Path::new(n).file_name())
        .map(|n| n.to_string_lossy().into_owned())
        .filter(|n| !n.is_empty() && !n.contains(invalid) && !n.chars().any(|c| c == '\0' || (cfg!(windows) && c.is_control())))
        .unwrap_or_else(|| "conversation.txt".to_string());
    let p = Path::new(&name);
    let stem = p.file_stem().map(|s| s.to_string_lossy().into_owned()).unwrap_or_default();
    let ext = p.extension().map(|e| format!(".{}", e.to_string_lossy())).unwrap_or_default();
    let mut path = dir.join(&name);
    let mut i = 2;
    while path.exists() {
        path = dir.join(format!("{stem}-{i}{ext}"));
        i += 1;
    }
    path
}
