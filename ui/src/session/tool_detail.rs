//! Port of `Components/Session/ToolDetail.razor` — spec 07 §31. Owner: ui-session-inspector.

use crate::state::{use_app, use_ui, SessionHandle};
use ccui_shared::model::*;
use ccui_shared::t;
use leptos::prelude::*;

/// Selected tool: input, diff (from_patch / from_bash / edit_diff_from_input), output (FileView), MCP sign-in again.
#[component]
pub fn ToolDetail(
    session: SessionHandle,
    /// Reactive (`SessionHandle::tool_at` / `tool_by_id`): Running → Done, live tokens re-render in place.
    #[prop(into)] tool: Signal<ToolItem>,
) -> impl IntoView {
    // port: ToolDetail.razor markup + behaviour (placeholder root only)
    view! { <div class="ph"></div> }
}

/// `ToolDetail.Pretty(json)`: indented JSON for display.
pub fn pretty(json: &serde_json::Value) -> String {
    todo!("port: ToolDetail.Pretty")
}
