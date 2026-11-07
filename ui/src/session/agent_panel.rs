//! Port of `Components/Session/AgentPanel.razor` — spec 07 §24. Owner: ui-session-inspector.

use crate::state::{use_app, use_ui, SessionHandle};
use ccui_shared::model::*;
use ccui_shared::t;
use leptos::prelude::*;

/// One Agent call: description, prompt, model, children items (`session_rules::children`), result.
#[component]
pub fn AgentPanel(
    session: SessionHandle,
    /// Reactive (`SessionHandle::tool_by_id`): state, tokens and children update in place.
    #[prop(into)] agent: Signal<ToolItem>,
) -> impl IntoView {
    // port: AgentPanel.razor markup + behaviour (placeholder root only)
    view! { <div class="ph"></div> }
}
