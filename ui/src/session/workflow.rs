//! Port of `Components/Session/Workflow.razor` — spec 07 §21. Owner: ui-session-thread-composer.

use crate::state::{use_app, use_ui, SessionHandle};
use ccui_shared::model::*;
use ccui_shared::t;
use leptos::prelude::*;

/// Consecutive Agent calls as a tree (MaxDepth 5) with dots and states; Ticker while any runs.
#[component]
pub fn Workflow(
    session: SessionHandle,
    agents: Vec<ToolItem>,
) -> impl IntoView {
    // port: Workflow.razor markup + behaviour (placeholder root only)
    view! { <div class="wf"></div> }
}

pub const MAX_DEPTH: usize = 5;
