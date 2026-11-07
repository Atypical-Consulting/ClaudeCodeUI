//! CONTRACT — backend → UI pushes. ONE Tauri event name (`PUSH_EVENT`) carrying this enum; the UI listens once
//! (`ui/src/ipc.rs::listen_push`) and dispatches. Replaces the C# `Changed` events (LiveSession, SessionManager,
//! WorktreeService, BackgroundAgents, Bookmarks, AppPref, TranscriptStore.Generation) and the stdout line protocol
//! of the old sidecar (`ccui-notify`, `ccui:check-for-updates`). Spec 01 §7.2, 03 §2/§4/§5/§9, 04 §1.3/§1.11, 06 §11.
//!
//! Owner: tauri-glue. Emitted through `ccui_core::UiSink::emit`.

use crate::session::{SessionPatch, SessionStream, SessionsSummary};
use serde::{Deserialize, Serialize};

/// The single Tauri event name for every push.
pub const PUSH_EVENT: &str = "ccui://push";

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(tag = "type", content = "data")]
pub enum Push {
    /// One session's structural change (items/meta), throttled 50 ms; immediate on permission request and result.
    SessionPatch(SessionPatch),
    /// One session's streaming text (throttled with the patches, does not bump version).
    SessionStream(SessionStream),
    /// App-wide session list / decision queue / quota (relayed only when some session's version moved, or on
    /// add/stop).
    Sessions(SessionsSummary),
    /// A session was removed from the live list (Stop): its page re-resolves (transcript) or shows "not found".
    SessionRemoved { id: String },
    /// `TranscriptStore.Invalidate()`: readers of `recent_sessions` holding an older generation reload now.
    TranscriptsInvalidated { generation: u64 },
    /// `WorktreeService.CleanableCount` changed (Rail tag). None = no scan yet.
    WorktreeCount { count: Option<i32> },
    /// One step of `worktrees_run` (in order), then `WorktreeRunDone`.
    WorktreeRunStep(crate::api::WorktreeRunStep),
    WorktreeRunDone { run_id: u64 },
    /// `BackgroundAgents.NeedsYouCount` changed (Rail badge, Mascot).
    BgAgents { needs_you: i32 },
    /// `Bookmarks.Changed` (ContextPanel, Thread).
    Bookmarks { session_id: String },
    /// `AppPref.Changed` / `AppFlag` set (palette theme command, another window). value None = file deleted.
    PrefChanged { name: String, value: Option<String> },
    /// Navigate the main window (notification click → `session/<id>`). Path is base-relative, no leading '/'.
    Navigate { path: String },
    /// An update is about to install: the UI shows nothing more and stops sending.
    Updating,
}
