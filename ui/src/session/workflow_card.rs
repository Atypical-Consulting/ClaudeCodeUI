//! Port of `Components/Session/WorkflowCard.razor` — spec 07 §13, 05 §4. Owner: ui-session-thread-composer.

use crate::state::{use_app, use_ui, SessionHandle};
use ccui_shared::model::*;
use ccui_shared::t;
use leptos::prelude::*;

/// Ultracode Workflow run card (workflow_run polled 1 s while live via Ticker).
#[component]
pub fn WorkflowCard(
    session: SessionHandle,
    tool: ToolItem,
) -> impl IntoView {
    // port: WorkflowCard.razor markup + behaviour (placeholder root only)
    view! { <div class="wfcard"></div> }
}
