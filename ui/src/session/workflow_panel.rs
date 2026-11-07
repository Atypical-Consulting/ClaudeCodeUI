//! Port of `Components/Session/WorkflowPanel.razor` — spec 07 §29, 05 §4. Owner: ui-session-inspector.

use crate::state::{use_app, use_ui, SessionHandle};
use ccui_shared::model::*;
use ccui_shared::t;
use leptos::prelude::*;

/// A Workflow run: phases, agents, selected phase/agent log (workflow_agent_log), result.
#[component]
pub fn WorkflowPanel(
    session: SessionHandle,
    /// Reactive (`SessionHandle::tool_at` / `tool_by_id`): Running → Done, live tokens re-render in place.
    #[prop(into)] tool: Signal<ToolItem>,
) -> impl IntoView {
    // port: WorkflowPanel.razor markup + behaviour (placeholder root only)
    view! { <div class="ph"></div> }
}
