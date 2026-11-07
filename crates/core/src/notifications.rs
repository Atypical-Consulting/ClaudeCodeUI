//! Port of the notification observer of `MainLayout.razor` + `Core/Notifications.cs` — spec 03 §8.2, 06 §2.
//! Owner: core-store-sessions. Runs in the backend on every session change (synchronously, so Waiting → answered →
//! Waiting inside one burst is still seen). Posts via `UiSink::notify` only when the `notify-on` flag is set and the
//! window is not focused. Pure decisions: `ccui_shared::notifications`.

use crate::live::LiveSession;
use crate::UiSink;
use ccui_shared::model::{SessionStatus, Time};
use ccui_shared::notifications::{between, body, remember, Kind};
use parking_lot::Mutex;
use std::collections::HashMap;
use std::sync::Arc;

#[derive(Default)]
pub struct NotifyObserver {
    /// id → (status, remembered result time, pushes). Empty until `prime`.
    seen: Mutex<HashMap<String, (SessionStatus, Option<Time>, i32)>>,
}

/// What one observation reads of a session, under its gate (status and result read together: no torn read).
struct Seen {
    id: String,
    name: String,
    status: SessionStatus,
    result: Option<Time>,
    subtype: Option<String>,
    first_tool: Option<String>,
    turn_ms: Option<i64>,
    pushes: i32,
    last_push: Option<String>,
}

impl NotifyObserver {
    /// First observation: record without notifying (what is already waiting is on screen, not news).
    pub fn prime(&self, sessions: &[Arc<LiveSession>]) {
        self.due(read(sessions));
    }

    /// Forget closed sessions, then per session: Decision / Done / PushNotification → post.
    pub fn observe(&self, sessions: &[Arc<LiveSession>], sink: &dyn UiSink) {
        let due = self.due(read(sessions));
        if due.is_empty() || !shell_on() || sink.window_focused() {
            return;
        }
        for (id, title, text) in due {
            sink.notify(&id, &title, &text);
        }
    }

    /// Compare each session with its last observation → (id, title, body) to post.
    fn due(&self, now: Vec<Seen>) -> Vec<(String, String, String)> {
        let mut seen = self.seen.lock();
        // A closed session forgets its observation: reopened under the same id, its pushes restart at 0.
        seen.retain(|id, _| now.iter().any(|s| s.id == *id));
        let mut due = Vec::new();
        for s in now {
            let had = seen.get(&s.id).copied();
            let before_result = had.and_then(|h| h.1);
            seen.insert(s.id.clone(), (s.status, remember(s.status, before_result, s.result), s.pushes));
            let kind = between(had.map(|h| h.0), before_result, s.status, s.result, s.subtype.as_deref());
            if kind != Kind::None {
                due.push((s.id.clone(), s.name.clone(), body(kind, s.first_tool.as_deref(), s.turn_ms, s.subtype.as_deref())));
            }
            // Claude's PushNotification tool (a replayed transcript's are old news).
            if had.is_some_and(|h| s.pushes > h.2) {
                let msg = s.last_push.filter(|m| !m.is_empty()).unwrap_or_else(|| ccui_shared::t!("Notify.Push"));
                due.push((s.id, s.name, msg));
            }
        }
        due
    }
}

fn read(sessions: &[Arc<LiveSession>]) -> Vec<Seen> {
    sessions
        .iter()
        .map(|s| {
            s.with_state(|st| Seen {
                id: st.id.clone(),
                name: st.name.clone(),
                status: st.status,
                result: st.last_result_at,
                subtype: st.last_result_subtype.clone(),
                first_tool: st.pending.first().map(|p| p.tool.clone()),
                turn_ms: st.last_turn_ms,
                pushes: st.pushes,
                last_push: st.last_push.clone(),
            })
        })
        .collect()
}

/// `Notifications.ShellOn` = AppFlag `notify-on`.
pub fn shell_on() -> bool {
    crate::app_flag::flag(ccui_shared::api::FLAG_NOTIFY)
}

#[cfg(test)]
mod tests {
    use super::*;
    use SessionStatus::*;

    fn seen(id: &str, status: SessionStatus, result: Option<Time>, pushes: i32) -> Seen {
        Seen {
            id: id.into(),
            name: format!("name {id}"),
            status,
            result,
            subtype: Some("success".into()),
            first_tool: (status == Waiting).then(|| "Bash".to_string()),
            turn_ms: Some(2400),
            pushes,
            last_push: None,
        }
    }

    #[test]
    fn observation_sequence() {
        ccui_shared::i18n::set_thread_lang(Some(ccui_shared::i18n::Lang::En));
        let o = NotifyObserver::default();
        let t = Some(chrono::Utc::now());
        // A session first seen Waiting is a decision (prime() drops what the first observation finds).
        let d = o.due(vec![seen("a", Waiting, None, 0), seen("b", Running, None, 0)]);
        assert_eq!(d.len(), 1);
        assert_eq!(d[0].2, "Needs your permission: Bash");
        let o = NotifyObserver::default();
        o.due(vec![seen("b", Running, None, 0)]);
        let d = o.due(vec![seen("b", Waiting, None, 0)]);
        assert_eq!(d.len(), 1);
        assert_eq!(d[0].1, "name b");
        assert!(o.due(vec![seen("b", Waiting, None, 0)]).is_empty(), "still waiting");
        assert_eq!(o.due(vec![seen("b", Idle, t, 0)]).len(), 1, "turn done");
        assert!(o.due(vec![seen("b", Idle, t, 0)]).is_empty(), "no new result");
        let d = o.due(vec![seen("b", Idle, t, 1)]);
        assert_eq!(d.len(), 1, "a PushNotification");
        assert_eq!(d[0].2, ccui_shared::t!("Notify.Push"));
        // A closed session forgets its observation: back under the same id, its first push count is not news.
        o.due(vec![]);
        assert!(o.due(vec![seen("b", Idle, t, 3)]).is_empty());
        ccui_shared::i18n::set_thread_lang(None);
    }
}
