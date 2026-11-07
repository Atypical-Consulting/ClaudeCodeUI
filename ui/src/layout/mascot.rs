//! Port of `Components/Layout/Mascot.razor` — spec 06 §4 Mascot. Owner: ui-shell.
//! Rail duck. Priority: decisions pending > sessions outside the app that need you (Background) > running >
//! crashed > turn done < 5 min > calm.

use crate::common::duck::{Duck, Mood};
use crate::state::use_app;
use ccui_shared::model::*;
use ccui_shared::session::SessionsSummary;
use ccui_shared::{fmt, i18n, t};
use leptos::prelude::*;

/// `(mood, title, text)` of the mascot.
pub fn state(s: &SessionsSummary, bg_needs: i32, now: Time) -> (Mood, String, String) {
    let pending = s.decision_queue.len();
    if pending > 0 {
        return (Mood::Shocked, i18n::plural(pending as i64, "Mascot.Decision"), i18n::kbd(&t!("Mascot.NextDecision")));
    }
    if bg_needs > 0 {
        return (Mood::Shocked, i18n::plural(bg_needs as i64, "Mascot.BgNeeds"), t!("Mascot.BgWhere"));
    }
    let running = s.sessions.iter().filter(|x| x.status == SessionStatus::Running).count();
    if running > 0 {
        return (Mood::Excited, t!("Mascot.Working"), i18n::plural(running as i64, "Mascot.Running"));
    }
    if let Some(c) = s.sessions.iter().rev().find(|x| x.status == SessionStatus::Crashed) {
        return (Mood::Sad, t!("Mascot.Crashed"), format!("{} · {}", c.name, c.exit_label));
    }
    // ponytail: re-evaluated on the next session event, not by a timer at the 5-minute mark
    let since = now - chrono::Duration::minutes(5);
    let done = s
        .sessions
        .iter()
        .filter(|x| x.last_result_subtype.as_deref() == Some("success") && x.last_result_at.is_some_and(|at| at > since))
        .max_by_key(|x| x.last_result_at);
    if let Some(d) = done {
        let text = match d.last_turn_ms {
            Some(ms) => format!("{} · {}", d.name, fmt::dur(ms)),
            None => d.name.clone(),
        };
        return (Mood::Blissful, t!("Mascot.TurnDone"), text);
    }
    (Mood::Happy, t!("Mascot.Calm"), t!("Mascot.NoDecision"))
}

/// The duck and one status line, priority: decisions, background needs, running, crashed, turn done
/// (5 min), calm.
#[component]
pub fn Mascot() -> impl IntoView {
    let app = use_app();
    let st = Memo::new(move |_| {
        let bg = app.bg_needs.get();
        app.summary.with(|s| state(s, bg, chrono::Utc::now()))
    });
    view! {
        <div class="mascot">
            <div class="duck" aria-hidden="true">{move || view! { <Duck mood=st.read().0 size=48/> }}</div>
            <div role="status" title=move || st.read().2.clone()><b>{move || st.read().1.clone()}</b>{move || st.read().2.clone()}</div>
        </div>
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::layout::rail::tests::summary;
    use ccui_shared::session::DecisionEntry;

    #[test]
    fn priorities() {
        let now: Time = "2026-11-07T12:00:00Z".parse().unwrap();
        let mut s = SessionsSummary::default();
        assert_eq!(state(&s, 0, now).0, Mood::Happy);

        let mut done = summary("d", "/d", None, SessionStatus::Idle);
        done.last_result_subtype = Some("success".into());
        done.last_result_at = Some(now - chrono::Duration::minutes(2));
        done.last_turn_ms = Some(65_000);
        s.sessions.push(done.clone());
        let (m, _, text) = state(&s, 0, now);
        assert_eq!(m, Mood::Blissful);
        assert!(text.starts_with("d · "));
        // older than 5 minutes: calm again
        s.sessions[0].last_result_at = Some(now - chrono::Duration::minutes(6));
        assert_eq!(state(&s, 0, now).0, Mood::Happy);

        let mut crashed = summary("c", "/c", None, SessionStatus::Crashed);
        crashed.exit_label = "exit 1".into();
        s.sessions.push(crashed);
        assert_eq!(state(&s, 0, now), (Mood::Sad, t!("Mascot.Crashed"), "c · exit 1".to_string()));

        s.sessions.push(summary("r", "/r", None, SessionStatus::Running));
        assert_eq!(state(&s, 0, now).0, Mood::Excited);
        assert_eq!(state(&s, 2, now).0, Mood::Shocked);
        assert_eq!(state(&s, 2, now).2, t!("Mascot.BgWhere"));

        s.decision_queue.push(DecisionEntry {
            session_id: "r".into(),
            session_name: "r".into(),
            session_cwd: "/r".into(),
            permission: PendingPermission {
                request_id: "q".into(),
                tool: "Bash".into(),
                input: serde_json::json!({}),
                description: None,
                tool_use_id: None,
                suggestions: None,
                at: now,
                info: None,
                target_exists: None,
            },
            agent_label: None,
        });
        assert_eq!(state(&s, 2, now).1, i18n::plural(1, "Mascot.Decision"));
    }
}
