//! Port of `Components/Session/StatusPanel.razor` — spec 07 §15. Owner: ui-session-inspector.

use crate::state::{use_app, use_ui, SessionHandle};
use ccui_shared::model::*;
use ccui_shared::t;
use leptos::prelude::*;

/// /status sections (session_refresh_status), account, sign out (claude_auth_logout).
#[component]
pub fn StatusPanel(
    session: SessionHandle,
) -> impl IntoView {
    // port: StatusPanel.razor markup + behaviour (placeholder root only)
    view! { <div class="section"></div> }
}
