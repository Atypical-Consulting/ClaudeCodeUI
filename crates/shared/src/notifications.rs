//! Port of the pure functions of `Core/Notifications.cs` — spec 03 §8.3. Owner: core-store-sessions.
//! The observer (backend, on every session change) is `ccui_core::notifications`.

use crate::model::{SessionStatus, Time};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Kind {
    None,
    Decision,
    Done,
}

/// Waiting (was not) → Decision; else seen && Idle && result changed && subtype != "interrupted" → Done; else None.
pub fn between(
    before: Option<SessionStatus>,
    result_before: Option<Time>,
    status: SessionStatus,
    result: Option<Time>,
    subtype: Option<&str>,
) -> Kind {
    if status == SessionStatus::Waiting && before != Some(SessionStatus::Waiting) {
        Kind::Decision
    } else if before.is_some()
        && status == SessionStatus::Idle
        && result != result_before
        && subtype != Some(crate::session_rules::INTERRUPTED)
    {
        Kind::Done
    } else {
        Kind::None
    }
}

/// Running/Waiting keep the previous result time (torn-read guard), else the new one.
pub fn remember(status: SessionStatus, result_before: Option<Time>, result: Option<Time>) -> Option<Time> {
    if matches!(status, SessionStatus::Running | SessionStatus::Waiting) {
        result_before
    } else {
        result
    }
}

/// Done → `Notify.Failed` (subtype starts with "error") / `Notify.Done` with `Fmt.Dur(turn)`; Decision →
/// `Notify.Question` (AskUserQuestion) / `Notify.Plan` (ExitPlanMode) / `Notify.Decision` (no tool) /
/// `Notify.Permission(tool)`.
pub fn body(kind: Kind, tool: Option<&str>, turn_ms: Option<i64>, subtype: Option<&str>) -> String {
    if kind == Kind::Done {
        let key = if subtype.is_some_and(|s| s.starts_with("error")) { "Notify.Failed" } else { "Notify.Done" };
        return crate::t!(key, crate::fmt::dur(turn_ms.unwrap_or(0)));
    }
    match tool {
        Some("AskUserQuestion") => crate::t!("Notify.Question"),
        Some("ExitPlanMode") => crate::t!("Notify.Plan"),
        None => crate::t!("Notify.Decision"),
        Some(t) => crate::t!("Notify.Permission", t),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::i18n::{set_thread_lang, Lang};
    use SessionStatus::*;

    #[test]
    fn between_cases() {
        let t1 = chrono::DateTime::UNIX_EPOCH;
        let t2 = t1 + chrono::Duration::seconds(5);
        let (s1, s2) = (Some(t1), Some(t2));
        assert_eq!(between(Some(Running), None, Waiting, None, None), Kind::Decision);
        assert_eq!(between(None, None, Waiting, None, None), Kind::Decision);
        assert_eq!(between(Some(Waiting), None, Waiting, None, None), Kind::None);
        assert_eq!(between(Some(Running), s1, Idle, s2, Some("success")), Kind::Done);
        assert_eq!(between(Some(Waiting), s1, Idle, s2, Some("error_during_execution")), Kind::Done);
        assert_eq!(between(Some(Running), s1, Idle, s2, Some("interrupted")), Kind::None);
        assert_eq!(between(Some(Running), s1, Running, s2, Some("success")), Kind::None);
        assert_eq!(between(Some(Idle), s2, Idle, s2, Some("success")), Kind::None);
        assert_eq!(between(None, None, Idle, s2, Some("success")), Kind::None);
        for mid in [Running, Waiting] {
            // A torn read mid-Reduce(ResultEvt): (mid, new result), then (Idle, new result). Done exactly once.
            let r = remember(mid, s1, s2);
            assert_eq!(between(Some(mid), s1, mid, s2, Some("success")), Kind::None);
            assert_eq!(between(Some(mid), r, Idle, s2, Some("success")), Kind::Done);
            assert_eq!(between(Some(Idle), remember(Idle, r, s2), Idle, s2, Some("success")), Kind::None);
        }
    }

    #[test]
    fn bodies_fr() {
        set_thread_lang(Some(Lang::Fr));
        assert_eq!(body(Kind::Decision, Some("Bash"), None, None), "Attend ton autorisation\u{a0}: Bash");
        assert_eq!(body(Kind::Decision, Some("ExitPlanMode"), None, None), "Plan prêt à valider");
        assert_eq!(body(Kind::Done, None, Some(2400), Some("success")), "Tour terminé en 2,4 s");
        assert_eq!(body(Kind::Done, None, Some(2400), Some("error_max_turns")), "Tour arrêté sur une erreur après 2,4 s");
        set_thread_lang(None);
    }
}
