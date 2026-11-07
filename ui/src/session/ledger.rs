//! Port of `Components/Session/Ledger.razor` — spec 07 §4. Owner: ui-session-thread-composer.

use crate::state::{use_app, use_ui, SessionHandle};
use ccui_shared::model::*;
use ccui_shared::t;
use leptos::prelude::*;

/// A run of top-level tool calls (ToolRow each), collapsible.
#[component]
pub fn Ledger(
    session: SessionHandle,
    tools: Vec<ToolItem>,
    #[prop(optional)] expanded: bool,
) -> impl IntoView {
    // port: Ledger.razor markup + behaviour (placeholder root only)
    view! { <div class="ledger"></div> }
}
