//! Port of `Components/Shared/StatusScreen.razor` — spec 06 §6.8. Owner: ui-shell.

use crate::state::{use_app, use_ui, SessionHandle};
use ccui_shared::model::*;
use ccui_shared::t;
use leptos::prelude::*;

/// 404 (and optional 500 panel) screen: duck, `{key}.Title/Heading/Text`, path, actions.
#[component]
pub fn StatusScreen(
    code: u16,
    path: String,
    #[prop(optional)] request_id: Option<String>,
) -> impl IntoView {
    // port: StatusScreen.razor markup + behaviour (placeholder root only)
    view! { <div class="pane"></div> }
}
