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
//! changed, then calls the manager's change hook.

pub mod args;
pub mod commands;
pub mod lifecycle;
pub mod reducer;
pub mod state;

use crate::process::ClaudeProcess;
use crate::UiSink;
use ccui_shared::model::{Item, Time, UserImage};
use ccui_shared::session::{SessionMeta, SessionSnapshot, SessionSummary};
use parking_lot::Mutex;
use rust_decimal::Decimal;
use state::SessionState;
use std::collections::HashSet;
use std::sync::atomic::{AtomicBool, AtomicI64, AtomicU64};
use std::sync::Arc;

/// Manager hook: `(session id, version)` after every raise (the manager relays app-wide only when version moved).
pub type ChangeHook = Arc<dyn Fn(&str, u64) + Send + Sync>;

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
    /// Streaming text last pushed (a stream push only when it changed).
    pub stream_pushed: String,
}

pub struct LiveSession {
    id: String,
    pub(crate) gate: Mutex<Inner>,
    /// Bumped by every change except streamed text.
    pub(crate) version: AtomicU64,
    /// Version of the last SessionPatch pushed.
    pub(crate) pushed_version: AtomicU64,
    pub(crate) last_notify_ms: AtomicI64,
    pub(crate) notify_queued: AtomicBool,
    pub(crate) status_running: AtomicBool,
    pub(crate) titling: AtomicBool,
    pub(crate) sink: Arc<dyn UiSink>,
    pub(crate) on_changed: ChangeHook,
}

impl LiveSession {
    /// `new LiveSession(...) { init-only fields }`; started_at = last_event_at = now unless given.
    pub fn new(init: SessionInit, sink: Arc<dyn UiSink>, on_changed: ChangeHook) -> Arc<LiveSession> {
        todo!("port: LiveSession constructor")
    }

    pub fn id(&self) -> &str {
        &self.id
    }

    pub fn version(&self) -> u64 {
        todo!("port: LiveSession.Version")
    }

    /// Read the state under the gate (never await inside).
    pub fn with_state<R>(&self, f: impl FnOnce(&SessionState) -> R) -> R {
        todo!("port: (lock gate, read)")
    }

    pub fn has_process(&self) -> bool {
        todo!("port: LiveSession.HasProcess")
    }

    /// The read model without items.
    pub fn meta(&self) -> SessionMeta {
        todo!("port: LiveSession public state → SessionMeta")
    }

    /// One rail/palette/Overview row.
    pub fn summary(&self) -> SessionSummary {
        todo!("port: (SessionManager readers of LiveSession)")
    }

    /// Full state for first render.
    pub fn snapshot(&self) -> SessionSnapshot {
        todo!("port: (session_snapshot)")
    }

    /// `notify()`: version += 1 and raise now (permission requests, results, commands).
    pub(crate) fn notify(&self) {
        todo!("port: LiveSession.Notify")
    }

    /// `throttled()`: raise now if 50 ms passed since the last one, else schedule one raise after the remainder.
    pub(crate) fn throttled(self: &Arc<Self>) {
        todo!("port: LiveSession.Throttled")
    }

    /// Emit SessionPatch (version moved: dirty items + meta + len) and/or SessionStream (text changed); call the hook.
    pub(crate) fn raise(&self) {
        todo!("port: LiveSession.Raise / Changed")
    }
}
