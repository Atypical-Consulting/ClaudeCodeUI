//! Port of `Components/Pages/Extensions.razor (Permissions tab)` — spec 06 §6.7, 02 §9. Owner: ui-extensions.

use crate::state::{use_app, use_ui, SessionHandle};
use ccui_shared::model::*;
use ccui_shared::t;
use leptos::prelude::*;

/// Rules (session_permission_rules), add/remove via permission_rule_edit, post-write poll.
#[component]
pub fn PermissionsTab(
    session: SessionHandle,
) -> impl IntoView {
    // port: Extensions.razor markup + behaviour (placeholder root only)
    view! { <div class="ext-perm"></div> }
}
