//! Port of `Components/Pages/Extensions.razor (MCP tab)` — spec 06 §6.7. Owner: ui-extensions.

use crate::state::{use_app, use_ui, SessionHandle};
use ccui_shared::model::*;
use ccui_shared::t;
use leptos::prelude::*;

/// Servers (mcp_status / init), toggle, reconnect (all), sign in (auth poll 2 s × 150), tools inspector.
#[component]
pub fn McpTab(
    session: SessionHandle,
) -> impl IntoView {
    // port: Extensions.razor markup + behaviour (placeholder root only)
    view! { <div class="ext-mcp"></div> }
}
