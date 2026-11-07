//! Port of `Components/Pages/Extensions.razor (Agents tab)` — spec 06 §6.7. Owner: ui-extensions.

use crate::state::{use_app, use_ui, SessionHandle};
use ccui_shared::model::*;
use ccui_shared::t;
use leptos::prelude::*;

/// Agents (initialize_info.agents or init.agents) with FindBox.
#[component]
pub fn AgentsTab(
    session: SessionHandle,
) -> impl IntoView {
    // port: Extensions.razor markup + behaviour (placeholder root only)
    view! { <table class="mcp"></table> }
}
