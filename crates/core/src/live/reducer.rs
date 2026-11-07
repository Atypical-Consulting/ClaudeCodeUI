//! Port of `LiveSession.Reduce` and its parts (`OnResult`, rate-limit merge, `SubagentModel`, `Reloaded`) —
//! spec 01 §7.4-7.7. Owner: core-live-session.
//! Pure over `SessionState` + `now`: no I/O, no locking (the caller holds the gate). Transcript replay runs it too.
//! Tests: LiveSession.Check() cases (cost accounting, rate-limit merge, queue/drain, denied vs NotRun…).

use super::state::SessionState;
use crate::events::{ClaudeEvent, ResultEvt};
use ccui_shared::model::{RateLimits, Time};
use serde_json::Value;

impl SessionState {
    /// Fold one event (rules in spec order; prelude: last_event_at, retry cleared by progress, hook removed from
    /// running_hooks).
    pub fn reduce(&mut self, e: &ClaudeEvent, now: Time) {
        todo!("port: LiveSession.Reduce")
    }

    /// §7.5: cost (cumulative per process, cost_base), last turn, context, denied → Denied, end tools, drain queue.
    pub fn on_result(&mut self, r: &ResultEvt, now: Time) {
        todo!("port: LiveSession.Reduce(ResultEvt)")
    }

    /// §7.6: merge windows (a window whose reset is None keeps its previous value).
    pub fn on_rate_limit(&mut self, e: &RateLimits, now: Time) {
        todo!("port: LiveSession.Reduce(RateLimitEvt)")
    }

    /// First message model of an Agent-kind parent tool.
    pub fn subagent_model(&mut self, parent: Option<&str>, model: Option<&str>) {
        todo!("port: LiveSession.SubagentModel")
    }

    /// §7.7: after reload_plugins + reload_skills.
    pub fn reloaded(&mut self, plugins: &Value, skills: &Value) {
        todo!("port: LiveSession.Reloaded")
    }
}
