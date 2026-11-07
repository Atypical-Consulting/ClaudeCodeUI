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
use ccui_shared::model::{Item, Time, UserImage};
use ccui_shared::session::{SessionDraft, SessionHeavy, SessionMeta, SessionPatch, SessionSnapshot, SessionStream, SessionSummary};
use parking_lot::Mutex;
use rust_decimal::Decimal;
use state::SessionState;
use std::collections::HashSet;
use std::sync::atomic::{AtomicBool, AtomicI64};
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

pub struct LiveSession {
    id: String,
    pub(crate) gate: Mutex<Inner>,
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

    /// `Inner.version`, read under the gate.
    pub fn version(&self) -> u64 {
        todo!("port: LiveSession.Version")
    }

    /// Read the state under the gate (never await inside).
    pub fn with_state<R>(&self, f: impl FnOnce(&SessionState) -> R) -> R {
        todo!("port: (lock gate, read)")
    }

    /// THE way to change state: lock, run `f`, `version += 1`, unlock (the caller then notify()/throttled()).
    /// Streamed text only (`text_delta`) uses `with_state_mut`, which does not bump.
    pub(crate) fn apply<R>(&self, f: impl FnOnce(&mut SessionState) -> R) -> R {
        todo!("port: (lock gate, mutate, Version++)")
    }

    /// Mutate without bumping the version (streamed text, draft writes from the UI).
    pub(crate) fn with_state_mut<R>(&self, f: impl FnOnce(&mut SessionState) -> R) -> R {
        todo!("port: (lock gate, mutate)")
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

    /// Full state for first render: version, meta, heavy, draft, items and streaming text in ONE lock.
    pub fn snapshot(&self) -> SessionSnapshot {
        todo!("port: (session_snapshot)")
    }

    /// `session_watch` / `session_open`: watchers += 1 and the snapshot, in the same lock (nothing can slip between).
    pub fn watch(&self) -> SessionSnapshot {
        todo!("port: (session_watch)")
    }

    /// `session_unwatch`: watchers −= 1 (saturating). At 0 patches and streams stop being built.
    pub fn unwatch(&self) {
        todo!("port: (session_unwatch)")
    }

    /// Under the gate: None when version == pushed_version. Unwatched → clear the dirty set, pushed_version =
    /// version, None. Else `SessionPatch { base_version: pushed_version, version, meta, heavy if changed, draft if
    /// rev changed, items: take_dirty(), len }` and record what was pushed.
    pub(crate) fn take_patch(&self) -> Option<SessionPatch> {
        todo!("port: (patch building)")
    }

    /// Under the gate, watched only: the `SessionStream` increment when the streaming text moved (see
    /// `Inner.stream_pushed`), else None.
    pub(crate) fn take_stream(&self) -> Option<SessionStream> {
        todo!("port: (stream increment)")
    }

    /// `notify()`: raise now (permission requests, results, commands). The version was already bumped by the
    /// locked mutation that called it.
    pub(crate) fn notify(&self) {
        todo!("port: LiveSession.Notify")
    }

    /// `throttled()`: raise now if 50 ms passed since the last one, else schedule one raise after the remainder.
    pub(crate) fn throttled(self: &Arc<Self>) {
        todo!("port: LiveSession.Throttled")
    }

    /// Emit `take_patch()` and/or `take_stream()` when Some; call the change hook with the current version.
    pub(crate) fn raise(&self) {
        todo!("port: LiveSession.Raise / Changed")
    }
}
