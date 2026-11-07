//! Port of `Components/Session/WorkflowCard.razor` — spec 07 §13, 05 §4. Owner: ui-session-thread-composer.

use crate::state::{use_app, use_ui, SessionHandle};
use ccui_shared::model::*;
use ccui_shared::t;
use ccui_shared::workflow::WfState;
use leptos::prelude::*;

/// Ultracode Workflow run card (workflow_run polled 1 s while live via Ticker).
#[component]
pub fn WorkflowCard(
    session: SessionHandle,
    /// Reactive (`SessionHandle::tool_at` / `tool_by_id`): Running → Done, live tokens re-render in place.
    #[prop(into)] tool: Signal<ToolItem>,
) -> impl IntoView {
    // port: WorkflowCard.razor markup + behaviour (placeholder root only)
    view! { <div class="wfcard"></div> }
}

/// `WorkflowCard.Dot(s, t)`: Done "done", Running "run", Failed "err", Unknown "exited", else (not started)
/// "wait" when the call waits for a permission, "idle" otherwise. Also used by WorkflowPanel.
pub fn dot(state: WfState, tool: Option<&ToolItem>) -> &'static str {
    todo!("port: WorkflowCard.Dot")
}

/// `WorkflowCard.Label(s, raw)`: raw ?? Tool.Done | Status.Running | Tool.Error | Wf.State.Unknown | Wf.State.Waiting.
/// Also used by WorkflowPanel.
pub fn label(state: WfState, raw: Option<&str>) -> String {
    todo!("port: WorkflowCard.Label")
}
