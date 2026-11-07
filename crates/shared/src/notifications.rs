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
    todo!("port: Notifications.Between")
}

/// Running/Waiting keep the previous result time (torn-read guard), else the new one.
pub fn remember(status: SessionStatus, result_before: Option<Time>, result: Option<Time>) -> Option<Time> {
    todo!("port: Notifications.Remember")
}

/// Done → `Notify.Failed` (subtype starts with "error") / `Notify.Done` with `Fmt.Dur(turn)`; Decision →
/// `Notify.Question` (AskUserQuestion) / `Notify.Plan` (ExitPlanMode) / `Notify.Decision` (no tool) /
/// `Notify.Permission(tool)`.
pub fn body(kind: Kind, tool: Option<&str>, turn_ms: Option<i64>, subtype: Option<&str>) -> String {
    todo!("port: Notifications.Body")
}
