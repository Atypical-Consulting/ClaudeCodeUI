//! Port of the lifecycle of `Core/LiveSession.cs`: EnsureProcess, Boot, OnExit, Restart, DisposeAsync,
//! RunStatusLine, RefreshGit, OnEvent — spec 01 §5, §7.1. Owner: core-live-session.

use super::{spawn, LiveSession};
use crate::events::{self, ClaudeEvent};
use crate::process::{ClaudeError, ClaudeProcess, DEFAULT_TIMEOUT};
use crate::session_manager::SessionManager;
use crate::{status_command, transcript_store, CoreResult};
use ccui_shared::api::{AnswerArgs, FLAG_THINKING};
use ccui_shared::json;
use ccui_shared::model::{Decision, RateLimits, SessionStatus};
use ccui_shared::t;
use ccui_shared::usage::Usage;
use rust_decimal::Decimal;
use serde_json::{json, Value};
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::Arc;
use std::time::Duration;

const INIT_SECONDS: u64 = 180;

impl LiveSession {
    /// Locked check-then-spawn (two windows / boot + reader must not start two claude on one --session-id):
    /// existing proc → it; stopped → Err(Session.ProcessStopped); spawn (failure → on_exit(None, -1,
    /// Session.StartFailed) then Err); status Starting; then notify and fire `boot`.
    pub async fn ensure_process(self: &Arc<Self>, continue_turn: bool) -> CoreResult<Arc<ClaudeProcess>> {
        let spawned = {
            let mut g = self.gate.lock();
            if let Some(p) = &g.proc {
                return Ok(p.clone());
            }
            // Ended by SessionManager.Stop: a late Send (a stale tab) must not start an orphan claude.
            if g.stopped {
                return Err(t!("Session.ProcessStopped"));
            }
            let resume = g.resumable || transcript_store::find(&self.id).is_some();
            let argv = super::args::args(&g.state, resume, crate::app_flag::flag(FLAG_THINKING));
            // --resume restores the last persisted cost-state: total_cost_usd counts on from it (a fork's from its source's).
            let persisted = if resume {
                transcript_store::persisted_cost(&self.id)
            } else {
                g.state.fork_of.as_deref().map(transcript_store::persisted_cost).unwrap_or(Decimal::ZERO)
            };
            g.state.cost_base = g.state.cost_usd - persisted;
            g.state.resume_turn = continue_turn && resume;
            g.state.crashed_mid_turn = false;
            // The exit handler learns its process's generation once spawned; it reads it under the gate, which this
            // spawn holds until the cell is set.
            let generation = Arc::new(AtomicU64::new(0));
            let (weak_ev, weak_exit, weak_trace) = (Arc::downgrade(self), Arc::downgrade(self), Arc::downgrade(self));
            let cell = generation.clone();
            let id = self.id.clone();
            let res = ClaudeProcess::spawn(
                &g.state.cwd,
                argv,
                g.state.resume_turn,
                Arc::new(move |raw| {
                    if let Some(s) = weak_ev.upgrade() {
                        s.on_event(raw);
                    }
                }),
                Arc::new(move |code, text| {
                    if let Some(s) = weak_exit.upgrade() {
                        let gen = {
                            let _sync = s.gate.lock();
                            cell.load(Ordering::SeqCst)
                        };
                        s.on_exit(Some(gen), code, text);
                    }
                }),
                Some(Arc::new(move |line: &str| {
                    if weak_trace.upgrade().is_some_and(|s| s.booting.load(Ordering::SeqCst)) {
                        eprintln!("[{id}] boot {line}");
                    }
                })),
            );
            match res {
                Ok(p) => {
                    generation.store(p.generation(), Ordering::SeqCst);
                    g.proc = Some(p.clone());
                    g.state.sent.clear();
                    g.state.exit_code = None;
                    g.state.exit_text = None;
                    g.state.status = SessionStatus::Starting;
                    g.version += 1;
                    self.booting.store(true, Ordering::SeqCst);
                    Ok(p)
                }
                Err(e) => Err(e.to_string()),
            }
        };
        match spawned {
            Ok(p) => {
                self.notify();
                spawn(self.clone().boot(p.clone()));
                Ok(p)
            }
            Err(msg) => {
                self.on_exit(None, Some(-1), t!("Session.StartFailed", msg));
                Err(msg)
            }
        }
    }

    /// initialize (180 s, waited ONCE), thinking-off live 0, get_settings, get_usage, then Starting → Idle and the
    /// status line. Initialize timeout → crash banner (on_exit), never a fake Idle.
    pub(crate) async fn boot(self: Arc<Self>, p: Arc<ClaudeProcess>) {
        match self.boot_steps(&p).await {
            Ok(true) => {}
            Ok(false) => return, // initialize timed out: already turned into a crash
            Err(e) => eprintln!("[{}] startup: {e}", self.id),
        }
        self.booting.store(false, Ordering::SeqCst);
        self.apply(|s| {
            if s.status == SessionStatus::Starting {
                s.status = SessionStatus::Idle;
            }
        });
        self.notify();
        spawn(self.clone().run_status_line());
    }

    /// Boot steps 1-7; Ok(false) = initialize timed out (the session was stopped).
    async fn boot_steps(self: &Arc<Self>, p: &Arc<ClaudeProcess>) -> Result<bool, ClaudeError> {
        let init_timeout = Duration::from_secs(INIT_SECONDS);
        // A cold start can keep the CLI busy past a minute. Wait ONCE on the same request_id.
        let init = p.request("initialize", Value::Null, init_timeout);
        tokio::pin!(init);
        // Thinking off survives a relaunch as a live 0, sent before any prompt (never a launch flag).
        let off = if self.with_state(|s| s.thinking) {
            None
        } else {
            let p2 = p.clone();
            Some(tokio::spawn(async move { p2.request("set_max_thinking_tokens", json!({"max_thinking_tokens": 0}), init_timeout).await }))
        };
        let info = tokio::select! {
            r = &mut init => r,
            _ = tokio::time::sleep(Duration::from_secs(60)) => {
                eprintln!("[{}] initialize: still waiting after 60 s", self.id);
                (&mut init).await
            }
        };
        let info = match info {
            Ok(i) => i,
            Err(ClaudeError::Timeout) => {
                // A CLI that never answered initialize will not run a turn either: stop it, show the crash banner.
                eprintln!("[{}] initialize: no answer after {INIT_SECONDS} s, stopping claude", self.id);
                self.booting.store(false, Ordering::SeqCst);
                if self.on_exit(Some(p.generation()), Some(-1), t!("Session.InitTimeout", INIT_SECONDS)) {
                    p.dispose().await;
                }
                return Ok(false);
            }
            Err(e) => return Err(e),
        };
        self.apply(|s| {
            if s.output_style.is_none() {
                s.output_style = json::str(&info, "output_style").map(str::to_string);
            }
            s.heavy.initialize_info = Some(info.clone());
        });
        if let Some(off) = off {
            if let Ok(r) = off.await {
                r?;
            }
        }
        self.apply(|s| {
            if s.mode.is_empty() {
                s.mode = json::str(&info, "current_permission_mode").unwrap_or("default").to_string();
            }
        });
        let count = |n: &str| info.get(n).and_then(Value::as_array).map_or(0, Vec::len);
        println!("[{}] initialize: {} models, {} commands", self.id, count("models"), count("commands"));

        let settings = p.request("get_settings", Value::Null, DEFAULT_TIMEOUT).await?;
        self.gate.lock().status_command = ccui_shared::status_command::of(&settings);
        self.apply(|s| {
            if let Some(a) = json::prop(&settings, "applied") {
                if let Some(e) = json::str(a, "effort") {
                    s.effort = Some(e.to_string());
                }
                if s.model.is_none() {
                    s.model = json::str(a, "model").map(str::to_string);
                }
                s.advisor = json::str(a, "advisor").map(str::to_string);
            }
            // alwaysThinkingEnabled: false starts the session with thinking off, as the terminal's toggle shows it.
            if json::prop(&settings, "effective").and_then(|e| e.get("alwaysThinkingEnabled")) == Some(&Value::Bool(false)) {
                s.thinking = false;
            }
        });

        let usage = p.request("get_usage", json!({"skip_behaviors": true}), DEFAULT_TIMEOUT).await?;
        let pct = |w: &Value| w.get("utilization").and_then(Value::as_f64).map_or(0.0, |u| u / 100.0);
        let reset = |w: &Value| json::str(w, "resets_at").and_then(json::parse_time);
        let remembered = self.apply(|s| {
            s.heavy.usage = Some(Usage::parse(&usage));
            let rl = json::prop(&usage, "rate_limits")?;
            let (h5, d7) = (json::prop(rl, "five_hour")?, json::prop(rl, "seven_day")?);
            if s.limits.is_some() {
                return None;
            }
            // get_usage is 0..100, normalised to 0..1
            let l = RateLimits {
                five_hour: pct(h5),
                five_hour_reset: reset(h5),
                seven_day: pct(d7),
                seven_day_reset: reset(d7),
                status: "allowed".into(),
                ..Default::default()
            };
            s.limits = Some(l.clone());
            s.limits_at = Some(chrono::Utc::now());
            Some(l)
        });
        if let Some(l) = remembered {
            SessionManager::remember_limits(&l);
        }
        Ok(true)
    }

    /// Process exited (`generation` None = never spawned). Ignored when a newer process replaced it. Returns whether
    /// it applied (status Crashed, pending/queue cleared, tools ended, plugin status cleared…).
    pub(crate) fn on_exit(&self, generation: Option<u64>, code: Option<i32>, text: String) -> bool {
        {
            let mut g = self.gate.lock();
            if let Some(gen) = generation {
                if g.proc.as_ref().map(|p| p.generation()) != Some(gen) {
                    return false;
                }
            }
            g.proc = None;
            let s = &mut g.state;
            s.crashed_mid_turn = generation.is_some() && matches!(s.status, SessionStatus::Running | SessionStatus::Waiting);
            s.status = SessionStatus::Crashed;
            s.remote_url = None; // the bridge lived in that process
            s.heavy.status_rows = None; // so did the version, login and model it reported
            s.exit_code = code;
            s.exit_text = Some(text);
            s.start_failed = generation.is_none();
            s.pending.clear();
            s.queued.clear();
            s.draining = false;
            s.turn_started_at = None;
            s.compacting_since = None;
            s.running_hooks.clear();
            s.clear_stream();
            s.end_tools(chrono::Utc::now(), false);
            s.plugin_status.clear(); // the next process never sends text:null for the old plugins' lines
            g.version += 1;
        }
        self.booting.store(false, Ordering::SeqCst);
        self.notify();
        true
    }

    /// Drop the current process first (its exit is then ignored), dispose it, respawn (re-running a crash-cut turn).
    pub async fn restart(self: &Arc<Self>) -> CoreResult<()> {
        let old = self.gate.lock().proc.take();
        if let Some(old) = old {
            old.dispose().await;
        }
        let crashed = self.apply(|s| {
            s.plugin_status.clear();
            s.crashed_mid_turn
        });
        self.ensure_process(crashed).await.map(|_| ())
    }

    /// SessionManager.Stop: stopped = true, dispose the process, Exited unless Crashed, queues cleared.
    pub async fn dispose(&self) {
        let p = {
            let mut g = self.gate.lock();
            g.stopped = true;
            g.proc.take()
        };
        if let Some(p) = p {
            p.dispose().await;
        }
        self.apply(|s| {
            if s.status != SessionStatus::Crashed {
                s.status = SessionStatus::Exited;
            }
            s.pending.clear();
            s.queued.clear();
            s.draining = false;
            s.turn_started_at = None;
            s.end_tools(chrono::Utc::now(), false);
        });
        self.booting.store(false, Ordering::SeqCst);
        self.notify();
    }

    /// Run the statusLine command (one at a time); a non-empty result replaces status_text.
    pub(crate) async fn run_status_line(self: Arc<Self>) {
        let Some((cmd, cwd, meta, info)) = ({
            let g = self.gate.lock();
            g.status_command
                .clone()
                .map(|c| (c, g.state.cwd.clone(), g.state.meta(g.proc.is_some()), g.state.heavy.initialize_info.clone()))
        }) else {
            return;
        };
        if self.status_running.swap(true, Ordering::SeqCst) {
            return;
        }
        let id = self.id.clone();
        let path = tokio::task::spawn_blocking(move || transcript_store::find(&id)).await.ok().flatten();
        let path = path.map(|p| p.to_string_lossy().into_owned());
        let payload = status_command::payload_with(&meta, info.as_ref(), path.as_deref());
        // A failing or silent command leaves the last line rather than flicker.
        if let Some(text) = status_command::run(&cmd, &cwd, &payload).await {
            self.apply(|s| s.status_text = Some(text));
            self.notify();
        }
        self.status_running.store(false, Ordering::SeqCst);
    }

    /// branch (`git branch --show-current`) and repo (common dir's parent name).
    pub async fn refresh_git(self: Arc<Self>) {
        let cwd = self.with_state(|s| s.cwd.clone());
        let branch = crate::git::output(&cwd, &["branch", "--show-current"]).await.map(|b| b.trim().to_string()).filter(|b| !b.is_empty());
        let repo = crate::git::output(&cwd, &["rev-parse", "--path-format=absolute", "--git-common-dir"])
            .await
            .map(|c| c.trim().trim_end_matches(['/', '\\']).to_string())
            .filter(|c| !c.is_empty())
            .and_then(|c| {
                let p = std::path::Path::new(&c);
                p.parent().and_then(|d| d.file_name()).map(|n| n.to_string_lossy().into_owned())
            });
        self.apply(|s| {
            s.branch = branch;
            if repo.is_some() {
                s.repo = repo;
            }
        });
        self.notify();
    }

    /// Every stdout line: parse_all → reduce under the gate → side effects (§7.1 steps 2-9) → notify/throttled.
    pub(crate) fn on_event(self: &Arc<Self>, raw: Value) {
        let (mut any, mut urgent) = (false, false);
        for e in events::parse_all(&raw) {
            let is_result = matches!(e, ClaudeEvent::Result(_));
            // PermissionCard titles a Write Create vs Overwrite: checked once, when the request arrives.
            let target_exists = match &e {
                ClaudeEvent::Permission { tool, input, .. } if tool == "Write" => {
                    json::str(input, "file_path").map(|f| std::path::Path::new(f).exists())
                }
                _ => None,
            };
            let (cost, limits, git, elicit, back, ultra_off) = {
                let mut g = self.gate.lock();
                let cwd = g.state.cwd.clone();
                g.state.reduce(&e, chrono::Utc::now());
                if !matches!(e, ClaudeEvent::TextDelta { .. }) {
                    g.version += 1;
                }
                if let ClaudeEvent::Permission { request_id, .. } = &e {
                    if let Some(p) = g.state.pending.iter_mut().find(|p| &p.request_id == request_id) {
                        p.target_exists = target_exists;
                    }
                }
                let s = &g.state;
                let cost = matches!(&e, ClaudeEvent::Result(r) if r.total_cost_usd > Decimal::ZERO).then_some(s.cost_usd);
                let limits = if matches!(e, ClaudeEvent::RateLimit(_)) { s.limits.clone() } else { None };
                let git = (matches!(e, ClaudeEvent::Init(_)) && s.branch.is_none()) || cwd != s.cwd;
                // The server confirmed a URL-mode elicitation: it is done, accept it for the user.
                let elicit = match &e {
                    ClaudeEvent::ElicitDone { server, elicitation_id } => s
                        .pending
                        .iter()
                        .find(|p| ccui_shared::elicit::matches(p, server, elicitation_id))
                        .map(|p| p.request_id.clone()),
                    _ => None,
                };
                let back = if is_result { s.unanswered().and_then(|u| u.uuid).filter(|u| g.can_rewind(u)) } else { None };
                let ultra_off = is_result && g.state.turn_ultra;
                if ultra_off {
                    g.state.turn_ultra = false;
                    g.state.ultracode = false;
                }
                (cost, limits, git, elicit, back, ultra_off)
            };
            any = true;
            if matches!(e, ClaudeEvent::Permission { .. }) || is_result {
                urgent = true;
            }
            if let Some(c) = cost {
                transcript_store::record_cost(&self.id, c);
            }
            if let Some(l) = limits {
                SessionManager::remember_limits(&l);
            }
            if is_result {
                spawn(self.clone().run_status_line());
            }
            if git {
                spawn(self.clone().refresh_git());
            }
            if let Some(request_id) = elicit {
                let me = self.clone();
                spawn(async move {
                    let args = AnswerArgs { request_id, decision: Decision::Allow, mode: None, message: None, input: None, destination: None };
                    if let Err(e) = me.answer(args).await {
                        eprintln!("[{}] elicitation accept: {e}", me.id());
                    }
                });
            }
            if let Some(uuid) = back {
                let me = self.clone();
                spawn(async move {
                    if let Err(e) = me.rewind(&uuid, false).await {
                        eprintln!("[{}] restore prompt: {e}", me.id());
                    }
                });
            }
            if ultra_off {
                let me = self.clone();
                spawn(async move {
                    if let Err(e) = me.request("apply_flag_settings", json!({"settings": {"ultracode": false}}), None).await {
                        eprintln!("[{}] ultracode off : {e}", me.id());
                    }
                });
            }
        }
        if urgent {
            self.notify();
        } else if any {
            self.throttled();
        }
    }
}
