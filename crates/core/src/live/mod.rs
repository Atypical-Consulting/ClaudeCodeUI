//! Port of `Core/LiveSession.cs` — one conversation: owns its `ClaudeProcess` (lazy spawn), the state folded by the
//! reducer, every user command, the boot sequence, crash/exit handling and the 50 ms change throttle that drives the
//! UI. Spec 01 (all). Owner: core-live-session.
//!
//! Layout: `state` (all fields + item mutation with patch tracking), `reducer` (events → state, also used by
//! transcript replay on a scratch `SessionState`), `args` (CLI argv), `lifecycle` (ensure_process, boot, exit,
//! restart, dispose, status line, git, on_event), `commands` (§6 table).
//!
//! Concurrency: ONE `parking_lot::Mutex<Inner>` (the C# `gate`), never held across `.await`. Fire-and-forget work
//! with `tokio::spawn`. Notifications: `notify()` (urgent: permission, result) / `throttled()` (≤ 1 per 50 ms);
//! each raise emits `Push::SessionPatch` when the version moved and `Push::SessionStream` when the streaming text
//! changed (both only while WATCHED), then calls the manager's change hook.
//!
//! SYNC INVARIANTS (docs/ARCHITECTURE.md §2): `Inner.version` is bumped INSIDE the locked mutation, and
//! `snapshot()` / `take_patch()` / `watch()` read version + items (+ dirty set) in the SAME lock, so a snapshot at
//! version N contains exactly the changes ≤ N and the next patch (base = last pushed version ≤ N) covers the rest.

pub mod args;
pub mod commands;
pub mod lifecycle;
pub mod reducer;
pub mod state;

use crate::process::ClaudeProcess;
use crate::UiSink;
use ccui_shared::model::{Item, Time, ToolState, UserImage};
use ccui_shared::push::Push;
use ccui_shared::session::{SessionDraft, SessionHeavy, SessionMeta, SessionPatch, SessionSnapshot, SessionStream, SessionSummary, ToolBrief};
use parking_lot::Mutex;
use rust_decimal::Decimal;
use state::SessionState;
use std::collections::HashSet;
use std::future::Future;
use std::sync::atomic::{AtomicBool, AtomicI64, Ordering};
use std::sync::Arc;
use std::time::Duration;

/// Manager hook: `(session id, version)` after every raise (the manager relays app-wide only when version moved).
pub type ChangeHook = Arc<dyn Fn(&str, u64) + Send + Sync>;

/// Throttle window of `throttled()` (spec 01 §7.2).
const THROTTLE_MS: i64 = 50;

/// Init-only values set by the creator (`SessionManager.Start/Open/Fork`), spec 01 §2.4.
#[derive(Clone, Debug, Default)]
pub struct SessionInit {
    pub id: String,
    pub name: String,
    pub cwd: String,
    pub mode: String,
    pub worktree: Option<String>,
    pub model: Option<String>,
    pub effort: Option<String>,
    /// A transcript exists (opened from history): `--resume` on first spawn.
    pub resumable: bool,
    pub add_dirs: Vec<String>,
    pub fallback_model: Option<String>,
    pub restricted: bool,
    pub safe_mode: bool,
    pub agent: Option<String>,
    pub fork_of: Option<String>,
    pub fork_at: Option<String>,
    /// History (Open / Fork).
    pub items: Vec<Item>,
    pub cost_usd: Decimal,
    pub cost_at_open: Decimal,
    pub pr_url: Option<String>,
    /// None = now.
    pub started_at: Option<Time>,
    pub last_event_at: Option<Time>,
    pub untitled: bool,
}

/// Private runtime state guarded with the session state by the gate.
pub(crate) struct Inner {
    pub state: SessionState,
    pub proc: Option<Arc<ClaudeProcess>>,
    /// Disposed: a late Send from a stale tab must not start an orphan.
    pub stopped: bool,
    pub resumable: bool,
    /// request ids with a reply in flight (no double answer).
    pub answering: HashSet<String>,
    /// `StatusCommand::of(get_settings)`.
    pub status_command: Option<String>,
    /// Bumped by every change except streamed text, inside the locked mutation (never outside the gate).
    pub version: u64,
    /// Version of the last SessionPatch pushed (= base_version of the next). While unwatched, `take_patch` drops
    /// the dirty set and advances this to `version`, so a later watch's snapshot is never older than the base.
    pub pushed_version: u64,
    /// Heavy part as last pushed (a patch carries `heavy` only when `state.heavy` differs; compared by value).
    pub heavy_pushed: SessionHeavy,
    /// `state.draft.rev` as last pushed.
    pub draft_rev_pushed: u64,
    /// `(state.stream_epoch, byte length)` of the streaming text last pushed: same epoch → `SessionStream { offset:
    /// len, append: stream[len..] }`; new epoch (clear_stream) → offset 0.
    pub stream_pushed: (u64, usize),
    /// UI watchers (`session_watch` / `session_open` +1, `session_unwatch` −1). 0 = no patch / stream is built.
    /// ponytail: a webview reload leaks its watches (patches keep flowing for those sessions); add a per-webview
    /// reset on `app_boot` if that ever costs.
    pub watchers: u32,
}

impl Inner {
    fn has_process(&self) -> bool {
        self.proc.is_some()
    }

    fn snapshot(&self) -> SessionSnapshot {
        let s = &self.state;
        SessionSnapshot {
            version: self.version,
            meta: s.meta(self.has_process()),
            heavy: s.heavy.clone(),
            draft: s.draft.clone(),
            items: s.items().to_vec(),
            streaming_text: s.stream.clone(),
        }
    }

    /// `CanRewind(u)` on the live state (the meta-free twin of `session_rules::can_rewind`).
    pub(crate) fn can_rewind(&self, uuid: &str) -> bool {
        self.state.sent.contains(uuid)
            && self.has_process()
            && self.state.status == ccui_shared::model::SessionStatus::Idle
            && !ccui_shared::session_rules::background_running(self.state.items())
    }
}

pub struct LiveSession {
    id: String,
    pub(crate) gate: Mutex<Inner>,
    pub(crate) last_notify_ms: AtomicI64,
    pub(crate) notify_queued: AtomicBool,
    pub(crate) status_running: AtomicBool,
    pub(crate) titling: AtomicBool,
    pub(crate) sink: Arc<dyn UiSink>,
    pub(crate) on_changed: ChangeHook,
    /// Serialises raises so patches leave in version order (taken without the gate, before it).
    raise_gate: Mutex<()>,
    /// Status is Starting: the process trace prints the boot timeline (read without the gate: the trace runs while
    /// `ensure_process` holds it).
    pub(crate) booting: AtomicBool,
}

pub(crate) fn now_ms() -> i64 {
    chrono::Utc::now().timestamp_millis()
}

/// Fire-and-forget on the current tokio runtime; outside one (plain unit tests) the work is dropped.
pub(crate) fn spawn<F: Future<Output = ()> + Send + 'static>(f: F) {
    if let Ok(h) = tokio::runtime::Handle::try_current() {
        h.spawn(f);
    }
}

impl LiveSession {
    /// `new LiveSession(...) { init-only fields }`; started_at = last_event_at = now unless given.
    pub fn new(init: SessionInit, sink: Arc<dyn UiSink>, on_changed: ChangeHook) -> Arc<LiveSession> {
        let now = chrono::Utc::now();
        let mut s = SessionState::new(&init.id, &init.name, &init.cwd, &init.mode, now);
        s.worktree = init.worktree;
        s.model = init.model;
        s.effort = init.effort;
        s.add_dirs = init.add_dirs;
        s.fallback_model = init.fallback_model;
        s.restricted = init.restricted;
        s.safe_mode = init.safe_mode;
        s.agent = init.agent;
        s.fork_of = init.fork_of;
        s.fork_at = init.fork_at;
        s.set_items(init.items);
        s.cost_usd = init.cost_usd;
        s.cost_at_open = init.cost_at_open;
        s.pr_url = init.pr_url;
        s.started_at = init.started_at.unwrap_or(now);
        s.last_event_at = init.last_event_at.unwrap_or(now);
        s.untitled = init.untitled;
        Arc::new(LiveSession {
            id: init.id,
            gate: Mutex::new(Inner {
                state: s,
                proc: None,
                stopped: false,
                resumable: init.resumable,
                answering: HashSet::new(),
                status_command: None,
                version: 0,
                pushed_version: 0,
                heavy_pushed: SessionHeavy::default(),
                draft_rev_pushed: 0,
                stream_pushed: (0, 0),
                watchers: 0,
            }),
            last_notify_ms: AtomicI64::new(0),
            notify_queued: AtomicBool::new(false),
            status_running: AtomicBool::new(false),
            titling: AtomicBool::new(false),
            sink,
            on_changed,
            raise_gate: Mutex::new(()),
            booting: AtomicBool::new(false),
        })
    }

    pub fn id(&self) -> &str {
        &self.id
    }

    /// `Inner.version`, read under the gate.
    pub fn version(&self) -> u64 {
        self.gate.lock().version
    }

    /// Read the state under the gate (never await inside).
    pub fn with_state<R>(&self, f: impl FnOnce(&SessionState) -> R) -> R {
        f(&self.gate.lock().state)
    }

    /// THE way to change state: lock, run `f`, `version += 1`, unlock (the caller then notify()/throttled()).
    /// Streamed text only (`text_delta`) uses `with_state_mut`, which does not bump.
    pub(crate) fn apply<R>(&self, f: impl FnOnce(&mut SessionState) -> R) -> R {
        let mut g = self.gate.lock();
        let r = f(&mut g.state);
        g.version += 1;
        r
    }

    /// Mutate without bumping the version (streamed text, draft writes from the UI).
    pub(crate) fn with_state_mut<R>(&self, f: impl FnOnce(&mut SessionState) -> R) -> R {
        f(&mut self.gate.lock().state)
    }

    pub fn has_process(&self) -> bool {
        self.gate.lock().has_process()
    }

    /// The read model without items.
    pub fn meta(&self) -> SessionMeta {
        let g = self.gate.lock();
        g.state.meta(g.has_process())
    }

    /// One rail/palette/Overview row.
    pub fn summary(&self) -> SessionSummary {
        let g = self.gate.lock();
        let s = &g.state;
        let items = s.items();
        let last_tool = match s.pending.first() {
            Some(p) => Some(ToolBrief { name: p.tool.clone(), input: p.input.clone() }),
            None => items
                .iter()
                .rev()
                .find_map(|i| i.as_tool().filter(|t| t.parent_tool_use_id.is_none()))
                .map(|t| ToolBrief { name: t.name.clone(), input: t.input.clone() }),
        };
        let todos = ccui_shared::todo_list::from(items);
        let done = todos.iter().filter(|e| e.status == ccui_shared::todo_list::TodoStatus::Completed).count();
        SessionSummary {
            id: s.id.clone(),
            version: g.version,
            name: s.name.clone(),
            cwd: s.cwd.clone(),
            repo: s.repo.clone(),
            branch: s.branch.clone(),
            worktree: s.worktree.clone(),
            mode: s.mode.clone(),
            status: s.status,
            untitled: s.untitled,
            items_len: items.len(),
            pending_count: s.pending.len(),
            first_pending_tool: s.pending.first().map(|p| p.tool.clone()),
            exit_label: s.exit_label(),
            exit_code: s.exit_code,
            start_failed: s.start_failed,
            cost_usd: s.cost_usd,
            cost_at_open: s.cost_at_open,
            tool_count: s.tool_count,
            started_at: s.started_at,
            last_event_at: s.last_event_at,
            last_result_subtype: s.last_result_subtype.clone(),
            last_result_at: s.last_result_at,
            last_turn_ms: s.last_turn_ms,
            has_process: g.has_process(),
            can_fork: s.can_fork(),
            has_init: s.heavy.init.is_some(),
            failed_mcp: s.heavy.init.as_ref().map_or(0, |i| i.mcp.iter().filter(|m| m.status == "failed").count() as i32),
            last_tool,
            todos: (!todos.is_empty()).then_some((done, todos.len())),
        }
    }

    /// Full state for first render: version, meta, heavy, draft, items and streaming text in ONE lock.
    pub fn snapshot(&self) -> SessionSnapshot {
        self.gate.lock().snapshot()
    }

    /// `session_watch` / `session_open`: watchers += 1 and the snapshot, in the same lock (nothing can slip between).
    pub fn watch(&self) -> SessionSnapshot {
        let mut g = self.gate.lock();
        if g.watchers == 0 {
            // The snapshot covers everything up to now: the next patch starts from it.
            g.state.take_dirty();
            g.pushed_version = g.version;
            g.heavy_pushed = g.state.heavy.clone();
            g.draft_rev_pushed = g.state.draft.rev;
            g.stream_pushed = (g.state.stream_epoch, g.state.stream.len());
        }
        g.watchers += 1;
        g.snapshot()
    }

    /// `session_unwatch`: watchers −= 1 (saturating). At 0 patches and streams stop being built.
    pub fn unwatch(&self) {
        let mut g = self.gate.lock();
        g.watchers = g.watchers.saturating_sub(1);
    }

    /// Under the gate: None when version == pushed_version. Unwatched → clear the dirty set, pushed_version =
    /// version, None. Else `SessionPatch { base_version: pushed_version, version, meta, heavy if changed, draft if
    /// rev changed, items: take_dirty(), len }` and record what was pushed.
    pub(crate) fn take_patch(&self) -> Option<SessionPatch> {
        let mut g = self.gate.lock();
        if g.version == g.pushed_version {
            return None;
        }
        if g.watchers == 0 {
            g.state.take_dirty();
            g.pushed_version = g.version;
            return None;
        }
        let heavy = (g.state.heavy != g.heavy_pushed).then(|| g.state.heavy.clone());
        if let Some(h) = &heavy {
            g.heavy_pushed = h.clone();
        }
        let draft: Option<SessionDraft> = (g.state.draft.rev != g.draft_rev_pushed).then(|| g.state.draft.clone());
        g.draft_rev_pushed = g.state.draft.rev;
        let patch = SessionPatch {
            id: self.id.clone(),
            base_version: g.pushed_version,
            version: g.version,
            meta: g.state.meta(g.has_process()),
            heavy,
            draft,
            items: g.state.take_dirty(),
            len: g.state.items().len(),
        };
        g.pushed_version = g.version;
        Some(patch)
    }

    /// Under the gate, watched only: the `SessionStream` increment when the streaming text moved (see
    /// `Inner.stream_pushed`), else None.
    pub(crate) fn take_stream(&self) -> Option<SessionStream> {
        let mut g = self.gate.lock();
        if g.watchers == 0 {
            return None;
        }
        let (epoch, len) = (g.state.stream_epoch, g.state.stream.len());
        let (pushed_epoch, pushed_len) = g.stream_pushed;
        if (epoch, len) == (pushed_epoch, pushed_len) {
            return None;
        }
        g.stream_pushed = (epoch, len);
        let s = &g.state.stream;
        if epoch == pushed_epoch && len > pushed_len && s.is_char_boundary(pushed_len) {
            return Some(SessionStream { id: self.id.clone(), offset: pushed_len, append: s[pushed_len..].to_string() });
        }
        if len == 0 && pushed_len == 0 {
            return None; // cleared while already empty: the UI holds "" too
        }
        Some(SessionStream { id: self.id.clone(), offset: 0, append: s.clone() })
    }

    /// `notify()`: raise now (permission requests, results, commands). The version was already bumped by the
    /// locked mutation that called it.
    pub(crate) fn notify(&self) {
        self.raise();
    }

    /// `throttled()`: raise now if 50 ms passed since the last one, else schedule one raise after the remainder.
    pub(crate) fn throttled(self: &Arc<Self>) {
        let wait = THROTTLE_MS - (now_ms() - self.last_notify_ms.load(Ordering::SeqCst));
        if wait <= 0 {
            self.raise();
            return;
        }
        if self.notify_queued.swap(true, Ordering::SeqCst) {
            return;
        }
        let me = self.clone();
        let fire = move || {
            me.notify_queued.store(false, Ordering::SeqCst);
            me.raise();
        };
        let wait = Duration::from_millis(wait as u64);
        match tokio::runtime::Handle::try_current() {
            Ok(h) => {
                h.spawn(async move {
                    tokio::time::sleep(wait).await;
                    fire();
                });
            }
            Err(_) => {
                std::thread::spawn(move || {
                    std::thread::sleep(wait);
                    fire();
                });
            }
        }
    }

    /// Emit `take_patch()` and/or `take_stream()` when Some; call the change hook with the current version.
    pub(crate) fn raise(&self) {
        self.last_notify_ms.store(now_ms(), Ordering::SeqCst);
        let version = {
            let _order = self.raise_gate.lock();
            if let Some(p) = self.take_patch() {
                self.sink.emit(Push::SessionPatch(p));
            }
            if let Some(s) = self.take_stream() {
                self.sink.emit(Push::SessionStream(s));
            }
            self.version()
        };
        (self.on_changed)(&self.id, version);
    }
}

#[cfg(test)]
mod tests {
    //! LiveSession.Check() (CheckReducer) ported case by case: send → tool_use → permission → deny → result, the
    //! queue, rewind, background agents/shells, workflows, hooks, compaction, fallback… replayed on the reducer.
    use super::*;
    use crate::events::{ClaudeEvent as E, ResultEvt, TaskDone};
    use ccui_shared::api::AnswerArgs;
    use ccui_shared::model::*;
    use ccui_shared::thread_blocks::{self, Block};
    use serde_json::{json, Value};

    struct Rec(Mutex<Vec<Push>>);
    impl UiSink for Rec {
        fn emit(&self, push: Push) {
            self.0.lock().push(push);
        }
        fn notify(&self, _: &str, _: &str, _: &str) {}
        fn window_focused(&self) -> bool {
            true
        }
    }

    fn rec() -> Arc<Rec> {
        Arc::new(Rec(Mutex::new(Vec::new())))
    }

    fn live_with(init: SessionInit, sink: Arc<Rec>) -> Arc<LiveSession> {
        LiveSession::new(init, sink, Arc::new(|_: &str, _: u64| {}))
    }

    fn ls(id: &str, cwd: &str, mode: &str) -> Arc<LiveSession> {
        live_with(SessionInit { id: id.into(), name: id.into(), cwd: cwd.into(), mode: mode.into(), ..Default::default() }, rec())
    }

    /// `Apply(e)`: lock + reduce, version bumped like on_event does for everything but text deltas.
    fn ap(s: &LiveSession, e: E) {
        let delta = matches!(e, E::TextDelta { .. });
        let mut g = s.gate.lock();
        g.state.reduce(&e, chrono::Utc::now());
        if !delta {
            g.version += 1;
        }
    }

    fn begin(s: &LiveSession, text: &str, uuid: &str) {
        s.apply(|st| st.begin_turn(text, None, uuid, chrono::Utc::now()));
    }

    fn items(s: &LiveSession) -> Vec<Item> {
        s.with_state(|st| st.items().to_vec())
    }

    fn last(s: &LiveSession) -> Item {
        s.with_state(|st| st.items().last().cloned().unwrap())
    }

    fn tool(s: &LiveSession, id: &str) -> ToolItem {
        s.with_state(|st| st.tool(id).cloned().unwrap())
    }

    fn input() -> Value {
        json!({"file_path":"C:\\w\\b.txt","content":"x"})
    }

    fn d(s: &str) -> rust_decimal::Decimal {
        s.parse().unwrap()
    }

    fn result(subtype: &str, is_error: bool, cost: &str, ctx: i64, reason: Option<&str>) -> E {
        E::Result(ResultEvt {
            subtype: subtype.into(),
            is_error,
            total_cost_usd: d(cost),
            duration_ms: 10,
            num_turns: 1,
            context_tokens: ctx,
            context_window: None,
            fast_mode_state: None,
            fast_mode_reason: None,
            terminal_reason: reason.map(Into::into),
            denied: None,
            cache_read: 0,
            cache_write: 0,
        })
    }

    fn done(reason: Option<&str>) -> E {
        result("success", false, "0.04", 0, reason)
    }

    fn tool_use(id: &str, name: &str, input: Value, parent: Option<&str>) -> E {
        E::ToolUse { id: id.into(), name: name.into(), input, parent_tool_use_id: parent.map(Into::into), model: None }
    }

    fn tool_result(id: &str, text: &str, is_error: bool, structured: Option<Value>) -> E {
        E::ToolResult { tool_use_id: id.into(), text: text.into(), is_error, structured, not_run: false }
    }

    fn text(t: &str, parent: Option<&str>, synthetic: bool, uuid: Option<&str>) -> E {
        E::AssistantText {
            message_id: "m".into(),
            text: t.into(),
            parent_tool_use_id: parent.map(Into::into),
            synthetic,
            uuid: uuid.map(Into::into),
            at: None,
            model: None,
        }
    }

    fn perm(rid: &str, tool: &str, input: Value, desc: Option<&str>, tuid: Option<&str>) -> E {
        E::Permission {
            request_id: rid.into(),
            tool: tool.into(),
            input,
            description: desc.map(Into::into),
            tool_use_id: tuid.map(Into::into),
            suggestions: None,
            info: None,
        }
    }

    fn hook(name: &str, outcome: &str, code: i32, output: &str, id: Option<&str>) -> E {
        E::Hook(HookEvt { name: name.into(), outcome: outcome.into(), exit_code: Some(code), output: output.into(), id: id.map(Into::into) })
    }

    fn status(st: Option<&str>, pm: Option<&str>) -> E {
        E::Status { status: st.map(Into::into), permission_mode: pm.map(Into::into) }
    }

    fn queue(uuid: &str, state: &str) -> E {
        E::Queue { uuid: uuid.into(), state: state.into() }
    }

    fn task_started(id: &str, tuid: &str, desc: &str, tt: &str, bg: bool, ambient: bool) -> E {
        E::TaskStarted {
            task_id: id.into(),
            tool_use_id: tuid.into(),
            description: desc.into(),
            subagent_type: String::new(),
            task_type: Some(tt.into()),
            backgrounded: bg,
            ambient,
        }
    }

    fn rl(h5: f64, r5: Option<i64>, d7: f64, r7: Option<i64>, status: &str, kind: Option<&str>) -> E {
        let t = |s: i64| chrono::DateTime::from_timestamp(s, 0).unwrap();
        E::RateLimit(RateLimits {
            five_hour: h5,
            five_hour_reset: r5.map(t),
            seven_day: d7,
            seven_day_reset: r7.map(t),
            status: status.into(),
            kind: kind.map(Into::into),
            ..Default::default()
        })
    }

    #[test]
    fn rate_limits_merge_and_prompt_cache() {
        let s = ls("rl", "/w", "default");
        ap(&s, rl(0.0, None, 0.0, None, "rejected", Some("five_hour")));
        assert!(s.with_state(|st| st.limits.is_none()), "no quota before a first window");
        ap(&s, rl(0.05, Some(1), 0.91, Some(2), "allowed", None));
        ap(&s, rl(0.0, None, 0.0, None, "rejected", Some("seven_day")));
        let l = s.with_state(|st| st.limits.clone().unwrap());
        assert!(l.status == "rejected" && l.seven_day == 0.91 && l.five_hour == 0.05, "rate limit status keeps the known windows");
        ap(&s, rl(0.4, Some(3), 0.0, None, "allowed", None));
        let l = s.with_state(|st| st.limits.clone().unwrap());
        assert!(l.five_hour == 0.4 && l.seven_day == 0.91 && l.seven_day_reset.unwrap().timestamp() == 2, "a lone five_hour window keeps seven_day");
        let mut r = result("success", false, "0", 34232, None);
        if let E::Result(x) = &mut r {
            (x.cache_read, x.cache_write) = (11204, 23026);
        }
        ap(&s, r);
        assert_eq!(s.with_state(|st| (st.context_tokens, st.cache_read, st.cache_write)), (34232, 11204, 23026), "prompt cache of the last call");
    }

    #[test]
    fn compaction() {
        // /compact as probed on 2.1.296: compacting, hooks, status null, boundary, summary; then an auto compaction mid-turn.
        let cp = ls("cp", "/w", "default");
        begin(&cp, "/compact", "");
        ap(&cp, status(Some("compacting"), None));
        let since = cp.with_state(|st| st.compacting_since);
        ap(&cp, hook("SessionStart:compact", "success", 0, "ctx", None));
        ap(&cp, status(Some("compacting"), None));
        assert!(since.is_some() && cp.with_state(|st| st.compacting_since) == since, "compacting since its first status");
        ap(&cp, status(None, None));
        assert!(cp.with_state(|st| st.compacting_since.is_none()), "status null ends compacting");
        ap(&cp, E::Compact { trigger: "manual".into(), pre_tokens: 37317, post_tokens: 8995, duration_ms: 3207 });
        ap(&cp, hook("SessionStart:compact", "error", 1, "boom", None));
        ap(&cp, E::Summary { text: "This session is being continued… Summary: hi".into() });
        let it = items(&cp);
        assert!(matches!(&it[..], [Item::User(_), Item::Compact(c), Item::Hook(_)]
            if c.trigger == "manual" && c.pre_tokens == 37317 && c.summary.as_deref() == Some("This session is being continued… Summary: hi")));
        ap(&cp, E::Summary { text: "again".into() });
        assert!(matches!(&items(&cp)[1], Item::Compact(c) if c.summary.as_deref() == Some("This session is being continued… Summary: hi")), "a summary never replaces another");
        ap(&cp, status(Some("compacting"), None));
        ap(&cp, result("success", false, "0", 0, None));
        assert!(cp.with_state(|st| st.compacting_since.is_none()));
        let it = items(&cp);
        assert!(matches!(&thread_blocks::of(&it, false)[..], [Block::Item(0), Block::Item(1), Block::Hooks(_)]), "the card is a block");
        // A transcript writes the /compact line after the boundary: it goes back above its card.
        ap(&cp, E::Compact { trigger: "manual".into(), pre_tokens: 1, post_tokens: 1, duration_ms: 1 });
        ap(&cp, E::UserText { text: "<command-name>/compact</command-name>".into(), at: None, images: None });
        let it = items(&cp);
        assert!(matches!(&it[it.len() - 2..], [Item::User(u), Item::Compact(_)] if u.text == "/compact"));
    }

    #[test]
    fn thinking_fallback_artifacts_push_not_run() {
        let fx = live_with(SessionInit { id: "fx".into(), cwd: "/w".into(), mode: "default".into(), model: Some("claude-fable-5".into()), ..Default::default() }, rec());
        begin(&fx, "q", "u7");
        ap(&fx, E::Thought { text: "Checking.".into(), parent_tool_use_id: None });
        ap(&fx, E::Thought { text: "sub".into(), parent_tool_use_id: Some("toolu_A".into()) });
        ap(&fx, text("partial", None, false, Some("a1")));
        ap(&fx, E::Fallback { text: "Fable 5 declined; retried with Opus 5.5".into(), retry: Some("u7".into()), retracted: vec!["a1".into()], model: Some("claude-opus-5-5".into()) });
        assert!(matches!(&items(&fx)[..], [Item::User(_), Item::Thought(t), Item::Fallback(f)] if t.text == "Checking." && f.retry.as_deref() == Some("u7")));
        assert_eq!(fx.with_state(|st| st.model.clone()).as_deref(), Some("claude-opus-5-5"), "fallback swaps the model");
        ap(&fx, E::Artifacts(vec![Artifact { url: "https://claude.ai/a/1".into(), title: "Report".into() }]));
        assert_eq!(fx.with_state(|st| st.artifacts[0].title.clone()), "Report");
        ap(&fx, E::Artifacts(vec![]));
        assert!(fx.with_state(|st| st.artifacts.is_empty()), "artifacts cleared");
        ap(&fx, tool_use("toolu_P", "PushNotification", json!({"message":"Build ready","status":"proactive"}), None));
        assert_eq!(fx.with_state(|st| (st.pushes, st.last_push.clone())), (1, Some("Build ready".into())), "push notification counted");
        ap(&fx, tool_use("toolu_N", "Bash", input(), None));
        ap(&fx, E::ToolResult { tool_use_id: "toolu_N".into(), text: "held back".into(), is_error: true, structured: None, not_run: true });
        let mut r = result("success", false, "0", 0, None);
        if let E::Result(x) = &mut r {
            x.denied = Some(vec!["toolu_N".into()]);
        }
        ap(&fx, r);
        assert_eq!(tool(&fx, "toolu_N").state, ToolState::NotRun, "held back by auto mode: not run, not denied");
        let epoch = chrono::DateTime::UNIX_EPOCH;
        ap(&fx, E::AssistantText { message_id: "m".into(), text: "x".into(), parent_tool_use_id: None, synthetic: false, uuid: Some("u".into()), at: Some(epoch), model: None });
        assert!(matches!(last(&fx), Item::Text(t) if t.at == Some(epoch)), "text keeps the transcript time");
    }

    #[test]
    fn api_errors_hooks_init() {
        const API_ERR: &str = "API Error: Output blocked by content filtering policy";
        let ae = ls("ae", "C:\\w", "default");
        ap(&ae, text(API_ERR, None, true, None));
        ap(&ae, text(API_ERR, None, false, None));
        ap(&ae, text("Total cost: $0.01", None, true, None));
        assert!(matches!(&items(&ae)[..], [Item::ApiError(e), Item::Text(_), Item::Text(_)] if e.error.raw == API_ERR), "api error item only for synthetic API Error text");
        ap(&ae, text("x", None, false, Some("u9")));
        assert!(matches!(last(&ae), Item::Text(t) if t.uuid.as_deref() == Some("u9")));

        let hk = ls("hk", "C:\\w", "default");
        ap(&hk, hook("Stop", "success", 0, "", None));
        ap(&hk, hook("UserPromptSubmit", "success", 0, "context", None));
        ap(&hk, hook("SessionStart:startup", "success", 0, "plugin prompt", None));
        ap(&hk, hook("PreToolUse:Write", "error", 2, "nope", None));
        ap(&hk, hook("SessionStart:resume", "error", 1, "boom", None));
        assert!(matches!(&items(&hk)[..], [Item::Hook(a), Item::Hook(b), Item::Hook(c)]
            if a.hook.output == "context" && b.hook.exit_code == Some(2) && c.hook.exit_code == Some(1)));
        // claude 2.1.296: hook_started per hook, answered by hook_id; one left unanswered ends with the turn.
        ap(&hk, E::HookStarted { id: "h1".into(), name: "SessionStart:startup".into() });
        ap(&hk, E::HookStarted { id: "h2".into(), name: "SessionStart:startup".into() });
        ap(&hk, hook("SessionStart:startup", "success", 0, "", Some("h1")));
        assert!(hk.with_state(|st| st.running_hooks.len() == 1 && st.running_hooks[0].id == "h2" && st.items().len() == 3));
        ap(&hk, result("success", false, "0", 0, None));
        assert!(hk.with_state(|st| st.running_hooks.is_empty()), "a result ends the hooks still listed");
        ap(&hk, E::Init(InitInfo { session_id: "s".into(), fast_mode_state: "off".into(), output_style: Some("Concise".into()), ..Default::default() }));
        assert_eq!(hk.with_state(|st| st.output_style.clone()).as_deref(), Some("Concise"), "init output_style");

        // Two Agent calls: successful SubagentStart (plain or JSON context) and a silent PostToolBatch stay hidden.
        let sa = ls("sa", "C:\\w", "default");
        let ctx = r#"{"hookSpecificOutput":{"hookEventName":"SubagentStart","additionalContext":"PONYTAIL MODE ACTIVE"}}"#;
        for id in ["a1", "a2"] {
            ap(&sa, tool_use(id, "Agent", input(), None));
            ap(&sa, hook("SubagentStart:general-purpose", "success", 0, ctx, None));
            ap(&sa, hook("SubagentStart:general-purpose", "success", 0, "plain context", None));
            ap(&sa, tool_result(id, "ok", false, None));
            ap(&sa, hook("PostToolBatch", "success", 0, "", None));
        }
        ap(&sa, hook("UserPromptSubmit", "success", 0, &ctx.replace("SubagentStart", "UserPromptSubmit"), None));
        ap(&sa, hook("SubagentStart:Explore", "error", 1, "boom", None));
        let it = items(&sa);
        assert!(matches!(&it[..], [Item::Tool(_), Item::Tool(_), Item::Hook(h)] if h.hook.name == "SubagentStart:Explore"));
        assert!(matches!(&thread_blocks::of(&it, false)[..], [Block::Run { agents: true, tools }, Block::Hooks(_)] if tools.len() == 2));
    }

    #[test]
    fn result_recorded_in_ledger() {
        let dir = tempfile::tempdir().unwrap();
        crate::paths::set_test_root(Some(dir.path()));
        let c = ls("c-ledger", "C:\\w", "default");
        c.on_event(json!({"type":"result","subtype":"success","is_error":false,"duration_ms":10,"num_turns":1,"total_cost_usd":0.0123}));
        assert_eq!(crate::transcript_store::recorded_cost("c-ledger"), Some(d("0.0123")));
        assert_eq!(c.with_state(|st| st.cost_usd), d("0.0123"));
        ls("z-ledger", "C:\\w", "default").on_event(json!({"type":"result","subtype":"success","is_error":false,"duration_ms":10,"num_turns":1,"total_cost_usd":0}));
        assert_eq!(crate::transcript_store::recorded_cost("z-ledger"), None, "slash result not recorded");
        crate::paths::set_test_root(None);
    }

    #[test]
    fn send_permission_deny_result() {
        let s = ls("id", "C:\\w", "default");
        begin(&s, "crée b.txt", "");
        assert!(s.with_state(|st| st.status == SessionStatus::Running && st.turn_started_at.is_some()));
        assert!(matches!(&items(&s)[..], [Item::User(u)] if u.text == "crée b.txt"), "send");
        ap(&s, E::TextDelta { text: "je ".into(), parent_tool_use_id: None });
        ap(&s, E::TextDelta { text: "crée".into(), parent_tool_use_id: None });
        assert_eq!(s.with_state(|st| st.stream.clone()), "je crée", "streaming text");
        let v = s.version();
        s.on_event(json!({"type":"stream_event","event":{"type":"content_block_delta","delta":{"type":"text_delta","text":"!"}}}));
        assert!(s.with_state(|st| st.stream == "je crée!") && s.version() == v, "text delta leaves Version unchanged");
        ap(&s, text("je crée", None, false, None));
        assert!(s.with_state(|st| st.stream.is_empty()) && matches!(last(&s), Item::Text(t) if t.markdown == "je crée"), "text block replaces stream");
        ap(&s, tool_use("t1", "Write", input(), None));
        assert!(s.with_state(|st| st.tool_count == 1) && matches!(last(&s), Item::Tool(t) if t.state == ToolState::Running), "tool_use");
        ap(&s, perm("r1", "Write", input(), Some("b.txt"), Some("t1")));
        assert!(s.with_state(|st| st.status == SessionStatus::Waiting && st.pending.len() == 1));
        assert!(matches!(last(&s), Item::Tool(t) if t.state == ToolState::Waiting), "permission");
        ap(&s, E::CancelRequest { request_id: "other".into() });
        assert_eq!(s.with_state(|st| st.pending.len()), 1, "a cancel for another request changes nothing");
        ap(&s, E::CancelRequest { request_id: "r1".into() });
        assert!(s.with_state(|st| st.status == SessionStatus::Running && st.pending.is_empty()));
        assert!(matches!(last(&s), Item::Tool(t) if t.state == ToolState::Running), "request withdrawn by the CLI");
        ap(&s, E::Retry(RetryEvt { attempt: 1, max_retries: 10, delay_ms: 500, error_status: Some(529), error: Some("overloaded".into()) }));
        assert_eq!(s.with_state(|st| st.retry.as_ref().map(|r| r.attempt)), Some(1), "api_retry shown");
        ap(&s, perm("r1", "Write", input(), Some("b.txt"), Some("t1")));
        assert!(s.with_state(|st| st.retry.is_none()), "progress clears the retry");
        assert!(s.with_state(|st| ccui_shared::session_rules::asking_agent(st.items(), &st.pending[0]).is_none()), "main thread: no asking agent");
        s.apply(|st| st.resolve("r1", Decision::Deny));
        assert!(s.with_state(|st| st.status == SessionStatus::Running && st.pending.is_empty()));
        assert!(matches!(last(&s), Item::Tool(t) if t.state == ToolState::Denied), "deny");
        ap(&s, tool_result("t1", "The user denied this tool use.", true, None));
        assert!(matches!(last(&s), Item::Tool(t) if t.state == ToolState::Denied && t.result_text.as_deref() == Some("The user denied this tool use.")), "denied stays denied");
        ap(&s, tool_use("tr", "Read", input(), None));
        ap(&s, tool_result("tr", "Permission to read b.txt has been denied.", true, None));
        ap(&s, E::Result(ResultEvt {
            subtype: "success".into(),
            is_error: false,
            total_cost_usd: d("0.01"),
            duration_ms: 2400,
            num_turns: 2,
            context_tokens: 41878,
            context_window: Some(1000000),
            fast_mode_state: Some("off".into()),
            fast_mode_reason: Some("sdk_opt_in_required".into()),
            terminal_reason: None,
            denied: Some(vec!["tr".into(), "gone".into()]),
            cache_read: 0,
            cache_write: 0,
        }));
        assert_eq!(tool(&s, "tr").state, ToolState::Denied, "a rule's denial marks the tool denied");
        assert!(s.with_state(|st| st.status == SessionStatus::Idle && st.cost_usd == d("0.01") && st.last_turn_ms == Some(2400) && st.context_tokens == 41878), "result");

        begin(&s, "encore", "");
        ap(&s, perm("r2", "Edit", input(), None, Some("nope")));
        ap(&s, result("error_during_execution", true, "0.03", 0, Some("aborted_streaming")));
        assert!(s.with_state(|st| st.cost_usd == d("0.03") && st.last_turn_cost_usd == d("0.02")), "cost assigned, not added");
        assert!(s.with_state(|st| st.pending.is_empty()
            && st.status == SessionStatus::Idle
            && st.last_result_subtype.as_deref() == Some(ccui_shared::session_rules::INTERRUPTED)
            && st.context_tokens == 41878), "interrupted turn");
        s.apply(|st| st.resolve("r2", Decision::Allow)); // the reply landed after the turn ended
        assert!(s.with_state(|st| st.status == SessionStatus::Idle && st.pending.is_empty()), "late resolve is a no-op");
        ap(&s, result("success", false, "0", 0, None));
        assert_eq!(s.with_state(|st| st.cost_usd), d("0.03"), "slash command result keeps cost");
    }

    #[test]
    fn esc_before_an_answer_restores_the_prompt() {
        let ir = ls("ir", "/w", "default");
        let shot = UserImage { media_type: "image/png".into(), url: Some("data:image/png;base64,AAAA".into()), name: None };
        ir.apply(|st| st.begin_turn("fix it", Some(vec![shot]), "u9", chrono::Utc::now()));
        ap(&ir, hook("UserPromptSubmit", "success", 0, "", None));
        ap(&ir, result("error_during_execution", true, "0", 0, Some("aborted_streaming")));
        assert_eq!(ir.with_state(|st| st.unanswered()).and_then(|u| u.uuid).as_deref(), Some("u9"), "interrupted before an answer: restorable");
        ir.apply(|st| st.cut_back("u9", Some("fix it"), None));
        assert!(ir.with_state(|st| st.draft.text == "fix it" && st.draft.images.len() == 1 && st.draft.images[0].media_type == "image/png" && st.items().is_empty()), "prompt and images back in the Composer");
        begin(&ir, "go", "u10");
        ap(&ir, text("partial", None, false, None));
        ap(&ir, result("error_during_execution", true, "0", 0, Some("aborted_streaming")));
        assert!(ir.with_state(|st| st.unanswered()).is_none(), "an answered turn stays in the thread");
        ap(&ir, E::Suggestion { text: "run the tests".into() });
        assert_eq!(ir.with_state(|st| st.suggestion.clone()).as_deref(), Some("run the tests"), "prompt suggestion kept");
        ap(&ir, E::Reset);
        assert!(ir.with_state(|st| st.suggestion.is_none()), "prompt suggestion dropped on /clear");
        ap(&ir, E::Suggestion { text: "run the tests".into() });
        ir.apply(|st| st.cut_back("u10", None, None));
        assert!(ir.with_state(|st| st.suggestion.is_none()), "prompt suggestion dropped on rewind");
    }

    #[test]
    fn queued_messages() {
        // Messages sent mid-turn, replayed as the command_lifecycle frames --probe-cli queue captured.
        let s = ls("q", "C:\\w", "default");
        let enqueue = |uuid: &str, t: &str| {
            s.apply(|st| st.queued.push(QueuedMessage { uuid: uuid.into(), text: t.into(), at: chrono::Utc::now() }));
            ap(&s, queue(uuid, "queued"));
        };
        let user_last = |s: &LiveSession| match last(s) {
            Item::User(u) => (u.text, u.ultracode),
            other => panic!("not a user item: {other:?}"),
        };
        begin(&s, "un", "u1");
        enqueue("u2", "deux");
        assert!(s.with_state(|st| st.queued.len() == 1 && st.queued[0].text == "deux"));
        assert_eq!(user_last(&s).0, "un", "queued message waits outside the thread");
        ap(&s, done(None));
        assert!(s.with_state(|st| st.status == SessionStatus::Running && st.turn_started_at.is_some()), "text-only turn: queued message runs next");
        ap(&s, queue("u1", "completed"));
        ap(&s, queue("u2", "started"));
        assert!(s.with_state(|st| st.queued.is_empty()) && user_last(&s).0 == "deux", "started: joins the thread");
        ap(&s, done(None));
        assert!(s.with_state(|st| st.status == SessionStatus::Idle && st.turn_started_at.is_none()), "queued turn done");

        begin(&s, "trois", "u3");
        enqueue("u4", "quatre");
        ap(&s, tool_use("t4", "Bash", input(), None));
        ap(&s, tool_result("t4", "one", false, None));
        ap(&s, queue("u4", "started"));
        let it = items(&s);
        assert!(matches!(&it[it.len() - 2..], [Item::Tool(t), Item::User(u)] if t.id == "t4" && u.text == "quatre"), "folded after the tool result");
        ap(&s, queue("u4", "completed"));
        ap(&s, done(None));
        assert!(s.with_state(|st| st.status == SessionStatus::Idle && st.queued.is_empty()), "one result for the folded turn");

        begin(&s, "cinq", "u5");
        enqueue("u6", "six");
        ap(&s, queue("u6", "cancelled"));
        assert!(s.with_state(|st| st.queued.is_empty() && st.status == SessionStatus::Running) && user_last(&s).0 == "cinq", "cancelled before start");
        ap(&s, done(None));
        assert_eq!(s.with_state(|st| st.status), SessionStatus::Idle, "cancelled: no extra turn");

        begin(&s, "sept", "u7");
        enqueue("u8", "huit");
        ap(&s, done(Some("aborted_streaming")));
        assert!(s.with_state(|st| st.status == SessionStatus::Running && st.last_result_subtype.as_deref() == Some("interrupted") && st.queued.len() == 1), "interrupt keeps the queue");
        ap(&s, queue("u8", "started"));
        assert_eq!(user_last(&s).0, "huit", "queued message runs after the interrupt");
        ap(&s, done(None));
        enqueue("u9", "neuf"); // a result raced the cancel: the CLI's terminal frame must not leave the session busy
        s.apply(|st| st.status = SessionStatus::Running);
        ap(&s, done(None));
        ap(&s, queue("u9", "discarded"));
        assert!(s.with_state(|st| st.status == SessionStatus::Idle && st.turn_started_at.is_none()), "draining ends when the queue empties");

        begin(&s, "dix", "u10");
        enqueue("u11", "onze"); // Esc while a permission waits: the queued turn runs, not "waiting"
        ap(&s, perm("r10", "Bash", input(), None, None));
        ap(&s, done(Some("aborted_streaming")));
        assert!(s.with_state(|st| st.status == SessionStatus::Running && st.pending.is_empty()), "interrupt while waiting: queued turn runs");
        ap(&s, queue("u11", "started"));
        assert!(s.with_state(|st| st.status == SessionStatus::Running) && user_last(&s) == ("onze".into(), false), "started after a waiting interrupt");
        ap(&s, done(None));

        s.apply(|st| st.turn_ultra = true);
        begin(&s, "douze", "u12");
        enqueue("u13", "treize");
        enqueue("u14", "quatorze");
        ap(&s, tool_use("t12", "Bash", input(), None));
        ap(&s, tool_result("t12", "ok", false, None));
        ap(&s, queue("u13", "started"));
        assert_eq!(user_last(&s), ("treize".into(), true), "folded into an ultracode turn: labelled ultracode");
        ap(&s, done(None));
        s.apply(|st| st.turn_ultra = false); // on_event resets it after the reducer
        ap(&s, queue("u14", "started"));
        assert_eq!(user_last(&s), ("quatorze".into(), true), "started right after an ultracode turn: labelled ultracode");
        ap(&s, done(None));
        begin(&s, "quinze", "u15");
        enqueue("u16", "seize");
        ap(&s, done(None));
        ap(&s, queue("u16", "started"));
        assert!(!user_last(&s).1, "after a plain turn: not ultracode");
    }

    #[test]
    fn rewind_cut() {
        let rw = ls("r", "C:\\w", "default");
        begin(&rw, "un", "u1");
        ap(&rw, text("ok", None, false, None));
        begin(&rw, "deux", "u2");
        ap(&rw, tool_use("t9", "Write", input(), None));
        ap(&rw, result("success", false, "0.01", 0, None));
        begin(&rw, "trois", "u3");
        assert_eq!(rw.with_state(|st| ccui_shared::session_rules::rewind_steps(st.items(), "u2")), vec!["u3", "u2"]);
        rw.apply(|st| st.truncate("u2", None));
        assert!(matches!(&items(&rw)[..], [Item::User(u), Item::Text(_)] if u.uuid.as_deref() == Some("u1")), "rewind cuts from the target message");
        assert!(!rw.gate.lock().can_rewind("u1"), "no rewind without a process that knows the uuid");
    }

    #[test]
    fn resumed_cost_and_background_agents() {
        // Opened from Récentes at 0.0127; --resume restores that cost-state and reports 0.0165 after one turn.
        let o = live_with(SessionInit { id: "o".into(), cwd: "C:\\w".into(), mode: "default".into(), resumable: true, cost_usd: d("0.0127"), ..Default::default() }, rec());
        o.apply(|st| st.cost_base = st.cost_usd - d("0.0127"));
        ap(&o, result("success", false, "0.0165", 0, None));
        assert!(o.with_state(|st| st.cost_usd == d("0.0165") && st.last_turn_cost_usd == d("0.0038")), "resumed cost not counted twice");

        let ag = ls("a", "C:\\w", "default");
        let recv = json!({"isAsync":true,"status":"async_launched","agentId":"aa5"});
        begin(&ag, "agent", "");
        ap(&ag, tool_use("toolu_A", "Agent", input(), None));
        ap(&ag, tool_result("toolu_A", "Async agent launched", false, Some(recv.clone())));
        assert!(matches!(last(&ag), Item::Tool(t) if t.state == ToolState::Running && t.ended_at.is_none() && t.background && t.task_id.as_deref() == Some("aa5")), "async agent stays running");
        ap(&ag, result("success", false, "0", 0, None));
        assert!(matches!(last(&ag), Item::Tool(t) if t.state == ToolState::Running), "background row survives the turn");
        let p = PendingPermission {
            request_id: "rq".into(),
            tool: "Write".into(),
            input: input(),
            description: None,
            tool_use_id: Some("toolu_X".into()),
            suggestions: None,
            at: chrono::Utc::now(),
            info: Some(PermissionInfo { agent_id: Some("aa5".into()), ..Default::default() }),
            target_exists: None,
        };
        assert_eq!(ag.with_state(|st| ccui_shared::session_rules::asking_agent(st.items(), &p).map(|t| t.id.clone())).as_deref(), Some("toolu_A"), "background subagent's request names its agent");
        let n = items(&ag).len();
        ap(&ag, E::TaskDone(TaskDone { task_id: "aa5".into(), tool_use_id: "toolu_A".into(), status: "completed".into(), result: Some("pong".into()), tokens: 31599, tool_uses: 1, duration_ms: 5088 }));
        let at = tool(&ag, "toolu_A");
        assert!(items(&ag).len() == n && at.state == ToolState::Done && at.result_text.as_deref() == Some("pong") && at.tokens == 31599 && at.sub_tool_uses == 1);
        assert_eq!((at.ended_at.unwrap() - at.started_at).num_milliseconds(), 5088, "notification completes the row");
        let ag2 = ls("b", "C:\\w", "default");
        ap(&ag2, tool_use("toolu_B", "Agent", input(), None));
        ap(&ag2, tool_result("toolu_B", "Async agent launched", false, Some(recv)));
        ag2.apply(|st| st.end_tools(chrono::Utc::now(), false));
        assert_eq!(tool(&ag2, "toolu_B").state, ToolState::Error, "process exit ends background row");
    }

    #[test]
    fn workflow_runs_past_its_turn() {
        let wf = ls("w", "/w", "default");
        let snap = json!([{"type":"workflow_phase","index":1,"title":"Ping"}]);
        begin(&wf, "wf", "");
        ap(&wf, tool_use("toolu_W", "Workflow", input(), None));
        ap(&wf, task_started("w0wlsjjl5", "toolu_W", "Two agents reply pong", "local_workflow", false, false));
        ap(&wf, tool_result("toolu_W", "Workflow launched in background. Task ID: w0wlsjjl5", false,
            Some(json!({"status":"async_launched","taskId":"w0wlsjjl5","taskType":"local_workflow","workflowName":"pong-probe","runId":"wf_8e162ae6-279"}))));
        ap(&wf, E::TaskProgress { task_id: "w0wlsjjl5".into(), total_tokens: 0, tool_uses: 0, duration_ms: 20, workflow: Some(snap) });
        ap(&wf, result("success", false, "0", 0, None));
        ap(&wf, E::TaskProgress { task_id: "w0wlsjjl5".into(), total_tokens: 49076, tool_uses: 0, duration_ms: 3483, workflow: None });
        let t = tool(&wf, "toolu_W");
        assert!(t.state == ToolState::Running && t.background && t.task_id.as_deref() == Some("w0wlsjjl5") && t.tokens == 49076 && t.progress.is_some() && t.result_text.is_none());
        assert!(wf.with_state(|st| st.tasks.is_empty()), "a workflow has its own row");
        ap(&wf, E::TaskDone(TaskDone { task_id: "w0wlsjjl5".into(), tool_use_id: "toolu_W".into(), status: "completed".into(), result: None, tokens: 49076, tool_uses: 0, duration_ms: 3493 }));
        let t = tool(&wf, "toolu_W");
        assert!(t.state == ToolState::Done && t.task_status.as_deref() == Some("completed") && t.progress.is_some(), "workflow notification");
    }

    #[tokio::test]
    async fn plan_elicitation_plugins_reload() {
        let plan = json!({"plan":"# P\n\n1. a","planFilePath":"/p.md"});
        let pm = ls("p", "C:\\w", "plan");
        begin(&pm, "plan it", "");
        ap(&pm, tool_use("toolu_P", "ExitPlanMode", plan.clone(), None));
        ap(&pm, perm("rp", "ExitPlanMode", plan, None, Some("toolu_P")));
        pm.apply(|st| st.resolve("rp", Decision::Allow));
        ap(&pm, status(None, Some("acceptEdits")));
        assert!(pm.with_state(|st| st.mode == "acceptEdits" && st.status == SessionStatus::Running));
        assert!(matches!(last(&pm), Item::Tool(t) if t.state == ToolState::Running), "plan approved, mode follows status");

        // An MCP elicitation queues like a permission and pauses the turn; a form one is never accepted without content.
        let el = ls("el", "C:\\w", "default");
        begin(&el, "ask", "");
        let form = json!({"subtype":"elicitation","mcp_server_name":"probe","message":"Name?","mode":"form","requested_schema":{"type":"object","properties":{"name":{"type":"string"}}}});
        ap(&el, perm("e1", ccui_shared::elicit::TOOL, form, Some("Name?"), None));
        assert!(el.with_state(|st| st.status == SessionStatus::Waiting && st.pending.len() == 1 && st.pending[0].description.as_deref() == Some("Name?")));
        let allow = |rid: &str, d: Decision| AnswerArgs { request_id: rid.into(), decision: d, mode: None, message: None, input: None, destination: None };
        assert_eq!(el.answer(allow("e1", Decision::Allow)).await, Err(ccui_shared::t!("Ask.NeedsAnswers")), "form elicitation never accepted bare");
        ap(&el, E::CancelRequest { request_id: "e1".into() });
        assert!(el.with_state(|st| st.pending.is_empty() && st.status == SessionStatus::Running), "elicitation withdrawn by the CLI");
        ap(&el, E::PluginStatus { plugin: "demo".into(), text: Some("syncing".into()) });
        ap(&el, E::PluginStatus { plugin: "demo".into(), text: Some("synced".into()) });
        assert!(el.with_state(|st| st.plugin_status.len() == 1 && st.plugin_status["demo"] == "synced"), "plugin status replaced");
        ap(&el, E::PluginStatus { plugin: "demo".into(), text: None });
        assert!(el.with_state(|st| st.plugin_status.is_empty()), "plugin status cleared");
        let gone = ls("g", "/w", "default");
        ap(&gone, E::PluginStatus { plugin: "demo".into(), text: Some("syncing".into()) });
        assert!(gone.on_exit(None, Some(1), "x".into()));
        assert!(gone.with_state(|st| st.plugin_status.is_empty() && st.status == SessionStatus::Crashed && st.start_failed), "plugin status dropped when the process ends");
        el.apply(|st| st.heavy.initialize_info = Some(json!({"commands":[{"name":"old"}],"agents":[],"models":[{"value":"haiku"}]})));
        ap(&el, E::Commands(json!([{"name":"demo:hello","description":"say hi"}])));
        let info = el.with_state(|st| st.heavy.initialize_info.clone().unwrap());
        assert!(info["commands"][0]["name"] == "demo:hello" && info.get("models").is_some(), "commands_changed");
        ap(&el, E::Init(InitInfo { session_id: "s".into(), agents: vec!["old".into()], plugins: vec!["gone".into()], fast_mode_state: "off".into(), ..Default::default() }));
        el.apply(|st| st.reloaded(
            &json!({"commands":[],"agents":[{"name":"claude","description":"Catch-all"}],"plugins":[{"name":"context7","path":"/p","source":"context7@claude-plugins-official"}],"mcpServers":[{"name":"plugin:context7:context7","status":"needs-auth","config":{"type":"http","url":"https://mcp.context7.com/mcp"},"scope":"dynamic","source":"plugin"}],"error_count":1}),
            &json!({"skills":[{"name":"demo:hello","description":"say hi","argumentHint":""}]}),
        ));
        el.with_state(|st| {
            let i = st.heavy.init.as_ref().unwrap();
            assert!(i.plugins == ["context7"] && i.agents == ["claude"] && i.skills == ["demo:hello"]);
            assert_eq!(ccui_shared::mcp::names(&st.heavy), vec!["plugin:context7:context7"]);
            assert_eq!(st.heavy.initialize_info.as_ref().unwrap()["agents"].as_array().unwrap().len(), 1, "reload refreshes the lists");
        });
        ap(&el, E::Notice(NoticeEvt { text: "Error compacting conversation".into(), color: Some("error".into()) }));
        assert!(matches!(last(&el), Item::Notice(n) if n.notice.color.as_deref() == Some("error")), "notification joins the thread");

        // AskUserQuestion is never allowed bare; checked before the missing process.
        let s = ls("ask", "C:\\w", "default");
        ap(&s, perm("r", ccui_shared::ask_user::TOOL, input(), None, None));
        assert_eq!(s.answer(allow("r", Decision::Allow)).await, Err(ccui_shared::t!("Ask.NeedsAnswers")));
        assert_eq!(s.answer(allow("r", Decision::Deny)).await, Err(ccui_shared::t!("Session.NoProcess")));
    }

    #[test]
    fn background_shells() {
        let bg = ls("bg", "C:\\w", "default");
        begin(&bg, "bg", "");
        ap(&bg, tool_use("toolu_S", "Bash", json!({"command":"sleep 60","run_in_background":true}), None));
        ap(&bg, task_started("b1", "toolu_S", "ticks", "local_bash", true, false));
        ap(&bg, task_started("a1", "toolu_X", "agent", "local_agent", true, false));
        ap(&bg, tool_result("toolu_S", "Command running in background with ID: b1.", false, None));
        ap(&bg, result("success", false, "0", 0, None));
        let tasks = bg.with_state(|st| st.tasks.clone());
        assert!(tasks.len() == 1 && tasks[0].id == "b1" && tasks[0].tool_use_id == "toolu_S" && state::task_running(&tasks[0]) && tasks[0].ended_at.is_none());
        assert!(matches!(last(&bg), Item::Tool(t) if t.state == ToolState::Done && t.task_id.as_deref() == Some("b1")), "background shell listed, survives the turn");
        ap(&bg, E::TaskUpdated { task_id: "b1".into(), status: None, ended_at: None, backgrounded: false });
        assert!(bg.with_state(|st| state::task_running(&st.tasks[0])), "status-less patch ignored");
        let stop_at = chrono::DateTime::from_timestamp_millis(1791585746931).unwrap();
        ap(&bg, E::TaskUpdated { task_id: "b1".into(), status: Some("killed".into()), ended_at: Some(stop_at), backgrounded: false });
        ap(&bg, E::TaskDone(TaskDone { task_id: "b1".into(), tool_use_id: "toolu_S".into(), status: "stopped".into(), ..Default::default() }));
        let k = bg.with_state(|st| st.tasks[0].clone());
        assert!(k.status == "killed" && !state::task_running(&k) && k.ended_at == Some(stop_at));
        assert!(matches!(last(&bg), Item::Tool(t) if t.state == ToolState::Done), "stopped task");
        ap(&bg, task_started("b2", "toolu_T", "watch", "local_bash", true, false));
        ap(&bg, task_started("b3", "toolu_U", "fg", "local_bash", false, false));
        bg.apply(|st| st.end_tools(chrono::Utc::now(), false));
        let t = bg.with_state(|st| st.tasks.clone());
        assert!(t.len() == 2 && t[0].status == "killed" && t[1].id == "b2" && t[1].status == "killed" && t[1].ended_at.is_some(), "process exit ends running tasks, foreground shells not listed");
        ap(&bg, task_started("b4", "toolu_V", "live", "local_bash", true, false));
        let cv = bg.version();
        bg.clear_ended_tasks();
        assert!(bg.with_state(|st| st.tasks.len() == 1 && st.tasks[0].id == "b4") && bg.version() != cv, "clear drops ended tasks only and bumps Version");
        // Foreground, then background_tasks: task_updated {is_backgrounded}, then the tool's result.
        begin(&bg, "fg", "");
        ap(&bg, tool_use("toolu_F", "Bash", json!({"command":"make","description":"Build"}), None));
        ap(&bg, task_started("b5", "toolu_F", "Build", "local_bash", false, false));
        ap(&bg, E::TaskUpdated { task_id: "b5".into(), status: None, ended_at: None, backgrounded: true });
        ap(&bg, tool_result("toolu_F", "Command was manually backgrounded by user with ID: b5.", false, None));
        let t = bg.with_state(|st| st.tasks.clone());
        assert!(t.len() == 2 && t[1].id == "b5" && t[1].description == "Build" && state::task_running(&t[1]));
        assert!(matches!(last(&bg), Item::Tool(t) if t.background), "shell moved to the background joins the tasks");
        ap(&bg, task_started("m1", "toolu_M", "github/search", "mcp_task", false, false));
        ap(&bg, task_started("d1", "", "scan", "auto_mode_scan", false, true));
        ap(&bg, task_started("w1", "toolu_W", "wf", "local_workflow", false, false));
        let t = bg.with_state(|st| st.tasks.clone());
        assert!(t.last().is_some_and(|x| x.id == "m1" && x.kind.as_deref() == Some("mcp_task")) && t[t.len() - 2].kind.is_none());
        assert!(!t.iter().any(|x| x.id == "d1" || x.id == "w1"), "MCP task listed by type, ambient and workflow not");
    }

    #[test]
    fn subagent_model_scheduled_worktrees_resume_pr() {
        let sm = ls("sm", "/w", "default");
        ap(&sm, E::ToolUse { id: "toolu_A".into(), name: "Agent".into(), input: input(), parent_tool_use_id: None, model: Some("claude-opus-5-5".into()) });
        ap(&sm, E::AssistantText { message_id: "m1".into(), text: "hi".into(), parent_tool_use_id: Some("toolu_A".into()), synthetic: false, uuid: None, at: None, model: Some("claude-haiku-5-5".into()) });
        ap(&sm, E::ToolUse { id: "t1".into(), name: "Read".into(), input: input(), parent_tool_use_id: Some("toolu_A".into()), model: Some("claude-sonnet-5".into()) });
        assert!(tool(&sm, "toolu_A").model.as_deref() == Some("claude-haiku-5-5") && tool(&sm, "t1").model.is_none(), "subagent model from its first message");
        ap(&sm, E::Scheduled { prompt: "tick".into(), at: None });
        assert!(matches!(last(&sm), Item::Scheduled(s) if s.prompt == "tick"), "scheduled prompt marker");

        let wm = live_with(SessionInit { id: "w".into(), cwd: "/r".into(), mode: "default".into(), worktree: Some("x".into()), ..Default::default() }, rec());
        begin(&wm, "go", "");
        ap(&wm, tool_use("toolu_E", "EnterWorktree", json!({"name":"y"}), None));
        ap(&wm, tool_result("toolu_E", "Created worktree at /r/.claude/worktrees/y on branch worktree-y.", false,
            Some(json!({"worktreePath":"/r/.claude/worktrees/y","worktreeBranch":"worktree-y","message":"Created worktree at /r/.claude/worktrees/y on branch worktree-y."}))));
        assert_eq!(wm.with_state(|st| st.cwd.clone()), "/r/.claude/worktrees/y", "EnterWorktree moves the session");
        ap(&wm, tool_use("toolu_S", "EnterWorktree", json!({"path":"/r/.claude/worktrees/z"}), Some("toolu_agent")));
        ap(&wm, tool_result("toolu_S", "Entered worktree", false, Some(json!({"worktreePath":"/r/.claude/worktrees/z","message":"Entered worktree"}))));
        assert_eq!(wm.with_state(|st| st.cwd.clone()), "/r/.claude/worktrees/y", "a subagent's EnterWorktree moves only that agent");
        ap(&wm, tool_use("toolu_X", "ExitWorktree", json!({"action":"keep"}), None));
        ap(&wm, tool_result("toolu_X", "Exited worktree.", false,
            Some(json!({"action":"keep","originalCwd":"/r","worktreePath":"/r/.claude/worktrees/y","restoredCwd":"/r","message":"Exited worktree."}))));
        assert!(wm.with_state(|st| st.cwd == "/r" && st.worktree.is_none() && !st.isolated()), "ExitWorktree moves it back, no longer isolated");

        let rt = live_with(SessionInit {
            id: "z".into(),
            cwd: "/w".into(),
            mode: "default".into(),
            items: vec![Item::User(UserItem { text: "count".into(), at: chrono::Utc::now(), ultracode: false, images: None, uuid: None })],
            ..Default::default()
        }, rec());
        rt.apply(|st| {
            st.status = SessionStatus::Starting;
            st.resume_turn = true;
        });
        ap(&rt, queue("1967353c-03fe-4c2a-8c3b-72e2b6697258", "started"));
        ap(&rt, status(Some("requesting"), None));
        assert!(rt.with_state(|st| st.status == SessionStatus::Running && st.turn_started_at.is_some()));
        assert!(matches!(&items(&rt)[..], [Item::User(_), Item::Resumed]), "re-run of the cut-off turn starts a turn, labelled once");
        ap(&rt, E::Pr { url: "https://github.com/o/r/pull/42".into(), number: Some("42".into()) });
        assert_eq!(rt.with_state(|st| st.pr_url.clone()).as_deref(), Some("https://github.com/o/r/pull/42"), "code_change_published links the PR");
    }

    #[test]
    fn export_file_never_over_a_file_never_out_of_the_folder() {
        let dl = tempfile::tempdir().unwrap();
        std::fs::write(dl.path().join("a.txt"), "").unwrap();
        assert_eq!(commands::free_file(dl.path(), Some("a.txt")), dl.path().join("a-2.txt"));
        assert_eq!(commands::free_file(dl.path(), Some("../../x.txt")), dl.path().join("x.txt"));
        assert_eq!(commands::free_file(dl.path(), None), dl.path().join("conversation.txt"));
        std::fs::write(dl.path().join("a-2.txt"), "").unwrap();
        assert_eq!(commands::free_file(dl.path(), Some("a.txt")), dl.path().join("a-3.txt"));
    }

    // ---------------------------------------------------------------- state sync (docs/ARCHITECTURE.md §2)

    #[test]
    fn patches_only_for_watched_sessions_with_dirty_items() {
        let sink = rec();
        let s = live_with(SessionInit { id: "p".into(), cwd: "/w".into(), mode: "default".into(), ..Default::default() }, sink.clone());
        begin(&s, "un", "u1");
        s.raise();
        assert!(sink.0.lock().is_empty(), "unwatched: nothing built");
        let snap = s.watch();
        assert_eq!(snap.items.len(), 1);
        assert_eq!(snap.version, s.version());
        s.raise();
        assert!(sink.0.lock().is_empty(), "nothing moved since the snapshot");
        ap(&s, text("ok", None, false, None));
        ap(&s, tool_use("t", "Bash", input(), None));
        s.raise();
        let pushes = std::mem::take(&mut *sink.0.lock());
        let Some(Push::SessionPatch(p)) = pushes.first() else { panic!("a patch: {pushes:?}") };
        assert_eq!((p.base_version, p.version, p.len), (snap.version, s.version(), 3));
        assert_eq!(p.items.iter().map(|(i, _)| *i).collect::<Vec<_>>(), vec![1, 2]);
        assert!(p.heavy.is_none() && p.draft.is_none(), "heavy and draft only when they changed");
        // A rewind cut: the patch truncates and carries the restored draft.
        s.apply(|st| st.cut_back("u1", None, None));
        s.raise();
        let pushes = std::mem::take(&mut *sink.0.lock());
        let Some(Push::SessionPatch(p)) = pushes.first() else { panic!() };
        assert_eq!(p.len, 0);
        assert!(p.items.is_empty());
        assert_eq!(p.draft.as_ref().map(|d| d.text.as_str()), Some("un"));
        // Draft writes from the UI: stale rev refused, never echoed.
        assert!(!s.set_draft_text("old".into(), 0), "stale rev");
        let v = s.version();
        assert!(s.set_draft_text("new".into(), 1));
        assert_eq!(s.version(), v, "a UI write does not bump the version");
        s.apply(|st| st.heavy.context = Some(json!({"x":1})));
        s.raise();
        let pushes = std::mem::take(&mut *sink.0.lock());
        assert!(matches!(pushes.first(), Some(Push::SessionPatch(p)) if p.heavy.is_some() && p.draft.is_none()));
        s.unwatch();
        ap(&s, text("later", None, false, None));
        s.raise();
        assert!(sink.0.lock().is_empty(), "unwatched again");
    }

    #[test]
    fn stream_offsets() {
        let sink = rec();
        let s = live_with(SessionInit { id: "st".into(), cwd: "/w".into(), mode: "default".into(), ..Default::default() }, sink.clone());
        s.watch();
        ap(&s, E::TextDelta { text: "je ".into(), parent_tool_use_id: None });
        s.raise();
        ap(&s, E::TextDelta { text: "crée".into(), parent_tool_use_id: None });
        s.raise();
        ap(&s, text("je crée", None, false, None)); // clears the stream
        s.raise();
        let streams: Vec<SessionStream> =
            sink.0.lock().iter().filter_map(|p| if let Push::SessionStream(x) = p { Some(x.clone()) } else { None }).collect();
        assert_eq!(streams.iter().map(|x| (x.offset, x.append.as_str())).collect::<Vec<_>>(), vec![(0, "je "), (3, "crée"), (0, "")]);
    }

    #[test]
    fn throttle_coalesces_and_urgent_events_go_at_once() {
        let sink = rec();
        let s = live_with(SessionInit { id: "th".into(), cwd: "/w".into(), mode: "default".into(), ..Default::default() }, sink.clone());
        s.watch();
        begin(&s, "go", "");
        s.notify();
        let n = sink.0.lock().len();
        for i in 0..5 {
            s.on_event(json!({"type":"assistant","message":{"id":"m","content":[{"type":"text","text":format!("t{i}")}]}}));
        }
        assert_eq!(sink.0.lock().len(), n, "within 50 ms of the last raise: deferred");
        s.on_event(json!({"type":"result","subtype":"success","is_error":false,"duration_ms":10,"num_turns":1,"total_cost_usd":0}));
        let pushes = sink.0.lock().clone();
        assert!(matches!(pushes.last(), Some(Push::SessionPatch(p)) if p.items.len() == 5 && p.meta.status == SessionStatus::Idle), "a result goes at once, with everything before it");
        std::thread::sleep(Duration::from_millis(120));
        assert_eq!(sink.0.lock().len(), pushes.len(), "the trailing raise finds nothing new");
    }

    #[test]
    fn summary_row() {
        let s = ls("sum", "/w", "default");
        begin(&s, "go", "");
        ap(&s, tool_use("t1", "Bash", json!({"command":"ls"}), None));
        ap(&s, tool_use("t2", "Read", input(), Some("t1")));
        let r = s.summary();
        assert!(r.items_len == 3 && r.tool_count == 2 && r.last_tool.as_ref().is_some_and(|t| t.name == "Bash") && r.todos.is_none());
        ap(&s, perm("r", "Write", input(), None, None));
        let r = s.summary();
        assert!(r.pending_count == 1 && r.first_pending_tool.as_deref() == Some("Write") && r.last_tool.unwrap().name == "Write");
    }

    /// End to end against the real CLI (haiku): spawn, boot, one turn, result, cost. `cargo test -- --ignored`.
    #[tokio::test(flavor = "multi_thread")]
    #[ignore = "drives the real claude CLI (network, account)"]
    async fn real_claude_haiku_turn() {
        let dir = tempfile::tempdir().unwrap();
        let sink = rec();
        let s = live_with(
            SessionInit {
                id: uuid::Uuid::new_v4().to_string(),
                name: "ccui-it".into(),
                cwd: dir.path().to_string_lossy().into_owned(),
                mode: "default".into(),
                model: Some("haiku".into()),
                ..Default::default()
            },
            sink.clone(),
        );
        s.watch();
        s.send("Reply with the single word pong.".into(), vec![]).await.unwrap();
        let deadline = std::time::Instant::now() + Duration::from_secs(180);
        while s.with_state(|st| st.last_result_at.is_none()) {
            assert!(std::time::Instant::now() < deadline, "no result in 3 min");
            tokio::time::sleep(Duration::from_millis(200)).await;
        }
        s.with_state(|st| {
            assert_eq!(st.status, SessionStatus::Idle);
            assert!(st.items().iter().any(|i| matches!(i, Item::Text(t) if t.markdown.to_lowercase().contains("pong"))));
            assert!(st.heavy.initialize_info.is_some(), "boot answered initialize");
        });
        assert!(sink.0.lock().iter().any(|p| matches!(p, Push::SessionPatch(_))));
        s.dispose().await;
        assert_eq!(s.with_state(|st| st.status), SessionStatus::Exited);
    }
}
