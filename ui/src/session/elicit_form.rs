//! Port of `Components/Session/ElicitForm.razor` — spec 07 §28, 02 §12. Owner: ui-session-inspector.

use crate::state::{use_app, use_ui, SessionHandle};
use ccui_shared::model::*;
use ccui_shared::t;
use leptos::prelude::*;

/// MCP elicitation form (`elicit::parse` / `content`) or URL mode link.
#[component]
pub fn ElicitForm(
    session: SessionHandle,
    permission: PendingPermission,
) -> impl IntoView {
    // port: ElicitForm.razor markup + behaviour (placeholder root only)
    view! { <form class="ask"></form> }
}
