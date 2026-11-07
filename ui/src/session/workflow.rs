//! Port of `Components/Session/Workflow.razor` — spec 07 §21. Owner: ui-session-thread-composer.

use crate::state::{use_app, use_ui, SessionHandle};
use ccui_shared::model::*;
use ccui_shared::t;
use leptos::prelude::*;

/// Consecutive Agent calls as a tree (MaxDepth 5) with dots and states; Ticker while any runs.
#[component]
pub fn Workflow(
    session: SessionHandle,
    /// Item indexes of the consecutive Agent calls; rows read `session.tool_at(i)` (the `open` set survives).
    #[prop(into)] agents: Signal<Vec<usize>>,
) -> impl IntoView {
    // port: Workflow.razor markup + behaviour (placeholder root only)
    view! { <div class="wf"></div> }
}

pub const MAX_DEPTH: usize = 5;

/// `Workflow.Dot(a)`: Done → "done", Running → "run", Waiting → "wait", else "err" (also used by AgentPanel).
pub fn dot(agent: &ToolItem) -> &'static str {
    todo!("port: Workflow.Dot")
}

/// `Workflow.State(a)`: `Workflow.Tokens(Fmt.Tokens) · ` when tokens > 0, then the state word (+ ` · Fmt.Dur(elapsed)`
/// for Running/Done; elapsed = (ended_at ?? now) − started_at). Also used by AgentPanel.
pub fn state(agent: &ToolItem, now: Time) -> String {
    todo!("port: Workflow.State")
}
