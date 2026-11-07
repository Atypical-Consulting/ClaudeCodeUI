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
use crate::{replay, transcript_store, UiSink};
use ccui_shared::api::{PastSession, SavedLimits, StartArgs};
use ccui_shared::model::{Item, RateLimits, SessionStatus, Time};
use ccui_shared::push::Push;
use ccui_shared::session::{DecisionEntry, SessionsSummary, UsageSource};
use ccui_shared::session_rules;
use chrono::{DateTime, FixedOffset};
use parking_lot::{Mutex, RwLock};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::sync::atomic::Ordering;
use std::sync::{Arc, Weak};
use std::time::Duration;

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

/// C# `default(DateTimeOffset)`: a window the event did not carry.
fn csharp_default() -> DateTime<FixedOffset> {
    DateTime::parse_from_rfc3339("0001-01-01T00:00:00+00:00").expect("valid literal")
}

fn to_file_time(t: Option<Time>) -> DateTime<FixedOffset> {
    t.map_or_else(csharp_default, |t| t.fixed_offset())
}

fn from_file_time(t: DateTime<FixedOffset>) -> Option<Time> {
    (t > csharp_default()).then(|| t.to_utc())
}

impl From<&RateLimits> for RateLimitsFile {
    fn from(l: &RateLimits) -> Self {
        RateLimitsFile {
            five_hour: l.five_hour,
            five_hour_reset: to_file_time(l.five_hour_reset),
            seven_day: l.seven_day,
            seven_day_reset: to_file_time(l.seven_day_reset),
            status: l.status.clone(),
            kind: l.kind.clone(),
            extra_on: l.extra_on,
            extra_in_use: l.extra_in_use,
            extra_off: l.extra_off.clone(),
        }
    }
}

impl From<RateLimitsFile> for RateLimits {
    fn from(l: RateLimitsFile) -> Self {
        RateLimits {
            five_hour: l.five_hour,
            five_hour_reset: from_file_time(l.five_hour_reset),
            seven_day: l.seven_day,
            seven_day_reset: from_file_time(l.seven_day_reset),
            status: l.status,
            kind: l.kind,
            extra_on: l.extra_on,
            extra_in_use: l.extra_in_use,
            extra_off: l.extra_off,
        }
    }
}

fn limits_file() -> std::path::PathBuf {
    crate::paths::app_data().join("limits.json")
}

fn now_ms() -> i64 {
    chrono::Utc::now().timestamp_millis()
}

/// The blocking store reads run off the async workers.
async fn blocking<T: Send + 'static>(f: impl FnOnce() -> T + Send + 'static) -> Option<T> {
    tokio::task::spawn_blocking(f).await.ok()
}

fn tool_count(items: &[Item]) -> i32 {
    items.iter().filter(|i| i.as_tool().is_some()).count() as i32
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
        Arc::new(SessionManager {
            all: RwLock::new(Vec::new()),
            open_gate: tokio::sync::Mutex::new(()),
            seen_versions: Mutex::new(HashMap::new()),
            summary_last_ms: std::sync::atomic::AtomicI64::new(0),
            summary_queued: std::sync::atomic::AtomicBool::new(false),
            notifier: NotifyObserver::default(),
            sink,
        })
    }

    pub fn all(&self) -> Vec<Arc<LiveSession>> {
        self.all.read().clone()
    }

    pub fn get(&self, id: &str) -> Option<Arc<LiveSession>> {
        self.all.read().iter().find(|s| s.id() == id).cloned()
    }

    /// status != Exited.
    pub fn active(&self) -> Vec<Arc<LiveSession>> {
        self.all().into_iter().filter(|s| s.with_state(|st| st.status != SessionStatus::Exited)).collect()
    }

    pub fn is_active(&self, id: &str) -> bool {
        self.get(id).is_some_and(|s| s.with_state(|st| st.status != SessionStatus::Exited))
    }

    /// `Get(id)?.HasProcess != true && HeldByClaude().contains(id)`.
    pub fn running_elsewhere(&self, id: &str) -> bool {
        !self.get(id).is_some_and(|s| s.has_process()) && transcript_store::held_by_claude().contains(id)
    }

    /// Every (session, pending) across sessions, by arrival time; `agent_label` =
    /// `session_rules::agent_label(asking_agent(items, p))` read under each session's gate.
    pub fn decision_queue(&self) -> Vec<DecisionEntry> {
        let mut queue: Vec<DecisionEntry> = self
            .all()
            .iter()
            .flat_map(|s| {
                s.with_state(|st| {
                    st.pending
                        .iter()
                        .map(|p| DecisionEntry {
                            session_id: st.id.clone(),
                            session_name: st.name.clone(),
                            session_cwd: st.cwd.clone(),
                            permission: p.clone(),
                            agent_label: session_rules::asking_agent(st.items(), p).map(session_rules::agent_label),
                        })
                        .collect::<Vec<_>>()
                })
            })
            .collect();
        queue.sort_by_key(|d| d.permission.at); // stable: arrival order
        queue
    }

    /// The app-wide summary (sessions, queue, latest limits, usage source, held ids).
    pub fn summary(&self) -> SessionsSummary {
        let all = self.all();
        let mut limits: Option<(Time, RateLimits)> = None;
        // UsageSource: last with usage and a process, else last with usage (the one that can be asked again).
        let (mut with_process, mut any): (Option<UsageSource>, Option<UsageSource>) = (None, None);
        for s in &all {
            let has_process = s.has_process();
            s.with_state(|st| {
                if let Some(l) = &st.limits {
                    let at = st.limits_at.unwrap_or(DateTime::UNIX_EPOCH);
                    // MaxBy keeps the first of equal maxima.
                    if limits.as_ref().is_none_or(|(best, _)| at > *best) {
                        limits = Some((at, l.clone()));
                    }
                }
                if let Some(u) = &st.heavy.usage {
                    let src = UsageSource { session_id: st.id.clone(), usage: u.clone(), has_process };
                    if has_process {
                        with_process = Some(src.clone());
                    }
                    any = Some(src);
                }
            });
        }
        SessionsSummary {
            sessions: all.iter().map(|s| s.summary()).collect(),
            decision_queue: self.decision_queue(),
            limits_at: limits.as_ref().map(|(at, _)| *at),
            limits: limits.map(|(_, l)| l),
            usage_source: with_process.or(any),
            held: transcript_store::held_by_claude().into_iter().collect(),
        }
    }

    /// Write `limits.json` `{Limits, At: now}`; errors swallowed.
    pub fn remember_limits(limits: &RateLimits) {
        let saved = SavedLimitsFile { limits: limits.into(), at: chrono::Local::now().fixed_offset() };
        let path = limits_file();
        if let (Some(dir), Ok(json)) = (path.parent(), serde_json::to_string(&saved)) {
            let _ = std::fs::create_dir_all(dir).and_then(|_| std::fs::write(&path, json));
        }
    }

    /// Read `limits.json` (missing / IO / bad JSON → None).
    pub fn last_limits() -> Option<SavedLimits> {
        let saved: SavedLimitsFile = serde_json::from_str(&std::fs::read_to_string(limits_file()).ok()?).ok()?;
        Some(SavedLimits { limits: saved.limits.into(), at: saved.at.to_utc() })
    }

    /// New session (uuid v4, Untitled), record add-dirs, spawn in a catch-all (a failed start leaves it Crashed),
    /// refresh git.
    pub async fn start(self: &Arc<Self>, args: StartArgs) -> Arc<LiveSession> {
        let init = SessionInit {
            id: uuid::Uuid::new_v4().to_string(),
            name: args.name,
            cwd: args.cwd,
            mode: args.mode,
            worktree: args.worktree,
            model: args.model,
            effort: args.effort,
            add_dirs: args.add_dirs.clone(),
            fallback_model: args.fallback,
            restricted: args.restricted,
            safe_mode: args.safe_mode,
            agent: args.agent,
            untitled: true,
            ..SessionInit::default()
        };
        let s = self.create(init);
        self.add(s.clone());
        if !args.add_dirs.is_empty() {
            transcript_store::record_dirs(s.id(), &args.add_dirs);
        }
        let _ = s.ensure_process(false).await; // a failed start leaves the session Crashed with the reason
        tokio::spawn(s.clone().refresh_git());
        s
    }

    /// History now, `--resume` process on first Send/Restart. Under `open_gate`; already live → it.
    pub async fn open(self: &Arc<Self>, past: PastSession) -> Arc<LiveSession> {
        let _gate = self.open_gate.lock().await;
        if let Some(live) = self.get(&past.id) {
            return live;
        }
        let id = past.id.clone();
        let (items, add_dirs) = blocking(move || {
            // The CLI skips a missing --add-dir folder silently: list only those still there.
            let dirs = transcript_store::recorded_dirs(&id).into_iter().filter(|d| std::path::Path::new(d).is_dir()).collect();
            (replay::load(&id), dirs)
        })
        .await
        .unwrap_or_default();
        // The transcript's mode, if the UI offers it (bypassPermissions / dontAsk fall back to default).
        let mode = past.mode.as_deref().filter(|m| session_rules::MODES.contains(m)).unwrap_or("default").to_string();
        let started_at = items.iter().find_map(|i| i.as_user().map(|u| u.at)).unwrap_or(past.last_write);
        let count = tool_count(&items);
        let cost = past.cost_usd.unwrap_or_default();
        let s = self.create(SessionInit {
            id: past.id,
            name: past.title,
            cwd: past.cwd,
            mode,
            resumable: true,
            add_dirs,
            items,
            cost_usd: cost,
            cost_at_open: cost,
            pr_url: past.pr_url,
            started_at: Some(started_at),
            last_event_at: Some(past.last_write),
            ..SessionInit::default()
        });
        s.with_state_mut(|st| st.tool_count = count);
        tokio::spawn(s.clone().refresh_git());
        self.add(s.clone());
        s
    }

    /// `Get(id) ?? TranscriptStore.ById(id) → Open` (deep links, past rows, reloads). Invalid id → None.
    pub async fn open_id(self: &Arc<Self>, id: &str) -> Option<Arc<LiveSession>> {
        if let Some(s) = self.get(id) {
            return Some(s);
        }
        let owned = id.to_string();
        let past = blocking(move || transcript_store::by_id(&owned)).await.flatten()?;
        Some(self.open(past).await)
    }

    /// Continue `src` under a new id (an unsent fork forks its source at the same cut). Returns the fork.
    pub async fn fork(self: &Arc<Self>, src: &Arc<LiveSession>, at: Option<String>) -> Arc<LiveSession> {
        let (src_id, fork_of, fork_at, name, cwd, mode, model, effort, restricted, safe_mode, agent) = src.with_state(|st| {
            (
                st.id.clone(),
                st.fork_of.clone(),
                st.fork_at.clone(),
                st.name.clone(),
                st.cwd.clone(),
                st.mode.clone(),
                st.model.clone(),
                st.effort.clone(),
                st.restricted,
                st.safe_mode,
                st.agent.clone(),
            )
        });
        // A fork writes its own transcript on its first message; before that `--resume <fork>` answers "No
        // conversation found", so forking it forks its source at the same cut. One step is enough.
        let probe = src_id.clone();
        let unsent = fork_of.is_some() && blocking(move || transcript_store::find(&probe).is_none()).await.unwrap_or(false);
        let (of, cut) = match fork_of {
            Some(o) if unsent => (o, at.or(fork_at)),
            _ => (src_id, at),
        };
        let (of2, cut2) = (of.clone(), cut.clone());
        let (items, inherited) = blocking(move || {
            (session_rules::upto(replay::load(&of2), cut2.as_deref()), transcript_store::persisted_cost(&of2))
        })
        .await
        .unwrap_or_default();
        let count = tool_count(&items);
        let s = self.create(SessionInit {
            id: uuid::Uuid::new_v4().to_string(),
            name: ccui_shared::t!("Session.ForkName", name),
            cwd,
            mode,
            model,
            effort,
            restricted,
            safe_mode,
            agent,
            fork_of: Some(of),
            fork_at: cut,
            items,
            // The source's spend, not this run's (Overview counts cost_usd - cost_at_open).
            cost_usd: inherited,
            cost_at_open: inherited,
            ..SessionInit::default()
        });
        s.with_state_mut(|st| st.tool_count = count);
        self.add(s.clone());
        let _ = s.ensure_process(false).await; // a failed start leaves the session Crashed with the reason
        tokio::spawn(s.clone().refresh_git());
        s
    }

    /// Remove, dispose, forget held, invalidate transcripts (emit TranscriptsInvalidated), emit SessionRemoved +
    /// Sessions.
    pub async fn stop(&self, id: &str) {
        let Some(s) = self.get(id) else { return };
        self.all.write().retain(|x| !Arc::ptr_eq(x, &s));
        self.seen_versions.lock().remove(id);
        s.dispose().await;
        transcript_store::forget_held();
        // Its Recent row must show the cost and title of the turns just run.
        let generation = transcript_store::invalidate();
        self.sink.emit(Push::TranscriptsInvalidated { generation });
        self.sink.emit(Push::SessionRemoved { id: id.to_string() });
        self.notifier.observe(&self.all(), self.sink.as_ref());
        self.summary_last_ms.store(now_ms(), Ordering::SeqCst);
        self.sink.emit(Push::Sessions(self.summary()));
    }

    /// Append, hook its changes, raise once.
    fn add(self: &Arc<Self>, s: Arc<LiveSession>) {
        self.seen_versions.lock().insert(s.id().to_string(), s.version());
        self.all.write().push(s);
        self.notifier.observe(&self.all(), self.sink.as_ref());
        self.emit_summary_throttled();
    }

    /// The change hook given to every LiveSession: when its version moved, run the notification observer now and
    /// `emit_summary_throttled()`.
    fn on_session_changed(self: &Arc<Self>, id: &str, version: u64) {
        // Streamed text alone (version unchanged) concerns the session page only.
        if self.seen_versions.lock().insert(id.to_string(), version) == Some(version) {
            return;
        }
        self.notifier.observe(&self.all(), self.sink.as_ref());
        self.emit_summary_throttled();
    }

    /// `Push::Sessions(summary())` now if 50 ms passed since the last one, else schedule one at the window's end
    /// (once). add/stop call it too.
    fn emit_summary_throttled(self: &Arc<Self>) {
        const WINDOW: i64 = 50;
        let elapsed = now_ms() - self.summary_last_ms.load(Ordering::SeqCst);
        if elapsed >= WINDOW && !self.summary_queued.load(Ordering::SeqCst) {
            self.summary_last_ms.store(now_ms(), Ordering::SeqCst);
            self.sink.emit(Push::Sessions(self.summary()));
            return;
        }
        if self.summary_queued.swap(true, Ordering::SeqCst) {
            return; // the trailing emit will carry this change
        }
        let wait = Duration::from_millis((WINDOW - elapsed).clamp(0, WINDOW) as u64);
        let weak: Weak<Self> = Arc::downgrade(self);
        let fire = move || {
            if let Some(m) = weak.upgrade() {
                m.summary_queued.store(false, Ordering::SeqCst);
                m.summary_last_ms.store(now_ms(), Ordering::SeqCst);
                m.sink.emit(Push::Sessions(m.summary()));
            }
        };
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

    /// Build a LiveSession wired to this manager (sink + change hook through a Weak).
    fn create(self: &Arc<Self>, init: SessionInit) -> Arc<LiveSession> {
        let weak = Arc::downgrade(self);
        let hook: crate::live::ChangeHook = Arc::new(move |id: &str, version: u64| {
            if let Some(m) = weak.upgrade() {
                m.on_session_changed(id, version);
            }
        });
        LiveSession::new(init, self.sink.clone(), hook)
    }

    /// Dispose every session in parallel; log `[id] shutdown: msg`.
    pub async fn dispose_all(&self) {
        // One failure must not leave the other claude trees running.
        let tasks: Vec<_> = self
            .all()
            .into_iter()
            .map(|s| (s.id().to_string(), tokio::spawn(async move { s.dispose().await })))
            .collect();
        for (id, t) in tasks {
            if let Err(e) = t.await {
                eprintln!("[{id}] shutdown: {e}");
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn limits_file_round_trip() {
        let dir = tempfile::tempdir().unwrap();
        crate::paths::set_test_root(Some(dir.path()));
        assert!(SessionManager::last_limits().is_none(), "missing file");
        // As the C# app wrote it (System.Text.Json, PascalCase, default(DateTimeOffset) for a missing window).
        std::fs::create_dir_all(crate::paths::app_data()).unwrap();
        std::fs::write(
            limits_file(),
            r#"{"Limits":{"FiveHour":0.42,"FiveHourReset":"2026-10-09T17:00:00+00:00","SevenDay":0.1,"SevenDayReset":"0001-01-01T00:00:00+00:00","Status":"allowed","Type":null,"ExtraOn":false,"ExtraInUse":false,"ExtraOff":null},"At":"2026-10-09T14:03:11.123+02:00"}"#,
        )
        .unwrap();
        let saved = SessionManager::last_limits().unwrap();
        assert_eq!(saved.limits.five_hour, 0.42);
        assert_eq!(saved.limits.five_hour_reset, ccui_shared::json::parse_time("2026-10-09T17:00:00Z"));
        assert_eq!(saved.limits.seven_day_reset, None);
        assert_eq!(saved.limits.status, "allowed");
        assert_eq!(saved.at, ccui_shared::json::parse_time("2026-10-09T12:03:11.123Z").unwrap());

        let l = RateLimits {
            five_hour: 0.5,
            seven_day_reset: saved.limits.five_hour_reset,
            status: "allowed_warning".into(),
            kind: Some("seven_day".into()),
            ..RateLimits::default()
        };
        SessionManager::remember_limits(&l);
        let text = std::fs::read_to_string(limits_file()).unwrap();
        assert!(
            text.starts_with(r#"{"Limits":{"FiveHour":0.5,"FiveHourReset":"0001-01-01T00:00:00Z","SevenDay":0.0,"SevenDayReset":"2026-10-09T17:00:00Z","Status":"allowed_warning","Type":"seven_day""#),
            "{text}"
        );
        assert_eq!(SessionManager::last_limits().unwrap().limits, l);
        std::fs::write(limits_file(), "{bad").unwrap();
        assert!(SessionManager::last_limits().is_none(), "bad JSON");
        crate::paths::set_test_root(None);
    }
}
