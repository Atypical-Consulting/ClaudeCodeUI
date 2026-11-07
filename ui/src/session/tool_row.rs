//! Port of `Components/Session/ToolRow.razor` — spec 07 §19. Owner: ui-session-thread-composer.

use crate::state::{use_app, use_ui, SessionHandle};
use ccui_shared::model::*;
use ccui_shared::t;
use leptos::prelude::*;

/// One tool call row (label, target, state, meta); selects it in the inspector.
#[component]
pub fn ToolRow(
    session: SessionHandle,
    tool: ToolItem,
) -> impl IntoView {
    // port: ToolRow.razor markup + behaviour (placeholder root only)
    view! { <button type="button" class="t"></button> }
}

/// `ToolRow.Meta(tool)`: the right-hand meta text (duration / lines / results…), computed once per finished tool.
pub fn meta(tool: &ToolItem) -> Option<String> {
    todo!("port: ToolRow.Meta")
}
