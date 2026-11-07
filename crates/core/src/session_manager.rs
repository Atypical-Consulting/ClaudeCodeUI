//! Port of `Core/SessionManager.cs` — every live session of the app; kills every claude process on shutdown.
//! Spec 03 §2. Owner: core-store-sessions.
//!
//! `all` is in creation order, never re-sorted, copy-on-write. Each session's change hook relays app-wide only when
//! that session's version moved; streamed text alone concerns the session page only. On a moved version the
//! notification observer runs synchronously, and `Push::Sessions` is THROTTLED app-wide to ≤ 1 per 50 ms (trailing:
//! a raise inside the window schedules one emit at its end) — the summary is rebuilt from every session
//! (TodoList, last-tool scan), never per raise.

use crate::live::{LiveSession, SessionInit};
use crate::notifications::NotifyObserver;
use crate::UiSink;
use ccui_shared::api::{PastSession, SavedLimits, StartArgs};
use ccui_shared::model::RateLimits;
use ccui_shared::session::{DecisionEntry, SessionsSummary};
use parking_lot::{Mutex, RwLock};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::sync::Arc;

/// `limits.json` as the C# app wrote it (System.Text.Json PascalCase, `default(DateTimeOffset)` =
/// "0001-01-01T00:00:00+00:00" for a missing window). Converted to/from `ccui_shared::api::SavedLimits`.
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "PascalCase")]
pub struct SavedLimitsFile {
    pub limits: RateLimitsFile,
    pub at: chrono::DateTime<chrono::FixedOffset>,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "PascalCase")]
pub struct RateLimitsFile {
    pub five_hour: f64,
    pub five_hour_reset: chrono::DateTime<chrono::FixedOffset>,
    pub seven_day: f64,
    pub seven_day_reset: chrono::DateTime<chrono::FixedOffset>,
    pub status: String,
    #[serde(rename = "Type")]
    pub kind: Option<String>,
    pub extra_on: bool,
    pub extra_in_use: bool,
    pub extra_off: Option<String>,
}

pub struct SessionManager {
    all: RwLock<Vec<Arc<LiveSession>>>,
    /// Serialises `open` (two windows opening the same past session would add it twice).
    open_gate: tokio::sync::Mutex<()>,
    /// Last relayed version per session id.
    seen_versions: Mutex<HashMap<String, u64>>,
    /// Unix ms of the last `Push::Sessions` emit (summary throttle).
    summary_last_ms: std::sync::atomic::AtomicI64,
    /// A trailing summary emit is scheduled.
    summary_queued: std::sync::atomic::AtomicBool,
    notifier: NotifyObserver,
    sink: Arc<dyn UiSink>,
}

impl SessionManager {
    pub fn new(sink: Arc<dyn UiSink>) -> Arc<SessionManager> {
        todo!("port: SessionManager constructor")
    }

    pub fn all(&self) -> Vec<Arc<LiveSession>> {
        todo!("port: SessionManager.All")
    }

    pub fn get(&self, id: &str) -> Option<Arc<LiveSession>> {
        todo!("port: SessionManager.Get")
    }

    /// status != Exited.
    pub fn active(&self) -> Vec<Arc<LiveSession>> {
        todo!("port: SessionManager.Active")
    }

    pub fn is_active(&self, id: &str) -> bool {
        todo!("port: SessionManager.IsActive")
    }

    /// `Get(id)?.HasProcess != true && HeldByClaude().contains(id)`.
    pub fn running_elsewhere(&self, id: &str) -> bool {
        todo!("port: SessionManager.RunningElsewhere")
    }

    /// Every (session, pending) across sessions, by arrival time; `agent_label` =
    /// `session_rules::agent_label(asking_agent(items, p))` read under each session's gate.
    pub fn decision_queue(&self) -> Vec<DecisionEntry> {
        todo!("port: SessionManager.DecisionQueue")
    }

    /// The app-wide summary (sessions, queue, latest limits, usage source, held ids).
    pub fn summary(&self) -> SessionsSummary {
        todo!("port: SessionManager.Limits / UsageSource + Rail/Mascot readers")
    }

    /// Write `limits.json` `{Limits, At: now}`; errors swallowed.
    pub fn remember_limits(limits: &RateLimits) {
        todo!("port: SessionManager.RememberLimits")
    }

    /// Read `limits.json` (missing / IO / bad JSON → None).
    pub fn last_limits() -> Option<SavedLimits> {
        todo!("port: SessionManager.LastLimits")
    }

    /// New session (uuid v4, Untitled), record add-dirs, spawn in a catch-all (a failed start leaves it Crashed),
    /// refresh git.
    pub async fn start(self: &Arc<Self>, args: StartArgs) -> Arc<LiveSession> {
        todo!("port: SessionManager.Start")
    }

    /// History now, `--resume` process on first Send/Restart. Under `open_gate`; already live → it.
    pub async fn open(self: &Arc<Self>, past: PastSession) -> Arc<LiveSession> {
        todo!("port: SessionManager.Open")
    }

    /// `Get(id) ?? TranscriptStore.ById(id) → Open` (deep links, past rows, reloads). Invalid id → None.
    pub async fn open_id(self: &Arc<Self>, id: &str) -> Option<Arc<LiveSession>> {
        todo!("port: SessionPage resolve (Get ?? ById → Open)")
    }

    /// Continue `src` under a new id (an unsent fork forks its source at the same cut). Returns the fork.
    pub async fn fork(self: &Arc<Self>, src: &Arc<LiveSession>, at: Option<String>) -> Arc<LiveSession> {
        todo!("port: SessionManager.Fork")
    }

    /// Remove, dispose, forget held, invalidate transcripts (emit TranscriptsInvalidated), emit SessionRemoved +
    /// Sessions.
    pub async fn stop(&self, id: &str) {
        todo!("port: SessionManager.Stop")
    }

    /// Append, hook its changes, raise once.
    fn add(self: &Arc<Self>, s: Arc<LiveSession>) {
        todo!("port: SessionManager.Add")
    }

    /// The change hook given to every LiveSession: when its version moved, run the notification observer now and
    /// `emit_summary_throttled()`.
    fn on_session_changed(self: &Arc<Self>, id: &str, version: u64) {
        todo!("port: SessionManager.Add (Changed relay)")
    }

    /// `Push::Sessions(summary())` now if 50 ms passed since the last one, else schedule one at the window's end
    /// (once). add/stop call it too.
    fn emit_summary_throttled(self: &Arc<Self>) {
        todo!("port: (SessionManager.Changed, throttled)")
    }

    /// Build a LiveSession wired to this manager (sink + change hook through a Weak).
    fn create(self: &Arc<Self>, init: SessionInit) -> Arc<LiveSession> {
        todo!("port: (new LiveSession + subscribe)")
    }

    /// Dispose every session in parallel; log `[id] shutdown: msg`.
    pub async fn dispose_all(&self) {
        todo!("port: SessionManager.DisposeAsync")
    }
}
